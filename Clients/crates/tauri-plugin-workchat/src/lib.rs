//! Mobile glue for WorkChat. On Android the commands are handled by `android/.../WorkChatPlugin.kt`, on iOS by
//! `ios/Sources/WorkChatPlugin.swift`; on desktop the plugin does nothing (the app does not call it there).
//!
//! The app calls it from Rust: `tauri_plugin_workchat::call(&app, "status", json!({}))`.

use serde::de::DeserializeOwned;
use tauri::{plugin::{Builder, TauriPlugin}, Runtime};

#[cfg(target_os = "ios")]
tauri::ios_plugin_binding!(init_plugin_workchat);

#[cfg(mobile)]
pub struct Mobile<R: Runtime>(tauri::plugin::PluginHandle<R>);

/// Runs a plugin command on the platform side. Always an error on desktop.
pub fn call<R: Runtime, T: DeserializeOwned>(app: &tauri::AppHandle<R>, command: &str, args: serde_json::Value) -> Result<T, String> {
    #[cfg(mobile)]
    {
        use tauri::Manager;
        let m = app.try_state::<Mobile<R>>().ok_or_else(|| "mobile plugin not loaded".to_string())?;
        m.0.run_mobile_plugin(command, args).map_err(|e| e.to_string())
    }
    #[cfg(not(mobile))]
    {
        let _ = (app, command, args);
        Err("not available on this platform".to_string())
    }
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("workchat")
        .setup(|app, api| {
            #[cfg(target_os = "android")]
            {
                let handle = api.register_android_plugin("app.workchat", "WorkChatPlugin")?;
                tauri::Manager::manage(app, Mobile(handle));
            }
            #[cfg(target_os = "ios")]
            {
                let handle = api.register_ios_plugin(init_plugin_workchat)?;
                tauri::Manager::manage(app, Mobile(handle));
            }
            #[cfg(not(mobile))]
            {
                let _ = (app, api);
            }
            Ok(())
        })
        .build()
}
