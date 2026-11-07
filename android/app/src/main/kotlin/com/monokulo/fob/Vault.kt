package com.monokulo.fob

import android.content.Context
import android.os.Build
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.security.keystore.KeyPermanentlyInvalidatedException
import android.security.keystore.UserNotAuthenticatedException
import android.util.Base64
import java.security.GeneralSecurityException
import java.security.KeyStore
import java.security.ProviderException
import java.security.UnrecoverableKeyException
import java.security.SecureRandom
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * The 32-byte key the Rust library seals the wallet file with. It's random,
 * and kept wrapped by an AES key that never leaves the Android Keystore, so
 * a copy of the app's files alone can't open the wallet.
 *
 * The wrapping key lives in StrongBox where the phone has one, and on Android
 * 15 (API 35) and later can only decrypt while the phone is unlocked. Before
 * Android 15 an unlocked-device-required key was tied to the lock screen's own
 * key, and some phones lost it for good when the screen lock was removed or
 * changed. That would lose the file key, and with it what only this phone
 * keeps (contacts, notes, receipts, signed payments not yet seen on chain);
 * the funds come back from the recovery words, those don't. So below API 35
 * the wrapping key is a plain one, StrongBox-backed where it can be.
 *
 * It is deliberately not bound to the lock prompt Fob shows before Backup,
 * Keys and Send: that prompt is a check in the app, not a key operation, so
 * it does not stop code running as Fob from unwrapping the file key. Binding
 * the key to it would stop the wallet opening for syncing, which happens
 * without a prompt.
 */
class Vault(context: Context) {
    private val prefs = context.getSharedPreferences("vault", Context.MODE_PRIVATE)

    private fun store() = KeyStore.getInstance(KEYSTORE).apply { load(null) }

    /**
     * The wrapping key, made the first time. A key's settings are fixed when
     * it's made, so the hardened one has its own alias and the old one is
     * only ever read, to move its file key over.
     */
    private fun keystoreKey(): SecretKey {
        readable { store().getKey(ALIAS, null) as? SecretKey }?.let { return it }
        // Strongest first; each later try drops what some phones refuse. An
        // unlocked-device-required key is only asked for where it can't be
        // lost with the screen lock (see the class comment).
        val unlockedSafe = Build.VERSION.SDK_INT >= UNLOCKED_SAFE_API
        val tries = listOf(true to unlockedSafe, false to unlockedSafe, false to false).distinct()
        for ((index, attempt) in tries.withIndex()) {
            val (strongBox, unlocked) = attempt
            try {
                val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, KEYSTORE)
                generator.init(spec(strongBox, unlocked))
                return generator.generateKey()
            } catch (e: Exception) {
                // A refusal can come from init (InvalidAlgorithmParameterException,
                // a GeneralSecurityException) or from generateKey (ProviderException,
                // StrongBoxUnavailableException among them); either way the next,
                // plainer try may work. Anything else is a real fault.
                if (e !is GeneralSecurityException && e !is ProviderException) throw e
                runCatching { store().deleteEntry(ALIAS) }
                if (index == tries.lastIndex) throw e
            }
        }
        error("unreachable")
    }

    private fun spec(strongBox: Boolean, unlocked: Boolean): KeyGenParameterSpec {
        val builder = KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
            .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
            .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
            .setKeySize(256)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            if (unlocked) builder.setUnlockedDeviceRequired(true)
            if (strongBox) builder.setIsStrongBoxBacked(true)
        }
        return builder.build()
    }

    private fun unwrap(stored: String, key: SecretKey): ByteArray {
        val bytes = Base64.decode(stored, Base64.NO_WRAP)
        val cipher = Cipher.getInstance(TRANSFORMATION)
        readable { cipher.init(Cipher.DECRYPT_MODE, key, GCMParameterSpec(128, bytes, 0, IV)) }
        return cipher.doFinal(bytes, IV, bytes.size - IV)
    }

    /**
     * Runs [read], turning the Keystore's word that a key is gone for good
     * into a [KeyLost] that says so, rather than a bare crypto exception.
     */
    private inline fun <T> readable(read: () -> T): T = try {
        read()
    } catch (e: KeyPermanentlyInvalidatedException) {
        throw KeyLost(e)
    } catch (e: UnrecoverableKeyException) {
        throw KeyLost(e)
    } catch (e: UserNotAuthenticatedException) {
        // A key that needs the device unlocked, asked for while it's locked
        // (Android 15+): not lost, just not now.
        throw DeviceLocked(e)
    }

    /** The key only opens while the phone is unlocked; this was asked for while it wasn't. */
    class DeviceLocked(cause: Throwable) : GeneralSecurityException(
        "Unlock your phone, then try again.",
        cause,
    )

    /** The phone's Keystore can no longer open the key Fob's local files are sealed with. */
    class KeyLost(cause: Throwable) : GeneralSecurityException(
        "This phone's keystore can no longer open Fob's wallet files, maybe after the screen lock changed. " +
            "Your funds are safe: restore the wallet from its recovery words.",
        cause,
    )

    private fun wrap(fileKey: ByteArray, key: SecretKey): String {
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(Cipher.ENCRYPT_MODE, key)
        return Base64.encodeToString(cipher.iv + cipher.doFinal(fileKey), Base64.NO_WRAP)
    }

    /** Runs [use] with the wallet file key, zeroing this copy of it after. */
    inline fun <T> withFileKey(use: (ByteArray) -> T): T {
        val key = fileKey()
        try {
            return use(key)
        } finally {
            key.fill(0)
        }
    }

    /** The wallet file key, made the first time it's asked for; prefer [withFileKey]. */
    fun fileKey(): ByteArray {
        prefs.getString(PREF, null)?.let { stored ->
            // A file key wrapped before needs the key it was wrapped with: making
            // a new one here would only fail to unwrap, less clearly.
            val key = readable { store().getKey(ALIAS, null) as? SecretKey } ?: throw KeyLost(UnrecoverableKeyException("no key under $ALIAS"))
            return unwrap(stored, key)
        }
        prefs.getString(OLD_PREF, null)?.let { stored ->
            val oldKey = readable { store().getKey(OLD_ALIAS, null) as? SecretKey } ?: throw KeyLost(UnrecoverableKeyException("no key under $OLD_ALIAS"))
            val key = unwrap(stored, oldKey)
            moveOver(key)
            return key
        }
        val key = ByteArray(32).also(SecureRandom()::nextBytes)
        prefs.edit().putString(PREF, wrap(key, keystoreKey())).apply()
        return key
    }

    /**
     * Rewraps a file key made before the hardened wrapping key. The old copy
     * goes only once the new one is written and known to unwrap, so a failure
     * here (a locked phone, say) leaves things as they were, to try next time.
     */
    private fun moveOver(fileKey: ByteArray) {
        try {
            val key = keystoreKey()
            val sealed = wrap(fileKey, key)
            if (!unwrap(sealed, key).contentEquals(fileKey)) return
            if (!prefs.edit().putString(PREF, sealed).commit()) return
            prefs.edit().remove(OLD_PREF).commit()
            store().deleteEntry(OLD_ALIAS)
        } catch (_: Exception) {
        }
    }

    private companion object {
        const val KEYSTORE = "AndroidKeyStore"
        const val ALIAS = "fob-wallet-file-2"
        const val PREF = "file-key-2"
        /** Before the wrapping key needed an unlocked phone. */
        const val OLD_ALIAS = "fob-wallet-file"
        const val OLD_PREF = "file-key"
        const val TRANSFORMATION = "AES/GCM/NoPadding"
        /** Android 15, where an unlocked-device-required key outlives a screen lock change. */
        const val UNLOCKED_SAFE_API = 35
        const val IV = 12
    }
}
