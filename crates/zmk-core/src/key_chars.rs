//! The character a key types in the active keyboard layout: Option+Shift+2 is
//! € in the US layout, the SEMI key is Ö in the German one. Keys with Ctrl or
//! Gui are shortcuts and type nothing.
//!
//! The table is filled by the platform (the app asks the OS for the current
//! layout) and stays empty otherwise, so tests don't depend on the machine.
//! Keys are identified by their USB HID usage, which is what ZMK uses.

use std::collections::HashMap;
use std::sync::{LazyLock, RwLock};

/// A key plus modifiers. `option` is Option on macOS and AltGr on Windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Combo {
    /// USB HID usage ID of the key (A = 0x04, 1 = 0x1E, ...)
    pub usage: u16,
    pub shift: bool,
    pub option: bool,
}

static TABLE: LazyLock<RwLock<HashMap<Combo, String>>> = LazyLock::new(|| RwLock::new(HashMap::new()));

pub struct KeyCharacters;

impl KeyCharacters {
    /// All typing keys of the keyboard layout (A..Z, digits, punctuation)
    pub fn typing_usages() -> Vec<u16> {
        let mut usages: Vec<u16> = KEY_USAGES.iter().map(|(_, u, _)| *u).collect();
        usages.sort_unstable();
        usages.dedup();
        usages
    }

    /// Replaces the table (empty = labels stay as the keymap names them)
    pub fn set_table(table: HashMap<Combo, String>) {
        *TABLE.write().unwrap() = table;
    }

    pub fn clear() {
        Self::set_table(HashMap::new());
    }

    /// The typed character for a ZMK key expression like "LS(LA(N2))", or None
    /// for keys that type nothing (shortcuts, modifiers, arrows, ...)
    pub fn typed_character(expression: &str) -> Option<String> {
        let mut shift = false;
        let mut option = false;
        let mut key = expression.to_uppercase();

        while key.ends_with(')') {
            let Some(open) = key.find('(') else { break };
            match &key[..open] {
                "LS" | "RS" => shift = true,
                "LA" | "RA" => option = true,
                _ => return None,
            }
            key = key[open + 1..key.len() - 1].to_string();
        }

        let &(_, usage, implied_shift) = KEY_USAGES.iter().find(|(name, _, _)| *name == key)?;
        let combo = Combo {
            usage,
            shift: shift || implied_shift,
            option,
        };
        let typed = TABLE.read().unwrap().get(&combo)?.clone();

        let mut chars = typed.chars();
        let (Some(only), None) = (chars.next(), chars.next()) else {
            return None;
        };
        if only.is_control() || only == '\t' || only.is_whitespace() {
            return None;
        }

        // Plain letters as printed on keycaps: Q, Ö. Not ß -> SS.
        let upper = typed.to_uppercase();
        if !shift && !option && !implied_shift && upper.chars().count() == 1 {
            Some(upper)
        } else {
            Some(typed)
        }
    }
}

/// ZMK key names of the typing keys: HID usage, and whether the name implies
/// shift (EXCL is LS(N1))
const KEY_USAGES: &[(&str, u16, bool)] = &[
    ("A", 0x04, false), ("B", 0x05, false), ("C", 0x06, false), ("D", 0x07, false),
    ("E", 0x08, false), ("F", 0x09, false), ("G", 0x0A, false), ("H", 0x0B, false),
    ("I", 0x0C, false), ("J", 0x0D, false), ("K", 0x0E, false), ("L", 0x0F, false),
    ("M", 0x10, false), ("N", 0x11, false), ("O", 0x12, false), ("P", 0x13, false),
    ("Q", 0x14, false), ("R", 0x15, false), ("S", 0x16, false), ("T", 0x17, false),
    ("U", 0x18, false), ("V", 0x19, false), ("W", 0x1A, false), ("X", 0x1B, false),
    ("Y", 0x1C, false), ("Z", 0x1D, false),
    ("N1", 0x1E, false), ("N2", 0x1F, false), ("N3", 0x20, false), ("N4", 0x21, false),
    ("N5", 0x22, false), ("N6", 0x23, false), ("N7", 0x24, false), ("N8", 0x25, false),
    ("N9", 0x26, false), ("N0", 0x27, false),
    ("NUMBER_1", 0x1E, false), ("NUMBER_2", 0x1F, false), ("NUMBER_3", 0x20, false),
    ("NUMBER_4", 0x21, false), ("NUMBER_5", 0x22, false), ("NUMBER_6", 0x23, false),
    ("NUMBER_7", 0x24, false), ("NUMBER_8", 0x25, false), ("NUMBER_9", 0x26, false),
    ("NUMBER_0", 0x27, false),
    ("MINUS", 0x2D, false), ("EQUAL", 0x2E, false),
    ("LBKT", 0x2F, false), ("LEFT_BRACKET", 0x2F, false),
    ("RBKT", 0x30, false), ("RIGHT_BRACKET", 0x30, false),
    ("BSLH", 0x31, false), ("BACKSLASH", 0x31, false),
    ("NUHS", 0x32, false), ("NON_US_HASH", 0x32, false),
    ("SEMI", 0x33, false), ("SEMICOLON", 0x33, false),
    ("SQT", 0x34, false), ("SINGLE_QUOTE", 0x34, false), ("APOSTROPHE", 0x34, false), ("APOS", 0x34, false),
    ("GRAVE", 0x35, false),
    ("COMMA", 0x36, false),
    ("DOT", 0x37, false), ("PERIOD", 0x37, false),
    ("FSLH", 0x38, false), ("SLASH", 0x38, false),
    ("NUBS", 0x64, false), ("NON_US_BACKSLASH", 0x64, false),
    // Names that imply shift
    ("EXCL", 0x1E, true), ("EXCLAMATION", 0x1E, true),
    ("AT", 0x1F, true), ("AT_SIGN", 0x1F, true),
    ("HASH", 0x20, true), ("POUND", 0x20, true),
    ("DLLR", 0x21, true), ("DOLLAR", 0x21, true),
    ("PRCNT", 0x22, true), ("PERCENT", 0x22, true),
    ("CARET", 0x23, true),
    ("AMPS", 0x24, true), ("AMPERSAND", 0x24, true),
    ("STAR", 0x25, true), ("ASTRK", 0x25, true), ("ASTERISK", 0x25, true),
    ("LPAR", 0x26, true), ("LEFT_PARENTHESIS", 0x26, true),
    ("RPAR", 0x27, true), ("RIGHT_PARENTHESIS", 0x27, true),
    ("UNDER", 0x2D, true), ("UNDERSCORE", 0x2D, true),
    ("PLUS", 0x2E, true),
    ("LBRC", 0x2F, true), ("LEFT_BRACE", 0x2F, true),
    ("RBRC", 0x30, true), ("RIGHT_BRACE", 0x30, true),
    ("PIPE", 0x31, true),
    ("COLON", 0x33, true),
    ("DQT", 0x34, true), ("DOUBLE_QUOTES", 0x34, true),
    ("TILDE", 0x35, true),
    ("LT", 0x36, true), ("LESS_THAN", 0x36, true),
    ("GT", 0x37, true), ("GREATER_THAN", 0x37, true),
    ("QMARK", 0x38, true), ("QUESTION", 0x38, true),
];

#[cfg(test)]
pub(crate) mod test_support {
    use std::sync::Mutex;

    /// The table is global; tests that touch it run one at a time
    pub static TABLE_LOCK: Mutex<()> = Mutex::new(());
}

#[cfg(test)]
mod tests {
    use super::test_support::TABLE_LOCK;
    use super::*;

    fn combo(usage: u16, shift: bool, option: bool) -> Combo {
        Combo { usage, shift, option }
    }

    /// A few keys of the US layout, plus German ones on SEMI and MINUS
    fn sample_table() {
        let table = HashMap::from([
            (combo(0x14, false, false), "q".to_string()),
            (combo(0x1F, false, false), "2".to_string()),
            (combo(0x1E, true, false), "!".to_string()),
            (combo(0x1F, true, true), "€".to_string()),
            (combo(0x16, false, true), "ß".to_string()),
            (combo(0x18, false, true), "¨".to_string()),
            (combo(0x33, false, false), "ö".to_string()),
            (combo(0x2D, false, false), "ß".to_string()),
        ]);
        KeyCharacters::set_table(table);
    }

    #[test]
    fn option_combos_show_their_character() {
        let _g = TABLE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        sample_table();
        assert_eq!(KeyCharacters::typed_character("LS(LA(N2))").as_deref(), Some("€"));
        assert_eq!(KeyCharacters::typed_character("LA(LS(NUMBER_2))").as_deref(), Some("€"), "nesting order doesn't matter");
        assert_eq!(KeyCharacters::typed_character("RA(S)").as_deref(), Some("ß"));
        assert_eq!(KeyCharacters::typed_character("LA(U)").as_deref(), Some("¨"), "a dead key shows its mark");
        KeyCharacters::clear();
    }

    #[test]
    fn plain_keys_follow_the_layout() {
        let _g = TABLE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        sample_table();
        assert_eq!(KeyCharacters::typed_character("Q").as_deref(), Some("Q"), "letters as on keycaps");
        assert_eq!(KeyCharacters::typed_character("SEMI").as_deref(), Some("Ö"));
        assert_eq!(KeyCharacters::typed_character("MINUS").as_deref(), Some("ß"), "not SS");
        assert_eq!(KeyCharacters::typed_character("EXCL").as_deref(), Some("!"), "EXCL implies shift");
        KeyCharacters::clear();
    }

    #[test]
    fn shortcuts_and_special_keys_type_nothing() {
        let _g = TABLE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        sample_table();
        assert_eq!(KeyCharacters::typed_character("LG(Q)"), None);
        assert_eq!(KeyCharacters::typed_character("LC(LA(Q))"), None);
        assert_eq!(KeyCharacters::typed_character("LSHFT"), None);
        assert_eq!(KeyCharacters::typed_character("LEFT"), None);
        assert_eq!(KeyCharacters::typed_character("W"), None, "not in this layout's table");
        KeyCharacters::clear();
    }
}
