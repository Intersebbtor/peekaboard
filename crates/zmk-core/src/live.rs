//! Live layer: decoding the Raw HID packets of the KeyPeek firmware modules
//! (usage page 0xFF60, usage 0x61), and choosing which layer the views show.
//! Port of `LiveLayer.swift`, extended for QMK.
//!
//! The packets are the same for both firmwares:
//! - ZMK: [zmk-keypeek-layer-notifier](https://github.com/srwi/zmk-keypeek-layer-notifier)
//! - QMK: the `keypeek_layer_notify` module of
//!   [KeyPeek](https://github.com/srwi/keypeek), which only sends while the
//!   host keeps a subscription alive (see [`QMK_SUBSCRIBE`])

use serde::Serialize;

/// Raw HID usage page / usage of the keyboard's vendor interface
pub const USAGE_PAGE: u16 = 0xFF60;
pub const USAGE: u16 = 0x61;

pub const LAYER_STATE_MARKER: u8 = 0xFF;
pub const KEY_EVENT_MARKER: u8 = 0xF1;
/// The ZMK Studio RPC tunnel of the same module
pub const STUDIO_MARKER: u8 = 0xF2;

/// QMK: sent about once a second, keeps the keyboard sending layer and key events
pub const QMK_SUBSCRIBE: [u8; 2] = [0xC0, 0xA1];
/// QMK: stops the events again
pub const QMK_UNSUBSCRIBE: [u8; 2] = [0xC0, 0xA0];

/// A Raw HID packet sent by the firmware module
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayerPacket {
    /// Bitmasks of the default and the currently active layers (bit N = layer N)
    LayerState {
        default_layers: u32,
        active_layers: u32,
    },
    /// The key with this binding index was pressed or released
    KeyEvent { position: usize, pressed: bool },
    /// A key event as the keyboard sends it: QMK sends the switch matrix
    /// position, ZMK row 0 and the binding index as the column.
    /// [`crate::Keymap::position_of`] turns it into a binding index.
    MatrixKeyEvent { row: u8, col: u8, pressed: bool },
    /// A chunk of the ZMK Studio RPC stream
    StudioChunk(Vec<u8>),
}

impl LayerPacket {
    /// Decodes one report. Returns None for unknown or truncated reports.
    pub fn decode(bytes: &[u8]) -> Option<LayerPacket> {
        // Some platforms hand over the report ID 0 in front of the data
        let bytes = match bytes {
            [0, marker, ..]
                if matches!(
                    *marker,
                    LAYER_STATE_MARKER | KEY_EVENT_MARKER | STUDIO_MARKER
                ) =>
            {
                &bytes[1..]
            }
            _ => bytes,
        };
        match *bytes.first()? {
            LAYER_STATE_MARKER => {
                // [0xFF, size, default mask, active mask], masks little endian and
                // `size` bytes long: 4 on ZMK, sizeof(layer_state_t) on QMK (1, 2 or 4)
                let size = *bytes.get(1)? as usize;
                if size == 0 || size > 4 || bytes.len() < 2 + 2 * size {
                    return None;
                }
                Some(LayerPacket::LayerState {
                    default_layers: le(&bytes[2..2 + size]),
                    active_layers: le(&bytes[2 + size..2 + 2 * size]),
                })
            }
            KEY_EVENT_MARKER => {
                // [0xF1, row, col, pressed]
                if bytes.len() < 4 {
                    return None;
                }
                Some(LayerPacket::MatrixKeyEvent {
                    row: bytes[1],
                    col: bytes[2],
                    pressed: bytes[3] != 0,
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

/// Little-endian number of up to four bytes
fn le(bytes: &[u8]) -> u32 {
    bytes
        .iter()
        .rev()
        .fold(0u32, |acc, &b| (acc << 8) | b as u32)
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
            Some(LayerPacket::MatrixKeyEvent {
                row: 0,
                col: 14,
                pressed: true
            })
        );
        assert_eq!(
            LayerPacket::decode(&report(&[0xF1, 0x00, 14, 0])),
            Some(LayerPacket::MatrixKeyEvent {
                row: 0,
                col: 14,
                pressed: false
            })
        );
    }

    #[test]
    fn decode_qmk_key_event_by_matrix_position() {
        assert_eq!(
            LayerPacket::decode(&report(&[0xF1, 3, 4, 1])),
            Some(LayerPacket::MatrixKeyEvent {
                row: 3,
                col: 4,
                pressed: true
            })
        );
    }

    #[test]
    fn decode_qmk_layer_state_with_16_bit_masks() {
        // QMK's default layer_state_t is 16 bits: size 2
        assert_eq!(
            LayerPacket::decode(&report(&[0xFF, 0x02, 0x01, 0x00, 0x09, 0x80])),
            Some(LayerPacket::LayerState {
                default_layers: 0x1,
                active_layers: 0x8009
            })
        );
        // LAYER_STATE_8BIT: size 1
        assert_eq!(
            LayerPacket::decode(&report(&[0xFF, 0x01, 0x01, 0x04])),
            Some(LayerPacket::LayerState {
                default_layers: 0x1,
                active_layers: 0x4
            })
        );
        assert_eq!(LayerPacket::decode(&report(&[0xFF, 0x05])), None);
        assert_eq!(LayerPacket::decode(&report(&[0xFF, 0x00])), None);
    }

    #[test]
    fn skips_a_leading_report_id() {
        assert_eq!(
            LayerPacket::decode(&[0x00, 0xF1, 3, 4, 1]),
            Some(LayerPacket::MatrixKeyEvent {
                row: 3,
                col: 4,
                pressed: true
            })
        );
        assert_eq!(
            LayerPacket::decode(&[0x00, 0xFF, 0x01, 0x01, 0x02]),
            Some(LayerPacket::LayerState {
                default_layers: 0x1,
                active_layers: 0x2
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
