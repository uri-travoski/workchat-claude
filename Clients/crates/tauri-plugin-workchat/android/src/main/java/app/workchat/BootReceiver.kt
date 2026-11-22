package app.workchat

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent

/** Starts the keep-alive service again after a reboot or an app update, when the person turned it on. */
class BootReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (Prefs.keepAlive(context)) KeepAliveService.start(context)
    }
}
