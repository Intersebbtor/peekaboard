//! Platform-independent core of the viewer: keymap parsers (ZMK `.keymap`,
//! QMK `keymap.json` and `keymap.c`), layout detection, live-layer packets and
//! typing speed. No UI, no OS calls.

pub mod key_chars;
pub mod layout;
pub mod live;
pub mod parser;
pub mod qmk;
pub mod wpm;

pub use key_chars::{Combo, KeyCharacters};
pub use layout::KeyboardLayout;
pub use live::{highest_layer, LayerPacket, LayerSelection};
pub use parser::{
    load_keymap, parse, parse_bindings, set_modifier_style, Firmware, KeyBinding, Keymap,
    KeymapLayer, ModifierStyle,
};
pub use wpm::WpmMeter;
