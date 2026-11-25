//! Live layer: decoding the Raw HID packets of the zmk-keypeek-layer-notifier
//! firmware module (usage page 0xFF60, usage 0x61), and choosing which layer
//! the views show. Port of `LiveLayer.swift`.

use serde::Serialize;

/// Raw HID usage page / usage of the keyboard's vendor interface
pub const USAGE_PAGE: u16 = 0xFF60;
pub const USAGE: u16 = 0x61;

pub const LAYER_STATE_MARKER: u8 = 0xFF;
pub const KEY_EVENT_MARKER: u8 = 0xF1;
/// The ZMK Studio RPC tunnel of the same module
pub const STUDIO_MARKER: u8 = 0xF2;

/// A Raw HID packet sent by the firmware module
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayerPacket {
    /// Bitmasks of the default and the currently active layers (bit N = layer N)
    LayerState {
        default_layers: u32,
        active_layers: u32,
    },
    /// The key at `position` was pressed or released
    KeyEvent { position: usize, pressed: bool },
    /// A chunk of the ZMK Studio RPC stream
    StudioChunk(Vec<u8>),
}

impl LayerPacket {
    /// Decodes one report. Returns None for unknown or truncated reports.
    pub fn decode(bytes: &[u8]) -> Option<LayerPacket> {
        match *bytes.first()? {
            LAYER_STATE_MARKER => {
                // [0xFF, 0x04, default mask (u32 LE), active mask (u32 LE)]
                if bytes.len() < 10 {
                    return None;
                }
                Some(LayerPacket::LayerState {
                    default_layers: u32_le(bytes, 2),
                    active_layers: u32_le(bytes, 6),
                })
            }
            KEY_EVENT_MARKER => {
                // [0xF1, 0x00, position, pressed]
                if bytes.len() < 4 {
                    return None;
                }
                Some(LayerPacket::KeyEvent {
                    position: bytes[2] as usize,
                    pressed: bytes[3] == 1,
                })
            }
            STUDIO_MARKER => {
                if bytes.len() < 2 {
                    return None;
                }
                let length = (bytes[1] as usize).min(bytes.len() - 2);
                Some(LayerPacket::StudioChunk(bytes[2..2 + length].to_vec()))
            }
            _ => None,
        }
    }
}

fn u32_le(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

/// The highest active layer is the one ZMK looks up keys on first, so it is
/// the layer to show. Layer 0 counts as always active.
pub fn highest_layer(active_layers: u32, default_layers: u32) -> usize {
    let mask = active_layers | default_layers | 1;
    (u32::BITS - 1 - mask.leading_zeros()) as usize
}

/// Which layer the views show: the keyboard's live layer, unless the user
/// picked a tab after the last live change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LayerSelection {
    /// The layer shown in the window and the HUD
    pub displayed_index: usize,
    /// The keyboard's highest active layer, None while no keyboard reports
    /// one. Can exceed the parsed layer count when the keymap file is old.
    pub live_index: Option<usize>,
}

impl LayerSelection {
    pub fn select_manually(&mut self, index: usize) {
        self.displayed_index = index;
    }

    /// A manual selection stays until the keyboard reports the next change.
    /// Layers the keymap file does not know are tracked but not displayed.
    pub fn apply_live(&mut self, index: usize, layer_count: usize, follow: bool) {
        self.live_index = Some(index);
        if follow && index < layer_count {
            self.displayed_index = index;
        }
    }

    pub fn clear_live(&mut self) {
        self.live_index = None;
    }

    /// For a newly loaded keymap: start on the live layer if it exists
    pub fn reset(&mut self, layer_count: usize) {
        self.displayed_index = match self.live_index {
            Some(live) if live < layer_count => live,
            _ => 0,
        };
    }

    /// For a reloaded keymap that may have lost layers
    pub fn clamp(&mut self, layer_count: usize) {
        if self.displayed_index >= layer_count {
            self.displayed_index = layer_count.saturating_sub(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 32-byte report as zmk-raw-hid sends it, zero padded
    fn report(prefix: &[u8]) -> Vec<u8> {
        let mut v = prefix.to_vec();
        v.resize(32, 0);
        v
    }

    #[test]
    fn decode_layer_state() {
        // default = layer 0, active = layers 0 and 2
        let bytes = report(&[0xFF, 0x04, 0x01, 0, 0, 0, 0x05, 0, 0, 0]);
        assert_eq!(
            LayerPacket::decode(&bytes),
            Some(LayerPacket::LayerState {
                default_layers: 0x1,
                active_layers: 0x5
            })
        );
    }

    #[test]
    fn decode_layer_state_is_little_endian() {
        let bytes = report(&[0xFF, 0x04, 0x01, 0, 0, 0, 0x01, 0x01, 0x00, 0x80]);
        assert_eq!(
            LayerPacket::decode(&bytes),
            Some(LayerPacket::LayerState {
                default_layers: 0x1,
                active_layers: 0x8000_0101
            })
        );
    }

    #[test]
    fn decode_key_event() {
        assert_eq!(
            LayerPacket::decode(&report(&[0xF1, 0x00, 14, 1])),
            Some(LayerPacket::KeyEvent {
                position: 14,
                pressed: true
            })
        );
        assert_eq!(
            LayerPacket::decode(&report(&[0xF1, 0x00, 14, 0])),
            Some(LayerPacket::KeyEvent {
                position: 14,
                pressed: false
            })
        );
    }

    #[test]
    fn decodes_studio_chunk() {
        assert_eq!(
            LayerPacket::decode(&report(&[0xF2, 0x03, 0xAB, 0x08, 0x01])),
            Some(LayerPacket::StudioChunk(vec![0xAB, 0x08, 0x01]))
        );
    }

    #[test]
    fn ignores_unknown_packets() {
        assert_eq!(LayerPacket::decode(&report(&[0x00])), None);
    }

    #[test]
    fn ignores_truncated_packets() {
        assert_eq!(LayerPacket::decode(&[]), None);
        assert_eq!(
            LayerPacket::decode(&[0xFF, 0x04, 0x01, 0, 0, 0, 0x05]),
            None
        );
        assert_eq!(LayerPacket::decode(&[0xF1, 0x00, 14]), None);
    }

    #[test]
    fn highest_layer_cases() {
        assert_eq!(highest_layer(0x1, 0x1), 0);
        assert_eq!(highest_layer(0b1011, 0x1), 3);
        assert_eq!(highest_layer(0, 0), 0, "empty mask counts as base");
        assert_eq!(highest_layer(0x8000_0001, 0x1), 31, "all bits");
    }

    #[test]
    fn live_layer_is_displayed_when_following() {
        let mut s = LayerSelection::default();
        s.apply_live(2, 5, true);
        assert_eq!(s.displayed_index, 2);
        assert_eq!(s.live_index, Some(2));
    }

    #[test]
    fn live_layer_is_not_displayed_when_not_following() {
        let mut s = LayerSelection::default();
        s.select_manually(1);
        s.apply_live(3, 5, false);
        assert_eq!(s.displayed_index, 1);
        assert_eq!(s.live_index, Some(3));
    }

    #[test]
    fn manual_selection_holds_until_next_live_change() {
        let mut s = LayerSelection::default();
        s.apply_live(2, 5, true);
        s.select_manually(4);
        assert_eq!(s.displayed_index, 4);
        s.apply_live(0, 5, true);
        assert_eq!(s.displayed_index, 0);
    }

    #[test]
    fn unknown_live_layer_keeps_displayed_layer() {
        let mut s = LayerSelection::default();
        s.apply_live(1, 3, true);
        s.apply_live(6, 3, true);
        assert_eq!(s.displayed_index, 1);
        assert_eq!(s.live_index, Some(6));
    }

    #[test]
    fn clear_live_keeps_displayed_layer() {
        let mut s = LayerSelection::default();
        s.apply_live(2, 5, true);
        s.clear_live();
        assert_eq!(s.live_index, None);
        assert_eq!(s.displayed_index, 2);
    }

    #[test]
    fn reset_starts_on_live_layer_or_base() {
        let mut s = LayerSelection::default();
        s.apply_live(3, 0, true);
        s.reset(5);
        assert_eq!(s.displayed_index, 3);

        let mut s = LayerSelection::default();
        s.select_manually(4);
        s.reset(5);
        assert_eq!(s.displayed_index, 0);
        s.apply_live(7, 5, true);
        s.reset(5);
        assert_eq!(s.displayed_index, 0);
    }

    #[test]
    fn clamp_after_layers_were_removed() {
        let mut s = LayerSelection::default();
        s.select_manually(4);
        s.clamp(5);
        assert_eq!(s.displayed_index, 4);
        s.clamp(3);
        assert_eq!(s.displayed_index, 2);
        s.clamp(0);
        assert_eq!(s.displayed_index, 0);
    }
}
