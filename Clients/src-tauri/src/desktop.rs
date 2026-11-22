//! Desktop shell: tray icon (plain app icon, no badge), close-to-tray, OS notifications (with click to open the chat).

use crate::notify::Notice;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;
use tauri::{menu::{Menu, MenuItem}, tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent}, AppHandle, Emitter, Manager};

pub static CLOSE_TO_TRAY: AtomicBool = AtomicBool::new(false);
static TRAY_OK: AtomicBool = AtomicBool::new(false);
static LAST_BADGE: AtomicU32 = AtomicU32::new(0);

pub struct Tray {
    #[allow(dead_code)] // kept alive: dropping the handle removes the tray icon
    icon: Mutex<Option<TrayIcon>>,
}

pub fn tray_ok() -> bool {
    TRAY_OK.load(Ordering::SeqCst)
}

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

pub fn open_chat(app: &AppHandle, conv: &str) {
    show_main(app);
    let _ = app.emit("wc_open_chat", conv.to_string());
}

pub fn setup_tray(app: &tauri::App) {
    let handle = app.handle().clone();
    let build = || -> tauri::Result<TrayIcon> {
        let open = MenuItem::with_id(app, "open", "Open WorkChat", true, None::<&str>)?;
        let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
        let menu = Menu::with_items(app, &[&open, &quit])?;
        let mut b = TrayIconBuilder::with_id("main").tooltip(TRAY_TOOLTIP).menu(&menu).show_menu_on_left_click(false);
        if let Some(i) = app.default_window_icon() {
            b = b.icon(i.clone());
        }
        b.on_menu_event(|app, ev| match ev.id().as_ref() {
            "open" => show_main(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, ev| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = ev {
                show_main(tray.app_handle());
            }
        })
        .build(app)
    };
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(build)) {
        Ok(Ok(t)) => {
            TRAY_OK.store(true, Ordering::SeqCst);
            handle.manage(Tray { icon: Mutex::new(Some(t)) });
        }
        Ok(Err(e)) => tracing::warn!("no tray icon: {e}"),
        Err(_) => tracing::warn!("no tray icon (the tray library failed)"),
    }
}

/// The tray shows only the plain app icon and this tooltip, always: no unread badge, no count overlay, no count in the tooltip.
pub const TRAY_TOOLTIP: &str = "WorkChat";

/// The main window's title (the tray is unaffected): a count in front while something is unread.
pub fn window_title(n: u32) -> String {
    if n == 0 {
        "WorkChat".to_string()
    } else {
        format!("({n}) WorkChat")
    }
}

/// Called when the unread count changes. Only the window title follows it; the tray icon never changes.
pub fn set_badge(app: &AppHandle, n: u32) {
    if LAST_BADGE.swap(n, Ordering::SeqCst) == n && n != 0 {
        return;
    }
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.set_title(&window_title(n));
    }
}

/// An OS notification. Clicking it brings the window up on that chat.
pub fn show_notice(app: &AppHandle, n: Notice) {
    #[cfg(target_os = "linux")]
    {
        let app = app.clone();
        std::thread::spawn(move || {
            let mut nt = notify_rust::Notification::new();
            nt.summary(&n.title).body(&n.body).appname("WorkChat").icon("app.workchat.WorkChat").action("default", "Open").hint(notify_rust::Hint::Category("im.received".into()));
            if n.sound == "none" {
                nt.hint(notify_rust::Hint::SuppressSound(true));
            } else {
                nt.sound_name(if n.sound == "gentle" { "message-new-instant" } else { "message-new-instant" });
            }
            if let Ok(h) = nt.show() {
                let conv = n.conv_id.clone();
                h.wait_for_action(move |a| {
                    if a == "default" || a == "clicked" {
                        open_chat(&app, &conv);
                    }
                });
            }
        });
    }
    #[cfg(not(target_os = "linux"))]
    {
        use tauri_plugin_notification::NotificationExt;
        let mut b = app.notification().builder().title(&n.title).body(&n.body);
        if n.sound != "none" {
            b = b.sound(if cfg!(windows) { "Default" } else { "default" });
        }
        let _ = b.show();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tray_tooltip_is_plain_and_never_carries_a_count() {
        assert_eq!(TRAY_TOOLTIP, "WorkChat");
        assert!(!TRAY_TOOLTIP.chars().any(|c| c.is_ascii_digit()));
    }

    #[test]
    fn window_title_counts_unread_but_the_tray_does_not() {
        assert_eq!(window_title(0), "WorkChat");
        assert_eq!(window_title(3), "(3) WorkChat");
    }
}
