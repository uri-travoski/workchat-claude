package app.workchat

import android.content.Context
import java.util.TimeZone

/**
 * The bridge to the Rust core (libworkchat_lib.so). The core keeps the sync engine; a push wake-up or the keep-alive
 * service runs it without the activity and the core calls [notify] to show what is new.
 */
object Native {
    @Volatile
    var context: Context? = null
    @Volatile
    private var ready = false

    @JvmStatic external fun nativeInit()
    @JvmStatic external fun wake(dir: String, tzMin: Int): Int
    @JvmStatic external fun startEngine(dir: String, tzMin: Int)
    @JvmStatic external fun stopEngine()
    @JvmStatic external fun endpoint(dir: String, tzMin: Int, kind: String, target: String)

    @Synchronized
    fun setup(c: Context) {
        context = c.applicationContext
        if (!ready) {
            System.loadLibrary("workchat_lib")
            nativeInit()
            ready = true
        }
    }

    fun tzMinutes(): Int = TimeZone.getDefault().getOffset(System.currentTimeMillis()) / 60000

    /** Called from Rust (any thread) with the notification to show, as JSON. */
    @JvmStatic
    fun notify(json: String) {
        val c = context ?: return
        try {
            Notifier.post(c, json)
        } catch (_: Throwable) {
        }
    }
}
