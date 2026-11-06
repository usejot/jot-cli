//! Small text helpers: the contract's `{name}` templates, one-line bodies,
//! where a comment was made, and the two encodings std does not carry
//! (URL query encoding, base64).

use serde_json::Value;

/// A contract template with each `{name}` replaced by its value, in one pass:
/// a value that itself contains `{x}` is never expanded again.
pub fn fill(template: &str, vars: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let value = after
            .find('}')
            .and_then(|close| vars.iter().find(|(key, _)| *key == &after[..close]).map(|(_, value)| (close, *value)));
        match value {
            Some((close, value)) => {
                out.push_str(value);
                rest = &after[close + 1..];
            }
            None => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// The text on one line, whitespace collapsed, cut to `max` characters with "…".
pub fn one_line(text: &str, max: usize) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max {
        return flat;
    }
    let mut cut: String = flat.chars().take(max - 1).collect();
    cut.push('…');
    cut
}

/// Where a comment was made, on one line: the page and selector for the web
/// widget; the screen, element and platform for a native app (whose url is a
/// synthetic app:// address that tells an agent little).
pub fn where_of(context: &serde_json::Map<String, Value>) -> String {
    let text = |key: &str| context.get(key).and_then(Value::as_str).unwrap_or("").to_string();
    let source = text("source");

    // 1. web: the context every comment had before the native SDKs
    if source != "mobile" && source != "desktop" {
        return format!("{} · {}", text("url"), text("domSelector"));
    }

    // 2. native: the screen, the element's label, the platform
    let label = context.get("anchor").and_then(anchor_label).unwrap_or_default();
    [text("screen"), label, text("platform")]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" · ")
}

/// contract anchorLabel: the identifier, else the label, else the text, else
/// the innermost element's type. None when the value is not an anchor.
fn anchor_label(anchor: &Value) -> Option<String> {
    let semantic = anchor.get("semantic")?.as_object()?;
    let path = anchor.get("path")?.as_array()?;
    let named = ["identifier", "label", "text"].iter().find_map(|key| semantic.get(*key).and_then(Value::as_str));
    if let Some(name) = named {
        return Some(name.to_string());
    }
    path.last()?.get("type")?.as_str().map(str::to_string)
}

/// Percent-encodes everything but the unreserved characters (RFC 3986).
pub fn url_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => out.push(byte as char),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Decodes a query component: `+` is a space, `%XX` a byte. Invalid escapes stay as they are.
pub fn url_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes.get(i + 1..i + 3).and_then(|pair| std::str::from_utf8(pair).ok());
        match (bytes[i], hex.and_then(|pair| u8::from_str_radix(pair, 16).ok())) {
            (b'%', Some(byte)) => {
                out.push(byte);
                i += 3;
            }
            (b'+', _) => {
                out.push(b' ');
                i += 1;
            }
            (byte, _) => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `a=1&b=2` → [("a", "1"), ("b", "2")], decoded.
pub fn parse_query(query: &str) -> Vec<(String, String)> {
    query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| match pair.split_once('=') {
            Some((key, value)) => (url_decode(key), url_decode(value)),
            None => (url_decode(pair), String::new()),
        })
        .collect()
}

/// Standard base64 with padding, for MCP image content.
pub fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        let symbols = chunk.len() + 1;
        for index in 0..4 {
            if index < symbols {
                out.push(ALPHABET[(n >> (18 - 6 * index) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn fills_each_placeholder_once() {
        assert_eq!(fill("{a} and {b}, {missing}", &[("a", "{b}"), ("b", "2")]), "{b} and 2, {missing}");
    }

    #[test]
    fn cuts_to_one_line() {
        assert_eq!(one_line("  a\n  b  ", 70), "a b");
        assert_eq!(one_line("abcdef", 4), "abc…");
    }

    #[test]
    fn says_where_on_web_and_native() {
        let web = json!({ "url": "https://x.test/a", "domSelector": "#pay" });
        assert_eq!(where_of(web.as_object().unwrap()), "https://x.test/a · #pay");
        let native = json!({
            "source": "mobile", "screen": "Checkout", "platform": "ios",
            "anchor": { "semantic": { "label": "Pay" }, "path": [{ "type": "Button" }] }
        });
        assert_eq!(where_of(native.as_object().unwrap()), "Checkout · Pay · ios");
        let bare = json!({ "source": "desktop", "anchor": { "semantic": {}, "path": [{ "type": "Button" }] } });
        assert_eq!(where_of(bare.as_object().unwrap()), "Button");
    }

    #[test]
    fn encodes_urls_and_base64() {
        assert_eq!(url_encode("a b/é"), "a%20b%2F%C3%A9");
        assert_eq!(url_decode("a+b%2Fc%zz"), "a b/c%zz");
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"M"), "TQ==");
    }
}
