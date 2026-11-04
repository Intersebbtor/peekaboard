//! The two windows: the main window (the Swift app's menu bar popover) and
//! the HUD (its floating panel).

use tauri::{
    AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder, WindowEvent,
};

use crate::hub::Hub;
use crate::settings::HudGeometry;

pub const HUD_DEFAULT_SIZE: (f64, f64) = (700.0, 400.0);
pub const HUD_MIN_SIZE: (f64, f64) = (280.0, 140.0);

pub fn create_main(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let window = WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
        .title("ZMK Keymap Viewer")
        .inner_size(740.0, 500.0)
        .min_inner_size(560.0, 380.0)
        .center()
        .build()?;

    // Closing the window keeps the app in the tray, like closing the popover
    let w = window.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            let _ = w.hide();
        }
    });
    Ok(window)
}

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

pub fn toggle_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        if w.is_visible().unwrap_or(false) && w.is_focused().unwrap_or(false) {
            let _ = w.hide();
        } else {
            show_main(app);
        }
    }
}

/// The HUD: transparent, always on top, no taskbar entry, and it never takes
/// the keyboard focus from the app you are typing in.
pub fn create_hud(hub: &Hub, geometry: Option<HudGeometry>) -> tauri::Result<WebviewWindow> {
    let window = WebviewWindowBuilder::new(&hub.app, "hud", WebviewUrl::App("hud.html".into()))
        .title("ZMK Keymap HUD")
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(true)
        .focused(false)
        .focusable(false)
        .visible(false)
        .inner_size(HUD_DEFAULT_SIZE.0, HUD_DEFAULT_SIZE.1)
        .min_inner_size(HUD_MIN_SIZE.0, HUD_MIN_SIZE.1)
        .build()?;

    match geometry {
        Some(g) => {
            let _ = window.set_size(PhysicalSize::new(g.width, g.height));
            let _ = window.set_position(PhysicalPosition::new(g.x, g.y));
        }
        None => {
            let _ = window.center();
        }
    }

    // Remember where the user put it
    let hub = hub.clone();
    let w = window.clone();
    window.on_window_event(move |event| {
        if matches!(event, WindowEvent::Moved(_) | WindowEvent::Resized(_)) {
            if let (Ok(pos), Ok(size)) = (w.outer_position(), w.inner_size()) {
                // A hidden or minimized window reports nonsense positions
                if size.width < 100 || pos.x < -30000 {
                    return;
                }
                let mut s = hub.lock();
                s.settings.hud_geometry = Some(HudGeometry {
                    x: pos.x,
                    y: pos.y,
                    width: size.width,
                    height: size.height,
                });
                s.save_settings();
            }
        }
    });
    Ok(window)
}

pub fn hud_visible(app: &AppHandle) -> bool {
    app.get_webview_window("hud")
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false)
}

pub fn set_hud_visible(app: &AppHandle, visible: bool) {
    if let Some(w) = app.get_webview_window("hud") {
        let _ = if visible { w.show() } else { w.hide() };
    }
}
