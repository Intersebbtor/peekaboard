#![cfg_attr(all(not(debug_assertions), target_os = "windows"), windows_subsystem = "windows")]

mod hid;
mod hub;
mod settings;
mod state;
mod sys_layout;
mod tray;
mod watch;
mod windows;

use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;

use crate::hub::Hub;
use crate::settings::{settings_path, Settings};
use crate::state::Inner;

// MARK: - Commands (what the windows can ask for)

/// Everything a window needs when it opens
#[tauri::command]
fn get_state(hub: State<'_, Hub>) -> Value {
    let s = hub.lock();
    json!({ "keymap": s.keymap_payload(), "live": s.live_payload() })
}

#[tauri::command]
fn select_layer(hub: State<'_, Hub>, index: usize) {
    eprintln!("[UI] Layer {index} selected");
    hub.lock().select_layer(index);
    hub.emit_live();
}

/// The file dialog, then load
#[tauri::command(async)]
fn choose_file(app: AppHandle, hub: State<'_, Hub>) {
    let start_dir = hub
        .lock()
        .file_path
        .as_deref()
        .and_then(|p| std::path::Path::new(p).parent().map(|d| d.to_path_buf()));

    let mut dialog = app
        .dialog()
        .file()
        .set_title("Choose your keymap")
        .add_filter("ZMK keymap", &["keymap"])
        .add_filter("All files", &["*"]);
    if let Some(dir) = start_dir {
        dialog = dialog.set_directory(dir);
    }
    if let Some(path) = dialog.blocking_pick_file() {
        if let Ok(path) = path.into_path() {
            hub.open_path(&path.to_string_lossy());
        }
    }
}

/// A recent file, or one dropped on the window
#[tauri::command]
fn load_path(hub: State<'_, Hub>, path: String) {
    hub.open_path(&path);
}

#[tauri::command]
fn clear_recent(hub: State<'_, Hub>) {
    hub.lock().clear_recent();
    hub.emit_keymap();
}

/// One setting by its name (the camelCase name in settings.json)
#[tauri::command]
fn set_setting(hub: State<'_, Hub>, key: String, value: Value) -> Result<(), String> {
    {
        let mut s = hub.lock();
        let mut current = serde_json::to_value(&s.settings).map_err(|e| e.to_string())?;
        let object = current.as_object_mut().ok_or("settings are not an object")?;
        if !object.contains_key(&key) {
            return Err(format!("Unknown setting: {key}"));
        }
        object.insert(key.clone(), value.clone());
        s.settings = serde_json::from_value::<Settings>(current).map_err(|e| format!("Bad value for {key}: {e}"))?;
        // Following the live layer starts right away
        if key == "followLiveLayer" && value == Value::Bool(true) {
            s.sync_to_live_layer();
        }
        if key == "hudUseMaterial" {
            windows::set_hud_material(&hub.app, s.settings.hud_use_material);
        }
        s.save_settings();
    }
    hub.emit_live();
    Ok(())
}

/// Names a key (by what it does, so it applies wherever the binding sits)
#[tauri::command]
fn set_key_name(hub: State<'_, Hub>, display_text: String, name: Option<String>) {
    hub.lock().set_key_name(&display_text, name.as_deref());
    hub.emit_all();
}

#[tauri::command]
fn set_shortcut(hub: State<'_, Hub>, accelerator: String) -> Result<(), String> {
    hub.register_shortcut(&accelerator)?;
    {
        let mut s = hub.lock();
        s.settings.shortcut = accelerator;
        s.save_settings();
    }
    hub.emit_live();
    Ok(())
}

#[tauri::command]
fn toggle_hud(hub: State<'_, Hub>) {
    hub.toggle_hud();
}

/// macOS: fades the HUD window to this transparency (see windows::fade_hud)
#[tauri::command]
fn fade_hud(app: AppHandle, alpha: f64) {
    windows::fade_hud(&app, alpha);
}

/// macOS: resizing from the HUD's edges, driven by the page
#[tauri::command]
fn set_hud_frame(app: AppHandle, x: f64, y: f64, width: f64, height: f64) {
    windows::set_hud_frame(&app, x, y, width, height);
}

#[tauri::command]
fn open_in_editor(hub: State<'_, Hub>) {
    let Some(path) = hub.lock().file_path.clone() else { return };
    open_with_default_app(&path);
}

#[tauri::command]
fn hide_main(app: AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.hide();
    }
}

#[tauri::command]
fn quit(app: AppHandle) {
    app.exit(0);
}

fn open_with_default_app(path: &str) {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        // No console flash
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", "", path])
            .creation_flags(0x0800_0000)
            .spawn();
    }
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(path).spawn();
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(path).spawn();
}

// MARK: - Start

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let simulate = args.iter().any(|a| a == "--simulate") || std::env::var("ZMK_SIMULATE").is_ok();
    let start_with_hud = args.iter().any(|a| a == "--hud");
    // Optional: a keymap file to open right away (also what "Open with" passes)
    let file_arg = args.iter().skip(1).find(|a| a.ends_with(".keymap")).cloned();

    let builder = tauri::Builder::default();
    #[cfg(target_os = "macos")]
    let builder = builder.plugin(tauri_nspanel::init());
    builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            get_state,
            select_layer,
            choose_file,
            load_path,
            clear_recent,
            set_setting,
            set_key_name,
            set_shortcut,
            toggle_hud,
            fade_hud,
            set_hud_frame,
            open_in_editor,
            hide_main,
            quit,
        ])
        .setup(move |app| {
            let config_dir = app.path().app_config_dir()?;
            let path = settings_path(config_dir);
            let settings = Settings::load(&path);
            let shortcut = settings.shortcut.clone();
            let hud_enabled = settings.hud_enabled || start_with_hud;
            let geometry = settings.hud_geometry;
            let use_material = settings.hud_use_material;
            let most_recent = settings.recent_keymaps.first().cloned();

            let state = Arc::new(Mutex::new(Inner::new(settings, path)));
            let hub = Hub::new(app.handle().clone(), state);
            app.manage(hub.clone());

            // Labels show what the keys type, from the first parse on
            sys_layout::load_system_layout();
            if let Some(file) = file_arg.or(most_recent) {
                hub.lock().load_path(&file);
            }

            windows::create_main(app.handle())?;
            windows::create_hud(&hub, geometry, use_material)?;
            windows::spawn_hud_hover_watch(app.handle());
            if hud_enabled {
                hub.lock().settings.hud_enabled = true;
                windows::set_hud_visible(app.handle(), true);
            }
            tray::create(&hub)?;

            // A menu bar app like the Swift one: no Dock icon, no Cmd-Tab
            // entry. Only once the windows exist: set before, the web views
            // of the windows stay blank.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            if let Err(e) = hub.register_shortcut(&shortcut) {
                eprintln!("[Shortcut] {e}");
            }

            watch::spawn(hub.clone());
            hid::spawn(hub.clone(), simulate);
            hub.spawn_wpm_ticker();
            hub.emit_all();
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running the ZMK Keymap Viewer");
}
