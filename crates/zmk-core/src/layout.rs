//! Keyboard layout: how many keys per row, split or not, and which board it is.
//! Port of `KeyboardLayout` from the Swift viewer.

use serde::Serialize;

/// Detected keyboard layout configuration
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyboardLayout {
    pub total_keys: usize,
    /// Number of keys in each row
    pub keys_per_row: Vec<usize>,
    pub row_count: usize,
    pub has_thumb_cluster: bool,
    pub thumb_keys_count: usize,
    /// e.g. "Sweep/Cradio", "Corne", "Go60", "Custom (60 keys)"
    pub name: String,
    /// Whether the keyboard has a gap in the middle
    pub is_split: bool,
}

/// Known boards, checked in this order: (keyword, name, is_split)
const KNOWN_KEYBOARDS: &[(&str, &str, bool)] = &[
    // Split keyboards
    ("sweep", "Sweep/Cradio", true),
    ("cradio", "Sweep/Cradio", true),
    ("corne", "Corne", true),
    ("crkbd", "Corne", true),
    ("sofle", "Sofle", true),
    ("lily58", "Lily58", true),
    ("kyria", "Kyria", true),
    ("ferris", "Ferris", true),
    ("ergodox", "Ergodox", true),
    ("dactyl", "Dactyl", true),
    ("totem", "Totem", true),
    ("go60", "Go60", true),
    ("glove80", "Glove80", true),
    // Non-split keyboards
    ("planck", "Planck", false),
    ("preonic", "Preonic", false),
    ("reviung", "Reviung", false),
    ("reviung41", "Reviung41", false),
    ("bdn9", "BDN9", false),
    ("nibble", "Nibble", false),
];

impl KeyboardLayout {
    /// Creates a layout straight from the parsed data
    pub fn create(
        key_count: usize,
        keys_per_row: Vec<usize>,
        detected_name: Option<&str>,
        is_split: bool,
    ) -> Self {
        let row_count = keys_per_row.len();
        let thumb_keys = keys_per_row.last().copied().unwrap_or(0);
        let first_row_keys = keys_per_row.first().copied().unwrap_or(0);

        // Thumb cluster: the last row has fewer keys than the first
        let has_thumb_cluster = thumb_keys < first_row_keys;

        let name = detected_name
            .map(str::to_string)
            .unwrap_or_else(|| format!("Custom ({key_count} keys)"));

        KeyboardLayout {
            total_keys: key_count,
            keys_per_row,
            row_count,
            has_thumb_cluster,
            thumb_keys_count: if has_thumb_cluster { thumb_keys } else { 0 },
            name,
            is_split,
        }
    }

    /// Detects the board from the file's content and path.
    /// Returns (name, is_split), or None when nothing matches.
    ///
    /// The Swift version walked an unordered dictionary; here the file path is
    /// checked first (the most reliable hint), then the content, so the result
    /// is always the same.
    pub fn detect_keyboard_info(content: &str, file_path: Option<&str>) -> Option<(&'static str, bool)> {
        // Layout editor signatures first
        if content.contains("GO60 LAYOUT EDITOR") || content.contains("KB_TYPE_GO_60") {
            return Some(("Go60", true));
        }
        if content.contains("GLOVE80 LAYOUT EDITOR") || content.contains("KB_TYPE_GLOVE_80") {
            return Some(("Glove80", true));
        }

        let lowercased = content.to_lowercase();
        let path_lower = file_path.unwrap_or("").to_lowercase();

        for (keyword, name, split) in KNOWN_KEYBOARDS {
            if path_lower.contains(keyword) {
                return Some((name, *split));
            }
        }
        for (keyword, name, split) in KNOWN_KEYBOARDS {
            if lowercased.contains(keyword) {
                return Some((name, *split));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create(count: usize, rows: &[usize], name: Option<&str>) -> KeyboardLayout {
        KeyboardLayout::create(count, rows.to_vec(), name, true)
    }

    #[test]
    fn sweep_layout_properties() {
        let sweep = create(34, &[10, 10, 10, 4], Some("Sweep/Cradio"));
        assert_eq!(sweep.total_keys, 34);
        assert_eq!(sweep.keys_per_row, vec![10, 10, 10, 4]);
        assert_eq!(sweep.row_count, 4);
        assert!(sweep.has_thumb_cluster);
        assert_eq!(sweep.thumb_keys_count, 4);
        assert_eq!(sweep.name, "Sweep/Cradio");
        assert!(sweep.is_split);
    }

    #[test]
    fn corne_layout_properties() {
        let corne = create(42, &[12, 12, 12, 6], Some("Corne"));
        assert_eq!(corne.total_keys, 42);
        assert_eq!(corne.keys_per_row, vec![12, 12, 12, 6]);
        assert_eq!(corne.row_count, 4);
        assert!(corne.has_thumb_cluster);
        assert_eq!(corne.thumb_keys_count, 6);
        assert_eq!(corne.name, "Corne");
    }

    #[test]
    fn create_without_name_uses_custom_name() {
        let layout = create(50, &[12, 12, 12, 14], None);
        assert_eq!(layout.total_keys, 50);
        assert_eq!(layout.name, "Custom (50 keys)");
        assert_eq!(layout.row_count, 4);
    }

    #[test]
    fn create_non_split() {
        let layout = KeyboardLayout::create(48, vec![12, 12, 12, 12], Some("Planck"), false);
        assert!(!layout.is_split);
    }

    #[test]
    fn thumb_cluster_detection() {
        assert!(create(34, &[10, 10, 10, 4], None).has_thumb_cluster);
        let without = create(30, &[10, 10, 10], None);
        assert!(!without.has_thumb_cluster);
        assert_eq!(without.thumb_keys_count, 0);
    }

    #[test]
    fn small_thumb_cluster() {
        let layout = create(32, &[10, 10, 10, 2], None);
        assert!(layout.has_thumb_cluster);
        assert_eq!(layout.thumb_keys_count, 2);
    }

    #[test]
    fn asymmetrical_rows() {
        let layout = create(44, &[12, 12, 12, 8], None);
        assert_eq!(layout.row_count, 4);
        assert_eq!(layout.keys_per_row.iter().sum::<usize>(), 44);
    }

    #[test]
    fn create_with_zero_keys() {
        let layout = create(0, &[], None);
        assert_eq!(layout.total_keys, 0);
        assert_eq!(layout.row_count, 0);
        assert!(!layout.has_thumb_cluster);
    }

    #[test]
    fn layout_equality() {
        assert_eq!(
            create(34, &[10, 10, 10, 4], Some("Sweep/Cradio")),
            create(34, &[10, 10, 10, 4], Some("Sweep/Cradio"))
        );
        assert_ne!(
            create(34, &[10, 10, 10, 4], Some("Sweep/Cradio")),
            create(42, &[12, 12, 12, 6], Some("Corne"))
        );
    }

    #[test]
    fn detect_from_filename() {
        let info = KeyboardLayout::detect_keyboard_info("", Some("/tmp/config/cradio.keymap"));
        assert_eq!(info, Some(("Sweep/Cradio", true)));
    }

    #[test]
    fn detect_non_split_keyboard() {
        let info = KeyboardLayout::detect_keyboard_info("", Some("/tmp/config/planck.keymap"));
        assert_eq!(info, Some(("Planck", false)));
    }

    #[test]
    fn detect_go60_signature() {
        let info = KeyboardLayout::detect_keyboard_info("/* GO60 LAYOUT EDITOR */", None);
        assert_eq!(info.map(|i| i.0), Some("Go60"));
    }

    #[test]
    fn detect_unknown_keyboard() {
        assert_eq!(
            KeyboardLayout::detect_keyboard_info("nothing to see here", Some("/tmp/my.keymap")),
            None
        );
    }

    #[test]
    fn path_wins_over_content() {
        // Content mentions another board; the file name decides
        let info = KeyboardLayout::detect_keyboard_info("# ported from the corne", Some("C:\\cfg\\sofle.keymap"));
        assert_eq!(info, Some(("Sofle", true)));
    }
}
