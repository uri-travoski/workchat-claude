const COMMANDS: &[&str] = &["status", "request_permission", "push_register", "push_unregister", "set_keep_alive", "install_apk", "save_download", "view_file", "pick_files", "ios_register", "set_system_bars"];

fn main() {
    tauri_plugin::Builder::new(COMMANDS).android_path("android").ios_path("ios").build();
}
