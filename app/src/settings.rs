//! Settings that survive a restart (settings.json in the app's config folder).
//! Same options as the Swift app's @AppStorage values.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct HudGeometry {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub hud_enabled: bool,
    pub hud_opacity: f64,
    /// macOS: the blurred material behind the HUD, else a plain background
    pub hud_use_material: bool,
    /// Seconds until the HUD fades, 0 = never
    pub hud_timeout: f64,
    /// Typing speed in the HUD's corner, like ZMK's nice!view widget
    pub show_wpm_in_hud: bool,
    pub follow_live_layer: bool,
    /// Layer number in the tray icon
    pub show_layer_in_tray: bool,
    /// Accelerator string, e.g. "Ctrl+Shift+K"
    pub shortcut: String,
    pub recent_keymaps: Vec<String>,
    /// Names given with a right-click, by the key's label
    pub key_names: BTreeMap<String, String>,
    pub hud_geometry: Option<HudGeometry>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            hud_enabled: false,
            hud_opacity: 0.9,
            hud_use_material: true,
            hud_timeout: 3.0,
            show_wpm_in_hud: true,
            follow_live_layer: true,
            show_layer_in_tray: true,
            shortcut: "Ctrl+Shift+K".to_string(),
            recent_keymaps: Vec::new(),
            key_names: BTreeMap::new(),
            hud_geometry: None,
        }
    }
}

pub const MAX_RECENT_KEYMAPS: usize = 5;

impl Settings {
    pub fn load(path: &Path) -> Settings {
        std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        match serde_json::to_vec_pretty(self) {
            Ok(bytes) => {
                if let Err(e) = std::fs::write(path, bytes) {
                    eprintln!("[Settings] Could not save: {e}");
                }
            }
            Err(e) => eprintln!("[Settings] Could not encode: {e}"),
        }
    }

    pub fn add_recent_keymap(&mut self, path: &str) {
        self.recent_keymaps.retain(|p| p != path);
        self.recent_keymaps.insert(0, path.to_string());
        self.recent_keymaps.truncate(MAX_RECENT_KEYMAPS);
    }
}

pub fn settings_path(config_dir: PathBuf) -> PathBuf {
    config_dir.join("settings.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_keymaps_move_to_front_and_are_capped() {
        let mut s = Settings::default();
        for i in 0..7 {
            s.add_recent_keymap(&format!("/k{i}.keymap"));
        }
        s.add_recent_keymap("/k3.keymap");
        assert_eq!(s.recent_keymaps.len(), MAX_RECENT_KEYMAPS);
        assert_eq!(s.recent_keymaps[0], "/k3.keymap");
        assert_eq!(s.recent_keymaps.iter().filter(|p| *p == "/k3.keymap").count(), 1);
    }

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let s: Settings = serde_json::from_str(r#"{"hudOpacity":0.5}"#).unwrap();
        assert_eq!(s.hud_opacity, 0.5);
        assert_eq!(s.hud_timeout, 3.0);
        assert_eq!(s.shortcut, "Ctrl+Shift+K");
    }
}
