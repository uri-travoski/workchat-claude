package app.workchat

import android.content.Intent
import android.net.Uri
import android.os.Build
import android.provider.OpenableColumns
import android.webkit.JavascriptInterface
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.util.UUID
import android.graphics.Color
import android.os.Bundle
import android.view.View
import android.webkit.WebView
import androidx.activity.OnBackPressedCallback
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat

/**
 * Edge-to-edge window with the insets applied as padding (status bar, navigation bar, keyboard), so the page is laid
 * out below the status bar and above the keyboard. The Back button goes through the page (thread, chat, list) and only
 * then moves the app to the background.
 */
class MainActivity : TauriActivity() {
    private var web: WebView? = null

    /** Files and text another app shared to WorkChat, copied into the cache; the page takes them with WcShare.take(). */
    @Volatile
    private var pendingShare: String? = null

    inner class ShareBridge {
        @JavascriptInterface
        fun take(): String {
            val v = pendingShare ?: ""
            pendingShare = null
            return v
        }
    }

    @Suppress("DEPRECATION")
    private fun readShare(i: Intent?) {
        if (i == null || (i.action != Intent.ACTION_SEND && i.action != Intent.ACTION_SEND_MULTIPLE)) return
        val uris = ArrayList<Uri>()
        if (i.action == Intent.ACTION_SEND) {
            (if (Build.VERSION.SDK_INT >= 33) i.getParcelableExtra(Intent.EXTRA_STREAM, Uri::class.java) else i.getParcelableExtra<Uri>(Intent.EXTRA_STREAM))?.let { uris.add(it) }
        } else {
            val l = if (Build.VERSION.SDK_INT >= 33) i.getParcelableArrayListExtra(Intent.EXTRA_STREAM, Uri::class.java) else i.getParcelableArrayListExtra<Uri>(Intent.EXTRA_STREAM)
            l?.let { uris.addAll(it) }
        }
        val text = i.getStringExtra(Intent.EXTRA_TEXT) ?: ""
        i.action = null
        if (uris.isEmpty() && text.isBlank()) return
        // Copying a large video takes a while: not on the UI thread.
        Thread {
            val files = JSONArray()
            val base = File(cacheDir, "shared")
            base.deleteRecursively()
            for (u in uris) {
                try {
                    val dir = File(base, UUID.randomUUID().toString()).also { it.mkdirs() }
                    var name = u.lastPathSegment?.substringAfterLast('/') ?: "file"
                    // The display name is optional: a provider that refuses the query must not stop the copy.
                    try {
                        contentResolver.query(u, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { c ->
                            if (c.moveToFirst()) c.getString(0)?.takeIf { it.isNotBlank() }?.let { name = it }
                        }
                    } catch (_: Throwable) {
                    }
                    name = name.replace('/', '_')
                    val dst = File(dir, name)
                    contentResolver.openInputStream(u)?.use { ins -> dst.outputStream().use { ins.copyTo(it) } } ?: continue
                    files.put(JSONObject().put("path", dst.absolutePath).put("name", name))
                } catch (_: Throwable) {
                }
            }
            pendingShare = JSONObject().put("files", files).put("text", text).toString()
            runOnUiThread { web?.evaluateJavascript("window.__wcShareNudge && window.__wcShareNudge()", null) }
        }.start()
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        Native.setup(this)
        readShare(intent)
        val content = findViewById<View>(android.R.id.content)
        window.decorView.setBackgroundColor(Color.WHITE)
        WindowCompat.getInsetsController(window, window.decorView).isAppearanceLightStatusBars = true
        ViewCompat.setOnApplyWindowInsetsListener(content) { v, insets ->
            val bars = insets.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout() or WindowInsetsCompat.Type.ime())
            v.setPadding(bars.left, bars.top, bars.right, bars.bottom)
            WindowInsetsCompat.CONSUMED
        }
    }

    override fun onWebViewCreate(webView: WebView) {
        web = webView
        webView.addJavascriptInterface(ShareBridge(), "WcShare")
        onBackPressedDispatcher.addCallback(this, object : OnBackPressedCallback(true) {
            override fun handleOnBackPressed() {
                val w = web
                if (w == null) {
                    moveTaskToBack(true)
                    return
                }
                w.evaluateJavascript("window.__wcBack ? window.__wcBack() : false") { r ->
                    if (r != "true") moveTaskToBack(true)
                }
            }
        })
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        readShare(intent)
    }

    override fun onResume() {
        super.onResume()
        // A share can finish copying while the page is not ready yet: ask the page again whenever the app comes forward.
        web?.evaluateJavascript("window.__wcShareNudge && window.__wcShareNudge()", null)
    }
}
