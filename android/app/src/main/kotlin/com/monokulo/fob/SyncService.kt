package com.monokulo.fob

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import uniffi.fob.SyncStatus

/**
 * Keeps Fob running while it catches up, so leaving the app doesn't pause a
 * restore. The syncing is the Rust wallet's; this only holds the process
 * open, shows progress, and stops once caught up.
 */
class SyncService : Service() {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private var watching: Job? = null

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val model = (application as FobApp).model
        running = true
        ServiceCompat.startForeground(this, ID, notification(model.status.value), ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC)
        watching?.cancel()
        watching = scope.launch {
            model.status.collect { status ->
                if (status?.synced == true || !model.catchingUp) {
                    if (!model.visible) model.stopSync()
                    ServiceCompat.stopForeground(this@SyncService, ServiceCompat.STOP_FOREGROUND_REMOVE)
                    stopSelf()
                } else {
                    getSystemService(NotificationManager::class.java).notify(ID, notification(status))
                }
            }
        }
        return START_NOT_STICKY
    }

    /** Android limits data-sync services to six hours a day. */
    override fun onTimeout(startId: Int, fgsType: Int) {
        stopSelf()
    }

    override fun onDestroy() {
        running = false
        scope.cancel()
    }

    private fun notification(status: SyncStatus?): Notification {
        val open = PendingIntent.getActivity(
            this, 0, Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP), PendingIntent.FLAG_IMMUTABLE,
        )
        val percent = ((status?.progress ?: 0f) * 100).toInt()
        val text = status?.error ?: "Fob is catching up · $percent%"
        return NotificationCompat.Builder(this, CHANNEL)
            .setSmallIcon(R.drawable.ic_stat_fob)
            .setContentTitle("Catching up")
            .setContentText(text)
            .setProgress(100, percent, status == null)
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .setSilent(true)
            .setContentIntent(open)
            .setForegroundServiceBehavior(NotificationCompat.FOREGROUND_SERVICE_IMMEDIATE)
            .build()
    }

    companion object {
        private const val ID = 1
        private const val CHANNEL = "sync"

        @Volatile
        private var running = false

        /** Starts holding Fob open while it catches up. Call while Fob is on screen. */
        fun start(context: Context) {
            if (running) return
            val manager = context.getSystemService(NotificationManager::class.java)
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O && manager.getNotificationChannel(CHANNEL) == null) {
                manager.createNotificationChannel(NotificationChannel(CHANNEL, "Catching up", NotificationManager.IMPORTANCE_LOW))
            }
            ContextCompat.startForegroundService(context, Intent(context, SyncService::class.java))
        }
    }
}
