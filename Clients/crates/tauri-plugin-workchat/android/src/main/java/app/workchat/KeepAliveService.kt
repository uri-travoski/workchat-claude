package app.workchat

import android.app.Notification
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import androidx.core.app.NotificationCompat
import androidx.core.content.ContextCompat

/**
 * Fallback for phones without a push distributor: a foreground service keeps the process alive and the core keeps its
 * WebSocket open, so messages notify within seconds. Shows a quiet "connected" notification (Android requires one).
 */
class KeepAliveService : Service() {
    companion object {
        @Volatile
        var running = false

        fun start(c: Context) {
            try {
                ContextCompat.startForegroundService(c, Intent(c, KeepAliveService::class.java))
            } catch (_: Throwable) {
            }
        }

        fun stop(c: Context) {
            c.stopService(Intent(c, KeepAliveService::class.java))
        }
    }

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        Native.setup(this)
        Notifier.ensureKeepAliveChannel(this)
        val n: Notification = NotificationCompat.Builder(this, Notifier.KEEPALIVE_CHANNEL)
            .setSmallIcon(Notifier.smallIcon(this))
            .setContentTitle("WorkChat is connected")
            .setContentText("Keeping the connection open so messages arrive at once.")
            .setOngoing(true)
            .setPriority(NotificationCompat.PRIORITY_MIN)
            .setContentIntent(Notifier.openIntent(this, "workchat://chat/", 7))
            .build()
        try {
            if (Build.VERSION.SDK_INT >= 34) startForeground(7, n, ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE) else startForeground(7, n)
        } catch (_: Throwable) {
            stopSelf()
            return START_NOT_STICKY
        }
        running = true
        val dir = Prefs.dataDir(this)
        Thread { Native.startEngine(dir, Native.tzMinutes()) }.start()
        return START_STICKY
    }

    override fun onDestroy() {
        running = false
        Native.stopEngine()
        super.onDestroy()
    }
}
