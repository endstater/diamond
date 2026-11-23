// SPDX-License-Identifier: GPL-3.0-only
//! GUI-independent RGB notation used by appearance settings.
pub fn parse_hex(input: &str) -> Option<[u8; 3]> {
    let input = input.trim();
    let value = input.strip_prefix('#').unwrap_or(input);
    if value.len() != 6 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    Some([
        u8::from_str_radix(&value[0..2], 16).ok()?,
        u8::from_str_radix(&value[2..4], 16).ok()?,
        u8::from_str_radix(&value[4..6], 16).ok()?,
    ])
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn six_digit_codes_are_case_insensitive_and_partial_input_is_rejected() {
        assert_eq!(parse_hex(" #FF8998 "), Some([255, 137, 152]));
        assert_eq!(parse_hex("7bb2ff"), Some([123, 178, 255]));
        for value in ["#fff", "#12345", "#1234567", "#GG0000", "１２３", "#é1234"] {
            assert_eq!(parse_hex(value), None);
        }
    }
}
