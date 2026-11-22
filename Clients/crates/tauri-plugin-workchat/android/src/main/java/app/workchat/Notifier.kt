package app.workchat

import android.Manifest
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.media.AudioAttributes
import android.net.Uri
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import org.json.JSONObject

/** Posts message notifications. Locked chats arrive from the core already reduced to "New message". */
object Notifier {
    const val KEEPALIVE_CHANNEL = "keepalive"

    private fun channelId(sound: String, vibrate: Boolean) = "msg_${sound}_${if (vibrate) "v" else "q"}"

    private fun ensureChannel(c: Context, sound: String, vibrate: Boolean): String {
        val id = channelId(sound, vibrate)
        if (Build.VERSION.SDK_INT >= 26) {
            val nm = c.getSystemService(NotificationManager::class.java)
            if (nm.getNotificationChannel(id) == null) {
                val importance = if (sound == "none") NotificationManager.IMPORTANCE_LOW else NotificationManager.IMPORTANCE_HIGH
                val ch = NotificationChannel(id, "Messages" + (if (sound == "gentle") " (gentle)" else if (sound == "none") " (silent)" else ""), importance)
                ch.description = "New messages"
                ch.enableVibration(vibrate)
                when (sound) {
                    "none" -> ch.setSound(null, null)
                    "gentle" -> {
                        val res = c.resources.getIdentifier("gentle", "raw", c.packageName)
                        if (res != 0) {
                            ch.setSound(
                                Uri.parse("android.resource://${c.packageName}/$res"),
                                AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_NOTIFICATION).setContentType(AudioAttributes.CONTENT_TYPE_SONIFICATION).build(),
                            )
                        }
                    }
                }
                nm.createNotificationChannel(ch)
            }
        }
        return id
    }

    fun ensureKeepAliveChannel(c: Context) {
        if (Build.VERSION.SDK_INT >= 26) {
            val nm = c.getSystemService(NotificationManager::class.java)
            if (nm.getNotificationChannel(KEEPALIVE_CHANNEL) == null) {
                val ch = NotificationChannel(KEEPALIVE_CHANNEL, "Connection", NotificationManager.IMPORTANCE_MIN)
                ch.description = "Shown while WorkChat keeps its connection open to receive messages"
                ch.setShowBadge(false)
                nm.createNotificationChannel(ch)
            }
        }
    }

    fun smallIcon(c: Context): Int {
        val id = c.resources.getIdentifier("ic_stat_workchat", "drawable", c.packageName)
        return if (id != 0) id else android.R.drawable.ic_dialog_email
    }

    fun openIntent(c: Context, uri: String, req: Int): PendingIntent {
        val i = Intent(Intent.ACTION_VIEW, Uri.parse(uri)).setPackage(c.packageName).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP)
        return PendingIntent.getActivity(c, req, i, PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
    }

    fun canPost(c: Context): Boolean {
        if (Build.VERSION.SDK_INT >= 33 && ContextCompat.checkSelfPermission(c, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) return false
        return NotificationManagerCompat.from(c).areNotificationsEnabled()
    }

    fun post(c: Context, json: String) {
        if (!canPost(c)) return
        val o = JSONObject(json)
        val conv = o.optString("conv_id")
        val sound = o.optString("sound", "default")
        val vibrate = o.optBoolean("vibrate", true)
        val channel = ensureChannel(c, sound, vibrate)
        val uri = if (conv.isNotEmpty()) "workchat://chat/$conv" else "workchat://chat/"
        val id = if (conv.isNotEmpty()) conv.hashCode() else 1
        val n = NotificationCompat.Builder(c, channel)
            .setSmallIcon(smallIcon(c))
            .setContentTitle(o.optString("title", "WorkChat"))
            .setContentText(o.optString("body"))
            .setStyle(NotificationCompat.BigTextStyle().bigText(o.optString("body")))
            .setAutoCancel(true)
            .setCategory(NotificationCompat.CATEGORY_MESSAGE)
            .setVisibility(if (o.optBoolean("locked")) NotificationCompat.VISIBILITY_PRIVATE else NotificationCompat.VISIBILITY_PUBLIC)
            .setPriority(if (sound == "none") NotificationCompat.PRIORITY_LOW else NotificationCompat.PRIORITY_HIGH)
            .setContentIntent(openIntent(c, uri, id))
            .setColor(0xFF21375B.toInt())
            .build()
        try {
            NotificationManagerCompat.from(c).notify(conv.ifEmpty { "wc" }, id, n)
        } catch (_: SecurityException) {
        }
    }
}
