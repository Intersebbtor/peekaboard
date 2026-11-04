//! The app's state, in one place and without any UI code, so it can be tested.
//! Port of the Swift `KeymapViewModel` plus the parts of `AppState` that are
//! about data (live layer, pressed keys, typing speed, recent files).

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use serde::Serialize;
use zmk_core::{KeyBinding, Keymap, LayerSelection, WpmMeter};

use crate::settings::Settings;

pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

pub type Shared = Arc<Mutex<Inner>>;

pub struct Inner {
    pub settings: Settings,
    settings_path: PathBuf,
    /// The keymap as parsed from the file, without key names
    file_keymap: Option<Keymap>,
    /// The displayed keymap: the file's, with the names given by right-click
    pub keymap: Option<Keymap>,
    pub file_path: Option<String>,
    pub error: Option<String>,
    pub selection: LayerSelection,
    /// Key positions currently held on the keyboard
    pub pressed: BTreeSet<usize>,
    wpm_meter: WpmMeter,
    pub wpm: u32,
    /// Product name of the connected keyboard
    pub device: Option<String>,
    epoch: Instant,
}

/// Everything that only changes when a keymap is loaded
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeymapPayload {
    pub keymap: Option<Keymap>,
    pub file_path: Option<String>,
    pub error: Option<String>,
    pub recent_keymaps: Vec<String>,
    pub key_names: std::collections::BTreeMap<String, String>,
}

/// Everything that changes while typing
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LivePayload {
    pub version: &'static str,
    pub selection: LayerSelection,
    pub pressed: Vec<usize>,
    pub wpm: u32,
    pub wpm_history: Vec<u32>,
    pub device: Option<String>,
    pub live_layer_name: Option<String>,
    pub settings: Settings,
}

impl Inner {
    pub fn new(settings: Settings, settings_path: PathBuf) -> Self {
        Inner {
            settings,
            settings_path,
            file_keymap: None,
            keymap: None,
            file_path: None,
            error: None,
            selection: LayerSelection::default(),
            pressed: BTreeSet::new(),
            wpm_meter: WpmMeter::new(),
            wpm: 0,
            device: None,
            epoch: Instant::now(),
        }
    }

    fn now(&self) -> f64 {
        self.epoch.elapsed().as_secs_f64()
    }

    pub fn save_settings(&self) {
        self.settings.save(&self.settings_path);
    }

    // MARK: - Keymap file

    /// Loads a keymap file. A reload of the same file keeps the selected layer.
    pub fn load_path(&mut self, path: &str) -> bool {
        if path.is_empty() {
            return false;
        }
        if !std::path::Path::new(path).is_file() {
            self.error = Some("File not found".to_string());
            return false;
        }
        match zmk_core::load_keymap(path) {
            Ok(parsed) => {
                let is_new_file = self.file_path.as_deref() != Some(path);
                self.file_keymap = Some(parsed);
                self.error = None;
                self.file_path = Some(path.to_string());
                self.update_displayed_keymap(is_new_file);
                true
            }
            Err(message) => {
                self.error = Some(message);
                false
            }
        }
    }

    /// Opens a file chosen by the user: also remembers it
    pub fn open_path(&mut self, path: &str) -> bool {
        let ok = self.load_path(path);
        if ok {
            self.settings.add_recent_keymap(path);
            self.save_settings();
        }
        ok
    }

    pub fn reload(&mut self) {
        if let Some(path) = self.file_path.clone() {
            self.load_path(&path);
        }
    }

    pub fn clear_recent(&mut self) {
        self.settings.recent_keymaps.clear();
        self.save_settings();
    }

    fn update_displayed_keymap(&mut self, reset_selection: bool) {
        let mut shown = self.file_keymap.clone();
        if let Some(keymap) = shown.as_mut() {
            if !self.settings.key_names.is_empty() {
                for layer in &mut keymap.layers {
                    for binding in &mut layer.bindings {
                        binding.name = self.settings.key_names.get(&binding.display_text).cloned();
                    }
                }
            }
        }
        let layer_count = shown.as_ref().map_or(0, |k| k.layers.len());
        if reset_selection {
            self.selection.reset(layer_count);
        } else {
            self.selection.clamp(layer_count);
        }
        self.keymap = shown;
    }

    // MARK: - Key names

    /// An empty name removes it
    pub fn set_key_name(&mut self, display_text: &str, name: Option<&str>) {
        let trimmed = name.map(str::trim).unwrap_or("");
        if trimmed.is_empty() {
            self.settings.key_names.remove(display_text);
        } else {
            self.settings
                .key_names
                .insert(display_text.to_string(), trimmed.to_string());
        }
        self.save_settings();
        self.update_displayed_keymap(false);
    }

    // MARK: - Layer selection

    pub fn select_layer(&mut self, index: usize) {
        self.selection.select_manually(index);
    }

    pub fn apply_live_layer(&mut self, index: usize) {
        let count = self.layer_count();
        self.selection
            .apply_live(index, count, self.settings.follow_live_layer);
    }

    pub fn clear_live_layer(&mut self) {
        self.selection.clear_live();
        // Releases sent while disconnected never arrive
        self.pressed.clear();
    }

    /// Settings changed: following the live layer starts right away
    pub fn sync_to_live_layer(&mut self) {
        if let Some(live) = self.selection.live_index {
            let count = self.layer_count();
            self.selection.apply_live(live, count, true);
        }
    }

    fn layer_count(&self) -> usize {
        self.keymap.as_ref().map_or(0, |k| k.layers.len())
    }

    /// Name of the keyboard's active layer, or "Layer N" if the keymap does not have it
    pub fn live_layer_name(&self) -> Option<String> {
        let index = self.selection.live_index?;
        Some(
            self.keymap
                .as_ref()
                .and_then(|k| k.layers.get(index))
                .map(|l| l.name.clone())
                .unwrap_or_else(|| format!("Layer {index}")),
        )
    }

    // MARK: - Keys and typing speed

    /// Returns true when the typing-speed timer has something to do
    pub fn set_key(&mut self, position: usize, pressed: bool) -> bool {
        if pressed {
            self.pressed.insert(position);
            self.count_for_wpm(position)
        } else {
            self.pressed.remove(&position);
            false
        }
    }

    /// A press on the live layer that types something
    fn count_for_wpm(&mut self, position: usize) -> bool {
        let layer = self
            .selection
            .live_index
            .unwrap_or(self.selection.displayed_index);
        let binding: Option<&KeyBinding> = self
            .keymap
            .as_ref()
            .and_then(|k| k.layers.get(layer))
            .and_then(|l| l.bindings.get(position));
        if let Some(b) = binding {
            if !WpmMeter::counts(&b.raw_code) {
                return false;
            }
        }
        let now = self.now();
        self.wpm_meter.record_press(now);
        true
    }

    /// Once a second while there is something to show. Returns false when the
    /// graph has gone flat and the timer can rest.
    pub fn wpm_tick(&mut self) -> bool {
        let now = self.now();
        self.wpm = self.wpm_meter.sample(now);
        !self.wpm_meter.is_idle()
    }

    pub fn wpm_is_idle(&self) -> bool {
        self.wpm_meter.is_idle()
    }

    // MARK: - Payloads for the windows

    pub fn keymap_payload(&self) -> KeymapPayload {
        KeymapPayload {
            keymap: self.keymap.clone(),
            file_path: self.file_path.clone(),
            error: self.error.clone(),
            recent_keymaps: self.settings.recent_keymaps.clone(),
            key_names: self.settings.key_names.clone(),
        }
    }

    pub fn live_payload(&self) -> LivePayload {
        LivePayload {
            version: APP_VERSION,
            selection: self.selection,
            pressed: self.pressed.iter().copied().collect(),
            wpm: self.wpm,
            wpm_history: self.wpm_meter.history().to_vec(),
            device: self.device.clone(),
            live_layer_name: self.live_layer_name(),
            settings: self.settings.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEYMAP: &str = r#"
/ {
    keymap {
        compatible = "zmk,keymap";
        base { label = "Base"; bindings = < &kp Q &kp W &mo 1 >; };
        nav { label = "Nav"; bindings = < &kp LEFT &kp RIGHT &trans >; };
    };
};
"#;

    fn temp_keymap(name: &str) -> String {
        let dir = std::env::temp_dir().join(format!("zmk-viewer-test-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.keymap");
        std::fs::write(&path, KEYMAP).unwrap();
        path.to_string_lossy().to_string()
    }

    fn state(name: &str) -> (Inner, String) {
        let path = temp_keymap(name);
        let settings_path = PathBuf::from(&path).with_file_name("settings.json");
        (Inner::new(Settings::default(), settings_path), path)
    }

    #[test]
    fn loading_a_file_shows_its_layers() {
        let (mut s, path) = state("load");
        assert!(s.open_path(&path));
        assert_eq!(s.keymap.as_ref().unwrap().layers.len(), 2);
        assert_eq!(s.settings.recent_keymaps, vec![path]);
        assert!(s.error.is_none());
    }

    #[test]
    fn missing_file_sets_an_error() {
        let (mut s, _) = state("missing");
        assert!(!s.load_path("/does/not/exist.keymap"));
        assert_eq!(s.error.as_deref(), Some("File not found"));
    }

    #[test]
    fn a_reload_keeps_the_selected_layer() {
        let (mut s, path) = state("reload");
        s.open_path(&path);
        s.select_layer(1);
        s.reload();
        assert_eq!(s.selection.displayed_index, 1);
    }

    #[test]
    fn live_layer_is_followed_and_cleared() {
        let (mut s, path) = state("live");
        s.open_path(&path);
        s.apply_live_layer(1);
        assert_eq!(s.selection.displayed_index, 1);
        assert_eq!(s.live_layer_name().as_deref(), Some("Nav"));
        s.apply_live_layer(5);
        assert_eq!(s.live_layer_name().as_deref(), Some("Layer 5"));
        assert_eq!(s.selection.displayed_index, 1, "unknown layer is not shown");

        s.settings.follow_live_layer = false;
        s.apply_live_layer(0);
        assert_eq!(s.selection.displayed_index, 1, "not following");
        s.sync_to_live_layer();
        assert_eq!(s.selection.displayed_index, 0, "following again jumps to the live layer");
    }

    #[test]
    fn pressed_keys_are_tracked_and_cleared_on_disconnect() {
        let (mut s, _) = state("pressed");
        s.set_key(3, true);
        s.set_key(7, true);
        s.set_key(3, false);
        assert_eq!(s.pressed.iter().copied().collect::<Vec<_>>(), vec![7]);
        s.clear_live_layer();
        assert!(s.pressed.is_empty());
    }

    #[test]
    fn layer_keys_do_not_count_for_wpm() {
        let (mut s, path) = state("wpm");
        s.open_path(&path);
        s.apply_live_layer(0);
        assert!(s.set_key(0, true), "&kp Q types");
        assert!(!s.set_key(2, true), "&mo 1 does not");
    }

    #[test]
    fn key_names_apply_by_label_and_can_be_removed() {
        let (mut s, path) = state("names");
        s.open_path(&path);
        s.set_key_name("Q", Some("  Quit  "));
        assert_eq!(s.keymap.as_ref().unwrap().layers[0].bindings[0].name.as_deref(), Some("Quit"));
        assert_eq!(s.keymap.as_ref().unwrap().layers[0].bindings[1].name, None);
        s.set_key_name("Q", Some(""));
        assert_eq!(s.keymap.as_ref().unwrap().layers[0].bindings[0].name, None);
    }
}
