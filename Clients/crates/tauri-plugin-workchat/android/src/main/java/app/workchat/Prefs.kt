package app.workchat

import android.content.Context

/** Small settings the Kotlin side needs while the app is not running (push endpoint, keep-alive switch, data folder). */
object Prefs {
    private fun sp(c: Context) = c.getSharedPreferences("workchat_native", Context.MODE_PRIVATE)

    fun dataDir(c: Context): String = sp(c).getString("dataDir", null) ?: c.dataDir.absolutePath
    fun setDataDir(c: Context, v: String) = sp(c).edit().putString("dataDir", v).apply()

    fun keepAlive(c: Context): Boolean = sp(c).getBoolean("keepAlive", false)
    fun setKeepAlive(c: Context, v: Boolean) = sp(c).edit().putBoolean("keepAlive", v).apply()

    fun endpoint(c: Context): String = sp(c).getString("endpoint", "") ?: ""
    fun setEndpoint(c: Context, v: String) = sp(c).edit().putString("endpoint", v).apply()

    fun pushError(c: Context): String = sp(c).getString("pushError", "") ?: ""
    fun setPushError(c: Context, v: String) = sp(c).edit().putString("pushError", v).apply()

    /** Notification permission asked at least once (to tell "not asked yet" from "blocked"). */
    fun askedNotifications(c: Context): Boolean = sp(c).getBoolean("askedNotifications", false)
    fun setAskedNotifications(c: Context) = sp(c).edit().putBoolean("askedNotifications", true).apply()
}
