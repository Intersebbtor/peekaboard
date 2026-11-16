//! The two windows: the main window (the Swift app's menu bar popover) and
//! the HUD (its floating panel).

use tauri::{
    AppHandle, LogicalPosition, LogicalSize, Manager, PhysicalPosition, PhysicalSize, WebviewUrl,
    WebviewWindow, WebviewWindowBuilder, WindowEvent,
};

use crate::hub::Hub;
use crate::settings::HudGeometry;

pub const HUD_DEFAULT_SIZE: (f64, f64) = (700.0, 400.0);
pub const HUD_MIN_SIZE: (f64, f64) = (280.0, 140.0);

// On macOS the HUD is an NSPanel that never activates the app, like the Swift
// app's FloatingPanel: clicking it leaves the focus in the app you type in.
#[cfg(target_os = "macos")]
tauri_nspanel::tauri_panel! {
    panel!(HudPanel {
        config: {
            can_become_key_window: false,
            is_floating_panel: true
        }
    })
}

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
pub fn create_hud(hub: &Hub, geometry: Option<HudGeometry>, use_material: bool) -> tauri::Result<WebviewWindow> {
    let builder = WebviewWindowBuilder::new(&hub.app, "hud", WebviewUrl::App("hud.html".into()))
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
        .min_inner_size(HUD_MIN_SIZE.0, HUD_MIN_SIZE.1);

    let window = builder.build()?;

    match geometry {
        Some(g) => {
            let _ = window.set_size(PhysicalSize::new(g.width, g.height));
            let _ = window.set_position(PhysicalPosition::new(g.x, g.y));
        }
        None => {
            let _ = window.center();
        }
    }

    #[cfg(target_os = "macos")]
    {
        make_panel(&window)?;
        round_corners(&window);
        // After the panel conversion: a material set before it is lost
        set_material(&window, use_material);
    }
    #[cfg(not(target_os = "macos"))]
    let _ = use_material;

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

#[cfg(target_os = "macos")]
fn hud_effects() -> tauri::utils::config::WindowEffectsConfig {
    tauri::window::EffectsBuilder::new()
        .effect(tauri::window::Effect::HudWindow)
        .state(tauri::window::EffectState::Active)
        .radius(HUD_CORNER_RADIUS)
        .build()
}

/// The Swift HUD's corner radius
#[cfg(target_os = "macos")]
const HUD_CORNER_RADIUS: f64 = 24.0;

/// Clips the whole window, material included, to rounded corners. The page
/// alone can only round what it draws, the material behind it stays square.
#[cfg(target_os = "macos")]
fn round_corners(window: &WebviewWindow) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;
    use objc2_foundation::NSString;

    let Ok(ns_window) = window.ns_window() else { return };
    let ns_window = ns_window as *mut AnyObject;
    // SAFETY: called on the main thread (setup) with the live NSWindow
    unsafe {
        let view: *mut AnyObject = msg_send![ns_window, contentView];
        if view.is_null() {
            return;
        }
        let _: () = msg_send![view, setWantsLayer: true];
        let layer: *mut AnyObject = msg_send![view, layer];
        if layer.is_null() {
            return;
        }
        let _: () = msg_send![layer, setCornerRadius: HUD_CORNER_RADIUS];
        // Like SwiftUI's RoundedRectangle(style: .continuous)
        let curve = NSString::from_str("continuous");
        let _: () = msg_send![layer, setCornerCurve: &*curve];
        let _: () = msg_send![layer, setMasksToBounds: true];
        // The Swift panel has a shadow; it follows the rounded shape
        let _: () = msg_send![ns_window, setHasShadow: true];
        let _: () = msg_send![ns_window, invalidateShadow];
    }
}

/// The background blur, or none (the Swift setting "Use Background Blur")
#[cfg(target_os = "macos")]
fn set_material(window: &WebviewWindow, on: bool) {
    let effects = if on { Some(hud_effects()) } else { None };
    let _ = window.set_effects(effects);
}

pub fn set_hud_material(app: &AppHandle, on: bool) {
    #[cfg(target_os = "macos")]
    {
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || {
            if let Some(w) = handle.get_webview_window("hud") {
                set_material(&w, on);
            }
        });
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (app, on);
}

#[cfg(target_os = "macos")]
fn make_panel(window: &WebviewWindow) -> tauri::Result<()> {
    use tauri_nspanel::objc2_app_kit::NSWindowStyleMask;
    use tauri_nspanel::{CollectionBehavior, PanelLevel, WebviewWindowExt};

    let panel = window.to_panel::<HudPanel>()?;
    if let Err(e) = panel.add_style_mask(NSWindowStyleMask::NonactivatingPanel) {
        eprintln!("[HUD] Could not make the panel non-activating: {e:?}");
    }
    panel.set_level(PanelLevel::Floating.value());
    // On every Space and over full-screen apps
    panel.set_collection_behavior(
        CollectionBehavior::new()
            .can_join_all_spaces()
            .full_screen_auxiliary()
            .into(),
    );
    panel.set_hides_on_deactivate(false);
    // A transparent window lets clicks through wherever it draws nothing
    // itself, and the web view draws in a layer of its own: without this
    // every click would land in the window behind the HUD
    panel.set_ignores_mouse_events(false);
    Ok(())
}

pub fn set_hud_visible(app: &AppHandle, visible: bool) {
    #[cfg(target_os = "macos")]
    {
        use tauri_nspanel::ManagerExt;
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || {
            if let Ok(panel) = handle.get_webview_panel("hud") {
                if visible {
                    panel.show();
                } else {
                    panel.hide();
                }
            }
        });
    }
    #[cfg(not(target_os = "macos"))]
    if let Some(w) = app.get_webview_window("hud") {
        let _ = if visible { w.show() } else { w.hide() };
    }
}

/// The whole HUD window fades on macOS, material included (the Swift HUD
/// fades its view). Elsewhere the page fades itself. Animated here, not by
/// the page: a web view in an app that is not in front may pause its
/// animation frames, and the HUD would stay where it was.
pub fn fade_hud(app: &AppHandle, target: f64) {
    #[cfg(target_os = "macos")]
    {
        use std::sync::atomic::{AtomicU64, Ordering};
        use std::time::{Duration, Instant};

        // The newest fade wins; the alpha in between, as f64 bits
        static GENERATION: AtomicU64 = AtomicU64::new(0);
        static CURRENT: AtomicU64 = AtomicU64::new(0x3FF0_0000_0000_0000); // 1.0

        let target = target.clamp(0.0, 1.0);
        let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
        let from = f64::from_bits(CURRENT.load(Ordering::SeqCst));
        let app = app.clone();
        std::thread::spawn(move || {
            const DURATION: f64 = 0.8; // like the Swift HUD's easeInOut(duration: 0.8)
            let start = Instant::now();
            loop {
                if GENERATION.load(Ordering::SeqCst) != generation {
                    return;
                }
                let t = (start.elapsed().as_secs_f64() / DURATION).min(1.0);
                let eased = if t < 0.5 { 2.0 * t * t } else { 1.0 - (-2.0 * t + 2.0).powi(2) / 2.0 };
                let alpha = from + (target - from) * eased;
                CURRENT.store(alpha.to_bits(), Ordering::SeqCst);
                set_panel_alpha(&app, alpha);
                if t >= 1.0 {
                    return;
                }
                std::thread::sleep(Duration::from_millis(16));
            }
        });
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (app, target);
}

#[cfg(target_os = "macos")]
fn set_panel_alpha(app: &AppHandle, alpha: f64) {
    use tauri_nspanel::ManagerExt;
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Ok(panel) = handle.get_webview_panel("hud") {
            panel.set_alpha_value(alpha);
        }
    });
}

/// Tells the HUD page whether the mouse is over it. The page's own
/// mouseleave does not arrive while another app is in front, so a HUD
/// that was once hovered would never fade again.
pub fn spawn_hud_hover_watch(app: &AppHandle) {
    use std::time::Duration;
    use tauri::Emitter;

    let app = app.clone();
    std::thread::spawn(move || {
        let mut was_inside: Option<bool> = None;
        loop {
            std::thread::sleep(Duration::from_millis(150));
            let Some(hud) = app.get_webview_window("hud") else { continue };
            if !hud.is_visible().unwrap_or(false) {
                was_inside = None;
                continue;
            }
            let (Ok(cursor), Ok(pos), Ok(size)) = (hud.cursor_position(), hud.outer_position(), hud.outer_size()) else {
                continue;
            };
            let inside = cursor.x >= pos.x as f64
                && cursor.x < pos.x as f64 + size.width as f64
                && cursor.y >= pos.y as f64
                && cursor.y < pos.y as f64 + size.height as f64;
            if was_inside != Some(inside) {
                was_inside = Some(inside);
                let _ = app.emit_to("hud", "hud-hover", inside);
            }
        }
    });
}

/// Moves and resizes the HUD in one go (macOS has no native edge dragging for
/// borderless windows, so the page drives it)
pub fn set_hud_frame(app: &AppHandle, x: f64, y: f64, width: f64, height: f64) {
    if let Some(w) = app.get_webview_window("hud") {
        let width = width.max(HUD_MIN_SIZE.0);
        let height = height.max(HUD_MIN_SIZE.1);
        let _ = w.set_size(LogicalSize::new(width, height));
        let _ = w.set_position(LogicalPosition::new(x, y));
    }
}
