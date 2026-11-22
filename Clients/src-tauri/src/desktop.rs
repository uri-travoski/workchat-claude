//! Desktop shell: tray icon with an unread badge, close-to-tray, OS notifications (with click to open the chat).

use crate::notify::Notice;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;
use tauri::{image::Image, menu::{Menu, MenuItem}, tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent}, AppHandle, Emitter, Manager};

pub static CLOSE_TO_TRAY: AtomicBool = AtomicBool::new(false);
static TRAY_OK: AtomicBool = AtomicBool::new(false);
static LAST_BADGE: AtomicU32 = AtomicU32::new(0);

pub struct Tray {
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
        let mut b = TrayIconBuilder::with_id("main").tooltip("WorkChat").menu(&menu).show_menu_on_left_click(false);
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

const DIGITS: [&str; 11] = ["111101101101111", "010110010010111", "111001111100111", "111001111001111", "101101111001001", "111100111001111", "111100111101111", "111001001001001", "111101111101111", "111101111001111", "000010111010000"];

/// The app icon with a red dot and the count (1 to 9, "+" above) in its top right corner.
pub fn badge_rgba(base: &[u8], w: u32, h: u32, n: u32) -> Vec<u8> {
    let mut px = base.to_vec();
    if n == 0 {
        return px;
    }
    let r = (w.min(h) as f32 * 0.30).max(4.0);
    let (cx, cy) = (w as f32 - r - 1.0, r + 1.0);
    for y in 0..h {
        for x in 0..w {
            let d = (((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2))).sqrt();
            if d <= r {
                let i = ((y * w + x) * 4) as usize;
                let edge = if d > r - 1.0 { 0.0 } else { 1.0 };
                let white = d > r - 1.6 && d <= r - 0.6;
                let c = if white { [255u8, 255, 255] } else { [229, 50, 45] };
                if edge > 0.0 || white || d <= r - 1.0 {
                    px[i..i + 3].copy_from_slice(&c);
                    px[i + 3] = 255;
                }
            }
        }
    }
    let glyph = DIGITS[if n > 9 { 10 } else { n as usize }].as_bytes();
    let scale = ((r * 2.0 * 0.5) / 5.0).floor().max(1.0) as u32;
    let (gw, gh) = (3 * scale, 5 * scale);
    let (ox, oy) = (cx as i32 - gw as i32 / 2, cy as i32 - gh as i32 / 2);
    for gy in 0..5u32 {
        for gx in 0..3u32 {
            if glyph[(gy * 3 + gx) as usize] == b'1' {
                for sy in 0..scale {
                    for sx in 0..scale {
                        let (x, y) = (ox + (gx * scale + sx) as i32, oy + (gy * scale + sy) as i32);
                        if x >= 0 && y >= 0 && (x as u32) < w && (y as u32) < h {
                            let i = ((y as u32 * w + x as u32) * 4) as usize;
                            px[i..i + 4].copy_from_slice(&[255, 255, 255, 255]);
                        }
                    }
                }
            }
        }
    }
    px
}

pub fn set_badge(app: &AppHandle, n: u32) {
    if LAST_BADGE.swap(n, Ordering::SeqCst) == n && n != 0 {
        return;
    }
    let Some(t) = app.try_state::<Tray>() else { return };
    let guard = t.icon.lock().unwrap_or_else(|e| e.into_inner());
    let Some(tray) = guard.as_ref() else { return };
    let _ = tray.set_tooltip(Some(if n == 0 { "WorkChat".to_string() } else { format!("WorkChat - {n} unread") }));
    if let Some(i) = app.default_window_icon() {
        let rgba = badge_rgba(i.rgba(), i.width(), i.height(), n);
        let _ = tray.set_icon(Some(Image::new_owned(rgba, i.width(), i.height())));
    }
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.set_title(&if n == 0 { "WorkChat".to_string() } else { format!("({n}) WorkChat") });
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
    fn badge_draws_a_dot_in_the_corner() {
        let base = vec![0u8; 32 * 32 * 4];
        assert_eq!(badge_rgba(&base, 32, 32, 0), base, "no unread: untouched icon");
        let img = badge_rgba(&base, 32, 32, 3);
        let px = |x: usize, y: usize| &img[(y * 32 + x) * 4..(y * 32 + x) * 4 + 4];
        assert_eq!(px(1, 30), &[0, 0, 0, 0], "far corner stays transparent");
        assert!(img.chunks(4).any(|p| p == [229, 50, 45, 255]), "red dot");
        assert!(img.chunks(4).any(|p| p == [255, 255, 255, 255]), "white digit");
        assert_ne!(badge_rgba(&base, 32, 32, 3), badge_rgba(&base, 32, 32, 12));
    }
}
