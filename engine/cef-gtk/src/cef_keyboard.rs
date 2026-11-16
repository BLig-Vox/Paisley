// Chromium virtual-key codes are not ASCII character codes.
pub(crate) fn windows_key_code(key: &str) -> i32 {
    match key {
        "Backspace" => 8,
        "Tab" => 9,
        "Enter" => 13,
        "Shift" => 16,
        "Control" => 17,
        "Alt" => 18,
        "Escape" => 27,
        " " => 32,
        "PageUp" => 33,
        "PageDown" => 34,
        "End" => 35,
        "Home" => 36,
        "ArrowLeft" => 37,
        "ArrowUp" => 38,
        "ArrowRight" => 39,
        "ArrowDown" => 40,
        "Insert" => 45,
        "Delete" => 46,
        ";" | ":" => 0xba,
        "=" | "+" => 0xbb,
        "," | "<" => 0xbc,
        "-" | "_" => 0xbd,
        "." | ">" => 0xbe,
        "/" | "?" => 0xbf,
        "`" | "~" => 0xc0,
        "[" | "{" => 0xdb,
        "\\" | "|" => 0xdc,
        "]" | "}" => 0xdd,
        "'" | "\"" => 0xde,
        // Shifted number-row characters retain their physical virtual key.
        ")" => 0x30,
        "!" => 0x31,
        "@" => 0x32,
        "#" => 0x33,
        "$" => 0x34,
        "%" => 0x35,
        "^" => 0x36,
        "&" => 0x37,
        "*" => 0x38,
        "(" => 0x39,
        _ => key
            .chars()
            .next()
            .filter(|c| key.chars().count() == 1 && c.is_ascii_alphanumeric())
            .map(|c| c.to_ascii_uppercase() as i32)
            .unwrap_or(0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn period_is_not_delete() {
        assert_eq!(windows_key_code("."), 0xbe);
        assert_eq!(windows_key_code("Delete"), 0x2e);
    }
    #[test]
    fn punctuation_uses_chromium_oem_keys() {
        for (keys, code) in [
            (";:", 0xba),
            ("=+", 0xbb),
            (",<", 0xbc),
            ("-_", 0xbd),
            (".>", 0xbe),
            ("/?", 0xbf),
            ("`~", 0xc0),
            ("[{", 0xdb),
            ("\\|", 0xdc),
            ("]}", 0xdd),
            ("'\"", 0xde),
        ] {
            for key in keys.chars() {
                assert_eq!(windows_key_code(&key.to_string()), code, "{key}");
            }
        }
    }
    #[test]
    fn shifted_digits_and_unicode_do_not_become_navigation_keys() {
        for (key, code) in ")!@#$%^&*(".chars().zip(0x30..=0x39) {
            assert_eq!(windows_key_code(&key.to_string()), code);
        }
        for key in ["日", "中", "é", "🦋", "Unknown"] {
            assert_eq!(windows_key_code(key), 0);
        }
    }
    #[test]
    fn text_does_not_collide_with_navigation() {
        assert_eq!(windows_key_code("a"), 65);
        assert_eq!(windows_key_code("Z"), 90);
        assert_eq!(windows_key_code("7"), 55);
        assert_eq!(windows_key_code("ArrowLeft"), 37);
        assert_eq!(windows_key_code("Backspace"), 8);
        assert_eq!(windows_key_code("Enter"), 13);
    }
}
