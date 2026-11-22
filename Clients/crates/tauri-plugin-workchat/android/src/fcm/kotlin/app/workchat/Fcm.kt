package app.workchat

import android.content.Context
import com.google.firebase.messaging.FirebaseMessaging
import com.google.firebase.messaging.FirebaseMessagingService
import com.google.firebase.messaging.RemoteMessage

/** Compiled only when the app module has a google-services.json (see build.gradle.kts). */
object Fcm {
    fun available(): Boolean = true
    fun token(c: Context, cb: (String?) -> Unit) {
        try {
            FirebaseMessaging.getInstance().token.addOnCompleteListener { t -> cb(if (t.isSuccessful) t.result else null) }
        } catch (_: Throwable) {
            cb(null)
        }
    }
}

class FcmService : FirebaseMessagingService() {
    override fun onNewToken(token: String) {
        Thread {
            Native.setup(this)
            Native.endpoint(Prefs.dataDir(this), Native.tzMinutes(), "fcm", token)
        }.start()
    }

    override fun onMessageReceived(message: RemoteMessage) {
        // A content-free wake-up (data type=wake): fetch what is new; the core posts the notification.
        Thread {
            Native.setup(this)
            Native.wake(Prefs.dataDir(this), Native.tzMinutes())
        }.start()
    }
}
