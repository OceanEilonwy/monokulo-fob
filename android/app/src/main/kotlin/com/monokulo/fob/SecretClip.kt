package com.monokulo.fob

import android.content.ClipData
import android.content.ClipDescription
import android.content.ClipboardManager
import android.content.Context
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.PersistableBundle
import java.nio.ByteBuffer
import java.nio.CharBuffer

/**
 * A secret key copied from Keys, taken off the clipboard again after a
 * minute, or as soon as Fob leaves the screen, whichever comes first, if
 * it's still the clip Fob put there. Before Android 13 nothing clears it
 * otherwise, and Android 13 waits about an hour.
 */
object SecretClip {
    /** In the clip's extras, so it can be told apart from what anyone else copies. */
    private const val MARK = "com.monokulo.fob.SECRET_CLIP"
    private const val KEEP_MS = 60_000L

    private val main = Handler(Looper.getMainLooper())
    private var manager: ClipboardManager? = null
    /** Fob's clip is on the clipboard as far as Fob can tell. */
    private var ours = false
    private val changed = ClipboardManager.OnPrimaryClipChangedListener { if (!marked()) ours = false }
    private val clearLater = Runnable { clear() }

    /**
     * Copies [secret] (UTF-8, as Rust hands keys over) under [label], marked
     * sensitive so previews don't show it, to be cleared after a minute.
     *
     * The text goes to the clipboard as characters Fob zeroes as soon as it's
     * handed over, never as a String of Fob's own: a JVM String can't be wiped,
     * and stays on the heap until it's collected and its memory reused. What
     * Android does with it on the way (it writes the text into the call to the
     * clipboard service as a String of its own) and keeps on the clipboard is
     * out of Fob's reach, which is why it's taken off again soon after.
     */
    fun copy(context: Context, label: String, secret: ByteArray) {
        val clipboard = context.getSystemService(ClipboardManager::class.java) ?: return
        manager?.removePrimaryClipChangedListener(changed)
        manager = clipboard
        val decoded = Charsets.UTF_8.decode(ByteBuffer.wrap(secret))
        val chars = CharArray(decoded.remaining()).also { decoded.get(it) }
        if (decoded.hasArray()) decoded.array().fill('\u0000')
        try {
            clipboard.setPrimaryClip(
                ClipData.newPlainText(label, CharBuffer.wrap(chars)).apply {
                    // Android 13 and later hide it from the keyboard's and the system's previews; earlier versions ignore it.
                    val sensitive = if (Build.VERSION.SDK_INT >= 33) ClipDescription.EXTRA_IS_SENSITIVE else "android.content.extra.IS_SENSITIVE"
                    description.extras = PersistableBundle().apply {
                        putBoolean(sensitive, true)
                        putBoolean(MARK, true)
                    }
                },
            )
        } finally {
            // The clip is in the clipboard service now; Fob's own copy goes.
            chars.fill('\u0000')
        }
        ours = true
        // Told while Fob is on screen when something else is copied, so that isn't cleared.
        clipboard.addPrimaryClipChangedListener(changed)
        main.removeCallbacks(clearLater)
        main.postDelayed(clearLater, KEEP_MS)
    }

    /**
     * Takes Fob's clip off the clipboard if it's still there. Called a
     * minute after copying and when Fob leaves the screen; from then on
     * nothing is cleared, so a clip made later in another app stays.
     */
    fun clear() {
        main.removeCallbacks(clearLater)
        val clipboard = manager ?: return
        clipboard.removePrimaryClipChangedListener(changed)
        manager = null
        // Android 10 and later may not let Fob read the clipboard once it has
        // lost focus (leaving the screen): then the listener, which ran while
        // Fob was in front, is what says whether it's still Fob's.
        val description = clipboard.primaryClipDescription
        val still = if (description != null) marked(description) else ours
        ours = false
        if (!still) return
        if (Build.VERSION.SDK_INT >= 28) clipboard.clearPrimaryClip() else clipboard.setPrimaryClip(ClipData.newPlainText("", ""))
    }

    private fun marked(description: ClipDescription? = manager?.primaryClipDescription): Boolean =
        description?.extras?.getBoolean(MARK, false) == true
}
