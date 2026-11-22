package app.workchat

import android.app.Service
import android.content.Context
import android.content.Intent
import android.os.IBinder

/** This build has no google-services.json: no Google push. (A disabled service keeps the manifest entry valid.) */
object Fcm {
    fun available(): Boolean = false
    fun token(c: Context, cb: (String?) -> Unit) = cb(null)
}

class FcmService : Service() {
    override fun onBind(intent: Intent?): IBinder? = null
}
