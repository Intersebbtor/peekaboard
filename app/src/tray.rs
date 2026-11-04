//! Tray icon: the app icon, or a badge with the live layer number (the Swift
//! app's menu bar icon shows the layer between the two keyboard halves).

use std::sync::Mutex;

use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::AppHandle;

use crate::hub::Hub;
use crate::windows;

const TRAY_ID: &str = "main";
const SIZE: usize = 32;

/// What the tray shows right now, so unchanged states are not redrawn
static SHOWN: Mutex<Option<Option<usize>>> = Mutex::new(None);

pub fn create(hub: &Hub) -> tauri::Result<()> {
    let app = &hub.app;
    let show = MenuItem::with_id(app, "show", "Show Keymap", true, None::<&str>)?;
    let hud = MenuItem::with_id(app, "hud", "HUD Mode", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&show, &hud, &separator, &quit])?;

    let menu_hub = hub.clone();
    let click_hub = hub.clone();
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("ZMK Keymap Viewer")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "show" => windows::show_main(app),
            "hud" => menu_hub.toggle_hud(),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(move |tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                windows::toggle_main(tray.app_handle());
                let _ = &click_hub;
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

/// `layer`: the live layer, None without keyboard (or when hidden in settings)
pub fn update(app: &AppHandle, layer: Option<usize>, layer_name: Option<&str>) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else { return };

    let tooltip = match layer_name {
        Some(name) => format!("ZMK Keymap Viewer · {name}"),
        None => "ZMK Keymap Viewer".to_string(),
    };
    let _ = tray.set_tooltip(Some(tooltip));

    let mut shown = SHOWN.lock().unwrap_or_else(|e| e.into_inner());
    if *shown == Some(layer) {
        return;
    }
    *shown = Some(layer);

    match layer {
        Some(n) => {
            let _ = tray.set_icon(Some(Image::new_owned(badge(n), SIZE as u32, SIZE as u32)));
        }
        None => {
            if let Some(icon) = app.default_window_icon() {
                let _ = tray.set_icon(Some(icon.clone()));
            }
        }
    }
}

/// 3x5 pixel digits
const DIGITS: [[u8; 5]; 10] = [
    [0b111, 0b101, 0b101, 0b101, 0b111],
    [0b010, 0b110, 0b010, 0b010, 0b111],
    [0b111, 0b001, 0b111, 0b100, 0b111],
    [0b111, 0b001, 0b111, 0b001, 0b111],
    [0b101, 0b101, 0b111, 0b001, 0b001],
    [0b111, 0b100, 0b111, 0b001, 0b111],
    [0b111, 0b100, 0b111, 0b101, 0b111],
    [0b111, 0b001, 0b001, 0b001, 0b001],
    [0b111, 0b101, 0b111, 0b101, 0b111],
    [0b111, 0b101, 0b111, 0b001, 0b111],
];

/// An accent-coloured rounded square with the layer number, readable on
/// light and dark taskbars alike
pub fn badge(layer: usize) -> Vec<u8> {
    let mut rgba = vec![0u8; SIZE * SIZE * 4];
    let (accent, white) = ([10u8, 132, 255, 255], [255u8, 255, 255, 255]);
    let radius = 7.0f32;

    // Rounded square
    for y in 0..SIZE {
        for x in 0..SIZE {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let dx = (radius - fx).max(fx - (SIZE as f32 - radius)).max(0.0);
            let dy = (radius - fy).max(fy - (SIZE as f32 - radius)).max(0.0);
            if dx * dx + dy * dy <= radius * radius {
                put(&mut rgba, x, y, accent);
            }
        }
    }

    // Digits, centred
    let digits: Vec<usize> = layer.to_string().chars().filter_map(|c| c.to_digit(10)).map(|d| d as usize).collect();
    let scale = if digits.len() == 1 { 5 } else { 3 };
    let gap = scale;
    let width = digits.len() * 3 * scale + (digits.len() - 1) * gap;
    let height = 5 * scale;
    let (left, top) = ((SIZE - width) / 2, (SIZE - height) / 2);

    for (i, digit) in digits.iter().enumerate() {
        let origin_x = left + i * (3 * scale + gap);
        for (row, bits) in DIGITS[*digit].iter().enumerate() {
            for col in 0..3 {
                if bits >> (2 - col) & 1 == 1 {
                    for dy in 0..scale {
                        for dx in 0..scale {
                            put(&mut rgba, origin_x + col * scale + dx, top + row * scale + dy, white);
                        }
                    }
                }
            }
        }
    }
    rgba
}

fn put(rgba: &mut [u8], x: usize, y: usize, color: [u8; 4]) {
    if x < SIZE && y < SIZE {
        let i = (y * SIZE + x) * 4;
        rgba[i..i + 4].copy_from_slice(&color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn badge_has_the_right_size_and_draws_something() {
        let one = badge(1);
        assert_eq!(one.len(), SIZE * SIZE * 4);
        let white = |b: &[u8]| b.chunks(4).filter(|p| p[..3] == [255, 255, 255]).count();
        assert!(white(&one) > 20);
        assert_ne!(badge(1), badge(2));
        // Two digits fit
        assert!(white(&badge(31)) > 20);
    }

    #[test]
    fn corners_stay_transparent() {
        let b = badge(0);
        assert_eq!(b[3], 0, "top-left pixel is transparent");
    }
}
