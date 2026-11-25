//! The hub ties the state to the outside world: it tells the windows and the
//! tray when something changed, and is what the HID thread, the file watcher,
//! the hotkey and the commands all talk to.

use std::sync::MutexGuard;
use std::time::Duration;

use tauri::{AppHandle, Emitter};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use zmk_core::{highest_layer, LayerPacket};

use crate::state::{Inner, Shared};
use crate::{tray, windows};

#[derive(Clone)]
pub struct Hub {
    pub app: AppHandle,
    pub state: Shared,
}

impl Hub {
    pub fn new(app: AppHandle, state: Shared) -> Self {
        Hub { app, state }
    }

    pub fn lock(&self) -> MutexGuard<'_, Inner> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    // MARK: - Telling the windows

    /// A keymap was loaded, or something about it changed
    pub fn emit_keymap(&self) {
        let payload = self.lock().keymap_payload();
        let _ = self.app.emit("keymap", payload);
    }

    /// Live layer, pressed keys, typing speed or settings changed
    pub fn emit_live(&self) {
        let (payload, layer) = {
            let s = self.lock();
            let layer = if s.settings.show_layer_in_tray {
                s.selection.live_index
            } else {
                None
            };
            (s.live_payload(), layer)
        };
        let name = payload.live_layer_name.clone();
        let _ = self.app.emit("live", payload);
        tray::update(&self.app, layer, name.as_deref());
    }

    pub fn emit_all(&self) {
        self.emit_keymap();
        self.emit_live();
    }

    // MARK: - Keymap file

    /// Loads a file the user chose (remembers it)
    pub fn open_path(&self, path: &str) -> bool {
        crate::sys_layout::load_system_layout();
        let ok = self.lock().open_path(path);
        self.emit_all();
        ok
    }

    /// Loads the file again, e.g. after it changed on disk
    pub fn reload(&self) {
        crate::sys_layout::load_system_layout();
        self.lock().reload();
        self.emit_all();
    }

    // MARK: - HUD

    pub fn toggle_hud(&self) {
        let visible = {
            let mut s = self.lock();
            s.settings.hud_enabled = !s.settings.hud_enabled;
            s.save_settings();
            s.settings.hud_enabled
        };
        windows::set_hud_visible(&self.app, visible);
        self.emit_live();
    }

    // MARK: - Global shortcut

    /// Registers the HUD hotkey, replacing the previous one
    pub fn register_shortcut(&self, accelerator: &str) -> Result<(), String> {
        let shortcut: Shortcut = accelerator
            .parse()
            .map_err(|e| format!("Not a valid shortcut: {e}"))?;
        let gs = self.app.global_shortcut();
        let _ = gs.unregister_all();
        let hub = self.clone();
        gs.on_shortcut(shortcut, move |_app, _shortcut, event| {
            if event.state() == ShortcutState::Pressed {
                hub.toggle_hud();
            }
        })
        .map_err(|e| format!("Could not register the shortcut: {e}"))
    }

    // MARK: - Keyboard

    pub fn keyboard_connected(&self, name: String) {
        {
            let mut s = self.lock();
            s.device = Some(name);
            // The firmware only reports changes, so assume the base layer
            // until the first one arrives
            s.apply_live_layer(0);
        }
        self.emit_live();
    }

    pub fn keyboard_disconnected(&self) {
        {
            let mut s = self.lock();
            s.device = None;
            s.clear_live_layer();
        }
        self.emit_live();
    }

    pub fn packet(&self, packet: LayerPacket) {
        match packet {
            LayerPacket::LayerState {
                default_layers,
                active_layers,
            } => {
                let layer = highest_layer(active_layers, default_layers);
                self.lock().apply_live_layer(layer);
                self.emit_live();
            }
            LayerPacket::KeyEvent { position, pressed } => {
                self.lock().set_key(position, pressed);
                self.emit_live();
            }
            LayerPacket::MatrixKeyEvent { row, col, pressed } => {
                self.lock().set_matrix_key(row, col, pressed);
                self.emit_live();
            }
            // The Studio tunnel (reading the keymap from the keyboard) is not in this version
            LayerPacket::StudioChunk(_) => {}
        }
    }

    // MARK: - Typing speed

    /// Samples the typing speed once a second while there is something to show
    pub fn spawn_wpm_ticker(&self) {
        let hub = self.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_secs(1));
            let active = {
                let mut s = hub.lock();
                if s.wpm_is_idle() && s.wpm == 0 {
                    false
                } else {
                    s.wpm_tick();
                    true
                }
            };
            if active {
                hub.emit_live();
            }
        });
    }
}
