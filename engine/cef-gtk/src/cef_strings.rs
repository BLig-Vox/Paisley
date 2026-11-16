//! Bound page-controlled UTF-16 before allocating a Rust copy.
use cef::{CefString, CefStringUserfree, sys};
pub fn borrowed(value: Option<&CefString>, limit: usize) -> String {
    value
        .and_then(|v| v.as_slice())
        .map(|s| decode(s, limit))
        .unwrap_or_default()
}
pub fn owned(value: &CefStringUserfree, limit: usize) -> String {
    let raw: Option<&sys::_cef_string_utf16_t> = value.into();
    let Some(raw) = raw.filter(|r| !r.str_.is_null() && r.length > 0) else {
        return String::new();
    };
    // CEF owns this allocation throughout the borrowed userfree value's lifetime.
    // Only the bounded prefix is exposed or copied.
    let prefix =
        unsafe { std::slice::from_raw_parts(raw.str_, raw.length.min(limit.saturating_mul(2))) };
    decode(prefix, limit)
}
fn decode(value: &[u16], limit: usize) -> String {
    char::decode_utf16(value.iter().copied().take(limit.saturating_mul(2)))
        .take(limit)
        .map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect()
}
