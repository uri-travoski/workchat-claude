package app.workchat

import android.content.Context
import android.os.PowerManager
import org.unifiedpush.android.connector.FailedReason
import org.unifiedpush.android.connector.PushService
import org.unifiedpush.android.connector.data.PushEndpoint
import org.unifiedpush.android.connector.data.PushMessage

/**
 * UnifiedPush receiver (ntfy and others). The server sends a content-free wake-up; we fetch what is new through the
 * core and the core posts the notification itself (sender and text, or "New message" for locked chats).
 */
class WcPushService : PushService() {
    private fun background(block: () -> Unit) {
        val pm = getSystemService(Context.POWER_SERVICE) as PowerManager
        val lock = pm.newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "workchat:push")
        lock.acquire(40_000)
        Thread {
            try {
                Native.setup(this)
                block()
            } catch (_: Throwable) {
            } finally {
                if (lock.isHeld) lock.release()
            }
        }.start()
    }

    override fun onNewEndpoint(endpoint: PushEndpoint, instance: String) {
        Prefs.setEndpoint(this, endpoint.url)
        Prefs.setPushError(this, "")
        background { Native.endpoint(Prefs.dataDir(this), Native.tzMinutes(), "unifiedpush", endpoint.url) }
    }

    override fun onMessage(message: PushMessage, instance: String) {
        background { Native.wake(Prefs.dataDir(this), Native.tzMinutes()) }
    }

    override fun onRegistrationFailed(reason: FailedReason, instance: String) {
        Prefs.setPushError(this, "The push app refused the registration (" + reason.name.lowercase() + ").")
    }

    override fun onUnregistered(instance: String) {
        Prefs.setEndpoint(this, "")
    }
}
