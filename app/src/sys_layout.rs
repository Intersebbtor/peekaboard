//! What the keys type in the active keyboard layout (the Swift app's
//! `KeyCharacters.loadSystemLayout`). Only Windows asks the OS for now; on the
//! other systems labels stay as the keymap names them.

// Plain and Shift keys only: Option combos like LS(LA(N2)) = € are what a Mac
// types, while Alt on Windows is a shortcut key (AltGr is RA only), so those
// keep their symbols (⇧+⌥+2) instead of showing a misleading character.
#[cfg(not(windows))]
pub fn load_system_layout() {}

#[cfg(windows)]
pub fn load_system_layout() {
    use std::collections::HashMap;

    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetKeyboardLayout, MapVirtualKeyExW, ToUnicodeEx, MAPVK_VSC_TO_VK_EX, VK_SHIFT,
    };
    use zmk_core::{Combo, KeyCharacters};

    let mut table: HashMap<Combo, String> = HashMap::new();
    // SAFETY: plain Win32 calls with buffers that live for the whole call
    unsafe {
        let layout = GetKeyboardLayout(0);
        for usage in KeyCharacters::typing_usages() {
            let Some(scancode) = scancode_for(usage) else { continue };
            let vk = MapVirtualKeyExW(scancode, MAPVK_VSC_TO_VK_EX, layout);
            if vk == 0 {
                continue;
            }
            for shift in [false, true] {
                let mut keys = [0u8; 256];
                if shift {
                    keys[VK_SHIFT as usize] = 0x80;
                }
                let mut buffer = [0u16; 8];
                // Flag 4: leave the keyboard's dead-key state alone
                let length = ToUnicodeEx(vk, scancode, keys.as_ptr(), buffer.as_mut_ptr(), buffer.len() as i32, 4, layout);
                // length < 0: a dead key, the buffer holds its own mark
                let count = if length < 0 { 1 } else { length as usize };
                if count > 0 {
                    let typed = String::from_utf16_lossy(&buffer[..count]);
                    table.insert(Combo { usage, shift, option: false }, typed);
                }
            }
        }
    }
    KeyCharacters::set_table(table);
}

/// Scan code (set 1) of a typing key by its USB HID usage
#[cfg(windows)]
fn scancode_for(usage: u16) -> Option<u32> {
    let code = match usage {
        // Letters A..Z
        0x04 => 0x1E, 0x05 => 0x30, 0x06 => 0x2E, 0x07 => 0x20, 0x08 => 0x12, 0x09 => 0x21,
        0x0A => 0x22, 0x0B => 0x23, 0x0C => 0x17, 0x0D => 0x24, 0x0E => 0x25, 0x0F => 0x26,
        0x10 => 0x32, 0x11 => 0x31, 0x12 => 0x18, 0x13 => 0x19, 0x14 => 0x10, 0x15 => 0x13,
        0x16 => 0x1F, 0x17 => 0x14, 0x18 => 0x16, 0x19 => 0x2F, 0x1A => 0x11, 0x1B => 0x2D,
        0x1C => 0x15, 0x1D => 0x2C,
        // Digits 1..9, 0
        0x1E..=0x27 => 0x02 + (usage - 0x1E) as u32,
        0x2D => 0x0C, // MINUS
        0x2E => 0x0D, // EQUAL
        0x2F => 0x1A, // [
        0x30 => 0x1B, // ]
        0x31 => 0x2B, // BACKSLASH
        0x32 => 0x2B, // NON_US_HASH
        0x33 => 0x27, // ;
        0x34 => 0x28, // '
        0x35 => 0x29, // `
        0x36 => 0x33, // ,
        0x37 => 0x34, // .
        0x38 => 0x35, // /
        0x64 => 0x56, // NON_US_BACKSLASH
        _ => return None,
    };
    Some(code)
}
