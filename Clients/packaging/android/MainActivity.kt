package app.workchat

import android.content.Intent
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

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        Native.setup(this)
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
    }
}
