package app.workchat

import android.Manifest
import android.app.Activity
import android.content.ContentValues
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.Color
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.provider.MediaStore
import android.provider.OpenableColumns
import android.provider.Settings
import android.webkit.MimeTypeMap
import android.webkit.WebView
import androidx.activity.result.ActivityResult
import androidx.core.content.ContextCompat
import androidx.core.content.FileProvider
import androidx.core.view.WindowCompat
import app.tauri.PermissionState
import app.tauri.annotation.ActivityCallback
import app.tauri.annotation.Command
import app.tauri.annotation.Permission
import app.tauri.annotation.PermissionCallback
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSArray
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import org.unifiedpush.android.connector.UnifiedPush
import java.io.File
import java.util.UUID

/**
 * Everything the Rust core cannot do on Android by itself: runtime permissions, picking files (content URIs become
 * cache files), saving to Downloads, opening files, the APK installer, push registration and the keep-alive service.
 */
@TauriPlugin(
    permissions = [
        Permission(strings = [Manifest.permission.RECORD_AUDIO], alias = "microphone"),
        Permission(strings = [Manifest.permission.CAMERA], alias = "camera"),
        Permission(strings = [Manifest.permission.POST_NOTIFICATIONS], alias = "notifications"),
    ],
)
class WorkChatPlugin(private val activity: Activity) : Plugin(activity) {

    override fun load(webView: WebView) {
        super.load(webView)
        Native.setup(activity)
    }

    private fun ctx() = activity.applicationContext
    private fun authority() = "${ctx().packageName}.workchat.fileprovider"
    private fun argString(i: Invoke, k: String): String? = i.getArgs().let { if (it.has(k) && !it.isNull(k)) it.getString(k) else null }

    // ------------------------------------------------------------------ status and permissions

    private fun notificationState(): String {
        if (Build.VERSION.SDK_INT < 33) return if (Notifier.canPost(ctx())) "granted" else "denied"
        if (ContextCompat.checkSelfPermission(ctx(), Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED) return "granted"
        return if (Prefs.askedNotifications(ctx())) "denied" else "prompt"
    }

    @Command
    fun status(invoke: Invoke) {
        argString(invoke, "dataDir")?.let { Prefs.setDataDir(ctx(), it) }
        val o = JSObject()
        o.put("notifications", notificationState())
        val dists = JSArray()
        try {
            UnifiedPush.getDistributors(ctx()).forEach { dists.put(it) }
        } catch (_: Throwable) {
        }
        o.put("distributors", dists)
        o.put("distributor", try { UnifiedPush.getSavedDistributor(ctx()) ?: "" } catch (_: Throwable) { "" })
        o.put("fcm", Fcm.available())
        o.put("keepAlive", KeepAliveService.running)
        o.put("canInstall", Build.VERSION.SDK_INT < 26 || ctx().packageManager.canRequestPackageInstalls())
        o.put("endpoint", Prefs.endpoint(ctx()))
        o.put("pushError", Prefs.pushError(ctx()))
        invoke.resolve(o)
    }

    @Command
    fun request_permission(invoke: Invoke) {
        val name = argString(invoke, "name") ?: "notifications"
        val alias = when (name) {
            "microphone" -> "microphone"
            "camera" -> "camera"
            else -> "notifications"
        }
        if (alias == "notifications") {
            if (Build.VERSION.SDK_INT < 33) {
                invoke.resolve(JSObject().put("state", notificationState()))
                return
            }
            Prefs.setAskedNotifications(ctx())
        }
        if (getPermissionState(alias) == PermissionState.GRANTED) {
            invoke.resolve(JSObject().put("state", "granted"))
            return
        }
        requestPermissionForAlias(alias, invoke, "permissionDone")
    }

    @PermissionCallback
    fun permissionDone(invoke: Invoke) {
        val alias = when (argString(invoke, "name")) {
            "microphone" -> "microphone"
            "camera" -> "camera"
            else -> "notifications"
        }
        val granted = getPermissionState(alias) == PermissionState.GRANTED
        invoke.resolve(JSObject().put("state", if (granted) "granted" else "denied"))
    }

    // ------------------------------------------------------------------ system bars

    @Command
    fun set_system_bars(invoke: Invoke) {
        val color = argString(invoke, "color") ?: "#ffffff"
        val dark = invoke.getArgs().optBoolean("dark", false)
        activity.runOnUiThread {
            try {
                val c = Color.parseColor(color)
                activity.window.decorView.setBackgroundColor(c)
                val ctl = WindowCompat.getInsetsController(activity.window, activity.window.decorView)
                ctl.isAppearanceLightStatusBars = !dark
                ctl.isAppearanceLightNavigationBars = !dark
            } catch (_: Throwable) {
            }
            invoke.resolve()
        }
    }

    // ------------------------------------------------------------------ push and keep-alive

    @Command
    fun push_register(invoke: Invoke) {
        Thread {
            try {
                val dists = UnifiedPush.getDistributors(ctx())
                if (dists.isNotEmpty()) {
                    val want = argString(invoke, "distributor")?.takeIf { it.isNotEmpty() }
                    val saved = try { UnifiedPush.getSavedDistributor(ctx()) } catch (_: Throwable) { null }
                    val pick = want ?: saved?.takeIf { dists.contains(it) } ?: dists.first()
                    UnifiedPush.saveDistributor(ctx(), pick)
                    Prefs.setPushError(ctx(), "")
                    val before = Prefs.endpoint(ctx())
                    UnifiedPush.register(ctx())
                    // The distributor answers with the endpoint a moment later (WcPushService.onNewEndpoint).
                    var waited = 0
                    while (waited < 8000 && Prefs.endpoint(ctx()).isEmpty() && Prefs.pushError(ctx()).isEmpty()) {
                        Thread.sleep(250)
                        waited += 250
                    }
                    val ep = Prefs.endpoint(ctx())
                    if (ep.isEmpty() && Prefs.pushError(ctx()).isNotEmpty()) {
                        invoke.reject(Prefs.pushError(ctx()))
                    } else {
                        invoke.resolve(JSObject().put("kind", "unifiedpush").put("target", ep).put("was", before))
                    }
                } else if (Fcm.available()) {
                    Fcm.token(ctx()) { t ->
                        if (t == null) invoke.reject("Google push is not available on this phone.")
                        else invoke.resolve(JSObject().put("kind", "fcm").put("target", t))
                    }
                } else {
                    invoke.reject("No push app is installed. Install ntfy (or another UnifiedPush app) first.")
                }
            } catch (e: Throwable) {
                invoke.reject(e.message ?: "Push registration failed.")
            }
        }.start()
    }

    @Command
    fun push_unregister(invoke: Invoke) {
        try {
            UnifiedPush.unregister(ctx())
        } catch (_: Throwable) {
        }
        Prefs.setEndpoint(ctx(), "")
        invoke.resolve()
    }

    @Command
    fun set_keep_alive(invoke: Invoke) {
        val on = invoke.getArgs().optBoolean("on", false)
        argString(invoke, "dataDir")?.let { Prefs.setDataDir(ctx(), it) }
        Prefs.setKeepAlive(ctx(), on)
        if (on) KeepAliveService.start(ctx()) else KeepAliveService.stop(ctx())
        invoke.resolve(JSObject().put("running", on))
    }

    // ------------------------------------------------------------------ the update installer

    @Command
    fun install_apk(invoke: Invoke) {
        val path = argString(invoke, "path") ?: return invoke.reject("No file.")
        try {
            if (Build.VERSION.SDK_INT >= 26 && !ctx().packageManager.canRequestPackageInstalls()) {
                // The person must allow "install unknown apps" for WorkChat first; the settings page opens right on it.
                val i = Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, Uri.parse("package:${ctx().packageName}")).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                activity.startActivity(i)
                invoke.resolve(JSObject().put("state", "needs_permission"))
                return
            }
            val uri = FileProvider.getUriForFile(ctx(), authority(), File(path))
            val i = Intent(Intent.ACTION_VIEW).setDataAndType(uri, "application/vnd.android.package-archive").addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK)
            activity.startActivity(i)
            invoke.resolve(JSObject().put("state", "installing"))
        } catch (e: Throwable) {
            invoke.reject("Could not start the installer: ${e.message}")
        }
    }

    // ------------------------------------------------------------------ files

    private fun mimeOf(name: String): String {
        val ext = name.substringAfterLast('.', "").lowercase()
        return MimeTypeMap.getSingleton().getMimeTypeFromExtension(ext) ?: "application/octet-stream"
    }

    @Command
    fun save_download(invoke: Invoke) {
        val path = argString(invoke, "path") ?: return invoke.reject("No file.")
        val name = argString(invoke, "name") ?: File(path).name
        Thread {
            try {
                val src = File(path)
                if (Build.VERSION.SDK_INT >= 29) {
                    val v = ContentValues()
                    v.put(MediaStore.Downloads.DISPLAY_NAME, name)
                    v.put(MediaStore.Downloads.MIME_TYPE, mimeOf(name))
                    v.put(MediaStore.Downloads.RELATIVE_PATH, Environment.DIRECTORY_DOWNLOADS)
                    v.put(MediaStore.Downloads.IS_PENDING, 1)
                    val res = ctx().contentResolver
                    val uri = res.insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI, v) ?: throw IllegalStateException("Could not create the file in Downloads.")
                    res.openOutputStream(uri)!!.use { out -> src.inputStream().use { it.copyTo(out) } }
                    val done = ContentValues()
                    done.put(MediaStore.Downloads.IS_PENDING, 0)
                    res.update(uri, done, null, null)
                    invoke.resolve(JSObject().put("where", "Downloads"))
                } else {
                    // Older phones: hand the file to the share sheet (save to Drive, Files, and so on).
                    val uri = FileProvider.getUriForFile(ctx(), authority(), src)
                    val send = Intent(Intent.ACTION_SEND).setType(mimeOf(name)).putExtra(Intent.EXTRA_STREAM, uri).addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                    val chooser = Intent.createChooser(send, "Save $name").addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                    activity.startActivity(chooser)
                    invoke.resolve(JSObject().put("where", "the share sheet"))
                }
            } catch (e: Throwable) {
                invoke.reject(e.message ?: "Could not save the file.")
            }
        }.start()
    }

    @Command
    fun view_file(invoke: Invoke) {
        val path = argString(invoke, "path") ?: return invoke.reject("No file.")
        try {
            val f = File(path)
            val uri = FileProvider.getUriForFile(ctx(), authority(), f)
            val i = Intent(Intent.ACTION_VIEW).setDataAndType(uri, mimeOf(f.name)).addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK)
            activity.startActivity(Intent.createChooser(i, "Open ${f.name}").addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
            invoke.resolve()
        } catch (e: Throwable) {
            invoke.reject("No app on this phone can open this file.")
        }
    }

    @Command
    fun pick_files(invoke: Invoke) {
        val kind = argString(invoke, "kind") ?: "any"
        val i = Intent(Intent.ACTION_GET_CONTENT).addCategory(Intent.CATEGORY_OPENABLE).putExtra(Intent.EXTRA_ALLOW_MULTIPLE, true)
        if (kind == "image") {
            i.type = "image/*"
            i.putExtra(Intent.EXTRA_ALLOW_MULTIPLE, false)
        } else if (kind == "media") {
            i.type = "*/*"
            i.putExtra(Intent.EXTRA_MIME_TYPES, arrayOf("image/*", "video/*"))
        } else {
            i.type = "*/*"
        }
        startActivityForResult(invoke, Intent.createChooser(i, if (kind == "image") "Choose a picture" else if (kind == "media") "Choose photos or videos" else "Choose files"), "pickResult")
    }

    private fun displayName(uri: Uri): String {
        try {
            ctx().contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { c ->
                if (c.moveToFirst()) {
                    val n = c.getString(0)
                    if (!n.isNullOrBlank()) return n
                }
            }
        } catch (_: Throwable) {
        }
        return uri.lastPathSegment?.substringAfterLast('/') ?: "file"
    }

    @ActivityCallback
    fun pickResult(invoke: Invoke, result: ActivityResult) {
        if (result.resultCode != Activity.RESULT_OK || result.data == null) {
            invoke.resolve(JSObject().put("files", JSArray()))
            return
        }
        val data = result.data!!
        val uris = ArrayList<Uri>()
        val clip = data.clipData
        if (clip != null) for (n in 0 until clip.itemCount) uris.add(clip.getItemAt(n).uri) else data.data?.let { uris.add(it) }
        // Copying a large video takes a while: not on the UI thread.
        Thread {
            try {
                val out = JSArray()
                val base = File(ctx().cacheDir, "picks")
                base.mkdirs()
                for (u in uris) {
                    val name = displayName(u).replace('/', '_')
                    val dir = File(base, UUID.randomUUID().toString())
                    dir.mkdirs()
                    val dst = File(dir, name)
                    ctx().contentResolver.openInputStream(u)?.use { ins -> dst.outputStream().use { ins.copyTo(it) } } ?: continue
                    out.put(JSObject().put("path", dst.absolutePath).put("name", name))
                }
                invoke.resolve(JSObject().put("files", out))
            } catch (e: Throwable) {
                invoke.reject(e.message ?: "Could not read the chosen file.")
            }
        }.start()
    }
}
