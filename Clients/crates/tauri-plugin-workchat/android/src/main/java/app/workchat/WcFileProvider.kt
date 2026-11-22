package app.workchat

import androidx.core.content.FileProvider

/** Its own class so it can sit next to the FileProvider Tauri generates for the webview (distinct manifest entries). */
class WcFileProvider : FileProvider()
