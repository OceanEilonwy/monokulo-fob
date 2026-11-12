package com.monokulo.fob

import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import java.security.MessageDigest

/**
 * The apps Fob knows, for Connect to Fob's verified mark (decision 423): on the same phone Android says which app
 * asked (`getCallingActivity`), and Fob checks that app's signing certificate against the ones it ships. A known
 * package with another certificate (a debug or sideloaded build) is unknown, as is any app Fob doesn't list.
 */
object KnownApps {
    /** An app Fob knows: the name its requests must give, and the SHA-256 of each certificate it's signed with. */
    data class Known(val name: String, val certificates: Set<String>)

    /**
     * By package. Till's release certificate is the owner's to add (its SHA-256, as `apksigner verify
     * --print-certs` gives it); until it's here Till shows as unknown, without the mark.
     */
    val apps: Map<String, Known> = mapOf(
        "com.monokulo.till" to Known("Till", emptySet()),
    )

    /** Whether [caller], the package Android said started Connect to Fob, is the known app named [app]. */
    fun verified(context: Context, caller: String?, app: String): Boolean {
        val known = apps[caller ?: return false] ?: return false
        if (known.name != app || known.certificates.isEmpty()) return false
        return runCatching { certificates(context, caller).any { it in known.certificates } }.getOrDefault(false)
    }

    /** The SHA-256 of each certificate [pkg] is signed with now, lower-case hex. */
    private fun certificates(context: Context, pkg: String): List<String> {
        val pm = context.packageManager
        val signatures = if (Build.VERSION.SDK_INT >= 28) {
            val info = pm.getPackageInfo(pkg, PackageManager.GET_SIGNING_CERTIFICATES).signingInfo ?: return emptyList()
            // A rotated key's history ends with the certificate it's signed with now: only that one counts.
            if (info.hasMultipleSigners()) info.apkContentsSigners.toList() else listOfNotNull(info.signingCertificateHistory.lastOrNull())
        } else {
            @Suppress("DEPRECATION")
            pm.getPackageInfo(pkg, PackageManager.GET_SIGNATURES).signatures?.toList() ?: emptyList()
        }
        return signatures.map { digest(it.toByteArray()) }
    }

    fun digest(bytes: ByteArray): String = MessageDigest.getInstance("SHA-256").digest(bytes).joinToString("") { "%02x".format(it) }
}
