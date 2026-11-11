//! A small, dependency-free JSON parser.
//!
//! Enough for plugin manifests' assets, config schemas, migrations, and UI
//! trees. It is strict (no trailing commas, no comments) and reports byte
//! offsets so manifest authors get useful errors.

use std::collections::BTreeMap;
use std::fmt;

#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Json>),
    Object(BTreeMap<String, Json>),
}

impl Json {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::String(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Json::Bool(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Json::Number(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_object(&self) -> Option<&BTreeMap<String, Json>> {
        match self {
            Json::Object(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Json]> {
        match self {
            Json::Array(value) => Some(value),
            _ => None,
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Json::Null => "null",
            Json::Bool(_) => "boolean",
            Json::Number(_) => "number",
            Json::String(_) => "string",
            Json::Array(_) => "array",
            Json::Object(_) => "object",
        }
    }

    /// Render as compact JSON. Numbers keep their integer form when exact.
    pub fn render(&self) -> String {
        let mut out = String::new();
        self.write(&mut out);
        out
    }

    fn write(&self, out: &mut String) {
        match self {
            Json::Null => out.push_str("null"),
            Json::Bool(true) => out.push_str("true"),
            Json::Bool(false) => out.push_str("false"),
            Json::Number(value) => {
                if value.fract() == 0.0 && value.abs() < 1e15 {
                    out.push_str(&format!("{}", *value as i64));
                } else {
                    out.push_str(&format!("{value}"));
                }
            }
            Json::String(value) => write_string(value, out),
            Json::Array(values) => {
                out.push('[');
                for (index, value) in values.iter().enumerate() {
                    if index > 0 {
                        out.push(',');
                    }
                    value.write(out);
                }
                out.push(']');
            }
            Json::Object(values) => {
                out.push('{');
                for (index, (key, value)) in values.iter().enumerate() {
                    if index > 0 {
                        out.push(',');
                    }
                    write_string(key, out);
                    out.push(':');
                    value.write(out);
                }
                out.push('}');
            }
        }
    }
}

impl fmt::Display for Json {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.render())
    }
}

fn write_string(value: &str, out: &mut String) {
    out.push('"');
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            character if (character as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", character as u32));
            }
            character => out.push(character),
        }
    }
    out.push('"');
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsonError {
    pub offset: usize,
    pub message: String,
}

impl fmt::Display for JsonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "json error at byte {}: {}", self.offset, self.message)
    }
}

impl std::error::Error for JsonError {}

pub fn parse(text: &str) -> Result<Json, JsonError> {
    let mut parser = Parser {
        bytes: text.as_bytes(),
        pos: 0,
    };
    parser.skip_whitespace();
    let value = parser.value()?;
    parser.skip_whitespace();
    if parser.pos != parser.bytes.len() {
        return Err(parser.error("trailing characters after the document"));
    }
    Ok(value)
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn error(&self, message: impl Into<String>) -> JsonError {
        JsonError {
            offset: self.pos,
            message: message.into(),
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let byte = self.peek()?;
        self.pos += 1;
        Some(byte)
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
    }

    fn expect(&mut self, byte: u8) -> Result<(), JsonError> {
        match self.bump() {
            Some(found) if found == byte => Ok(()),
            _ => Err(self.error(format!("expected '{}'", byte as char))),
        }
    }

    fn value(&mut self) -> Result<Json, JsonError> {
        match self.peek() {
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => Ok(Json::String(self.string()?)),
            Some(b't') => self.literal("true", Json::Bool(true)),
            Some(b'f') => self.literal("false", Json::Bool(false)),
            Some(b'n') => self.literal("null", Json::Null),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(other) => Err(self.error(format!("unexpected character '{}'", other as char))),
            None => Err(self.error("unexpected end of input")),
        }
    }

    fn literal(&mut self, text: &str, value: Json) -> Result<Json, JsonError> {
        if self.bytes[self.pos..].starts_with(text.as_bytes()) {
            self.pos += text.len();
            Ok(value)
        } else {
            Err(self.error(format!("expected '{text}'")))
        }
    }

    fn object(&mut self) -> Result<Json, JsonError> {
        self.expect(b'{')?;
        let mut map = BTreeMap::new();
        self.skip_whitespace();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(Json::Object(map));
        }
        loop {
            self.skip_whitespace();
            let key = self.string()?;
            self.skip_whitespace();
            self.expect(b':')?;
            self.skip_whitespace();
            let value = self.value()?;
            if map.insert(key.clone(), value).is_some() {
                return Err(self.error(format!("duplicate key '{key}'")));
            }
            self.skip_whitespace();
            match self.bump() {
                Some(b',') => continue,
                Some(b'}') => break,
                _ => return Err(self.error("expected ',' or '}' in object")),
            }
        }
        Ok(Json::Object(map))
    }

    fn array(&mut self) -> Result<Json, JsonError> {
        self.expect(b'[')?;
        let mut values = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(Json::Array(values));
        }
        loop {
            self.skip_whitespace();
            values.push(self.value()?);
            self.skip_whitespace();
            match self.bump() {
                Some(b',') => continue,
                Some(b']') => break,
                _ => return Err(self.error("expected ',' or ']' in array")),
            }
        }
        Ok(Json::Array(values))
    }

    fn string(&mut self) -> Result<String, JsonError> {
        self.expect(b'"')?;
        let mut out = String::new();
        loop {
            match self.bump() {
                Some(b'"') => return Ok(out),
                Some(b'\\') => self.escape(&mut out)?,
                Some(byte) if byte < 0x20 => {
                    return Err(self.error("control character in string"));
                }
                Some(byte) if byte < 0x80 => out.push(byte as char),
                Some(byte) => {
                    // Multi-byte UTF-8: decode the whole character, not each
                    // byte independently.
                    let start = self.pos - 1;
                    let width =
                        utf8_width(byte).ok_or_else(|| self.error("invalid UTF-8 leading byte"))?;
                    let end = start + width;
                    if end > self.bytes.len() {
                        return Err(self.error("truncated UTF-8 sequence"));
                    }
                    let text = std::str::from_utf8(&self.bytes[start..end])
                        .map_err(|_| self.error("invalid UTF-8 sequence"))?;
                    out.push_str(text);
                    self.pos = end;
                }
                None => return Err(self.error("unterminated string")),
            }
        }
    }

    fn escape(&mut self, out: &mut String) -> Result<(), JsonError> {
        let byte = self
            .bump()
            .ok_or_else(|| self.error("unterminated escape"))?;
        match byte {
            b'"' => out.push('"'),
            b'\\' => out.push('\\'),
            b'/' => out.push('/'),
            b'b' => out.push('\u{08}'),
            b'f' => out.push('\u{0c}'),
            b'n' => out.push('\n'),
            b'r' => out.push('\r'),
            b't' => out.push('\t'),
            b'u' => {
                let mut code = self.hex4()?;
                if (0xD800..=0xDBFF).contains(&code) {
                    // High surrogate: expect a low surrogate.
                    if self.bump() != Some(b'\\') || self.bump() != Some(b'u') {
                        return Err(self.error("expected a low surrogate"));
                    }
                    let low = self.hex4()?;
                    if !(0xDC00..=0xDFFF).contains(&low) {
                        return Err(self.error("invalid low surrogate"));
                    }
                    code = 0x10000 + ((code - 0xD800) << 10) + (low - 0xDC00);
                }
                let character =
                    char::from_u32(code).ok_or_else(|| self.error("invalid unicode escape"))?;
                out.push(character);
            }
            other => {
                return Err(self.error(format!("invalid escape '\\{}'", other as char)));
            }
        }
        Ok(())
    }

    fn hex4(&mut self) -> Result<u32, JsonError> {
        let mut value = 0u32;
        for _ in 0..4 {
            let byte = self
                .bump()
                .ok_or_else(|| self.error("truncated unicode escape"))?;
            let digit = match byte {
                b'0'..=b'9' => (byte - b'0') as u32,
                b'a'..=b'f' => (byte - b'a' + 10) as u32,
                b'A'..=b'F' => (byte - b'A' + 10) as u32,
                _ => return Err(self.error("invalid hex digit in unicode escape")),
            };
            value = value * 16 + digit;
        }
        Ok(value)
    }

    fn number(&mut self) -> Result<Json, JsonError> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.pos += 1;
        }
        if self.peek() == Some(b'.') {
            self.pos += 1;
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.pos += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
        }
        let text = std::str::from_utf8(&self.bytes[start..self.pos])
            .map_err(|_| self.error("invalid number"))?;
        let value: f64 = text
            .parse()
            .map_err(|_| self.error(format!("invalid number '{text}'")))?;
        Ok(Json::Number(value))
    }
}

fn utf8_width(byte: u8) -> Option<usize> {
    match byte {
        0xC0..=0xDF => Some(2),
        0xE0..=0xEF => Some(3),
        0xF0..=0xF7 => Some(4),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_ui_tree() {
        let text = r#"{
            "id": "panel.top",
            "type": "panel",
            "layout": "row",
            "children": [
                { "id": "clock", "type": "clock", "props": { "position": "left" } }
            ]
        }"#;
        let value = parse(text).expect("parse");
        let object = value.as_object().expect("object");
        assert_eq!(object.get("id").and_then(Json::as_str), Some("panel.top"));
        let children = object
            .get("children")
            .and_then(Json::as_array)
            .expect("children");
        assert_eq!(children.len(), 1);
    }

    #[test]
    fn round_trips_through_render() {
        let text = r#"{"a":1,"b":true,"c":["x",null],"d":{"e":2.5}}"#;
        let value = parse(text).expect("parse");
        let rendered = value.render();
        assert_eq!(parse(&rendered).expect("reparse"), value);
    }

    #[test]
    fn rejects_trailing_junk() {
        assert!(parse("{} x").is_err());
    }

    #[test]
    fn handles_escapes_and_surrogates() {
        let value = parse(r#""line\n\u0041\uD83D\uDE00""#).expect("parse");
        assert_eq!(value.as_str(), Some("line\nA😀"));
    }

    #[test]
    fn round_trips_multibyte_utf8() {
        let value = parse("{\"title\":\"Tiaki — café ☕\"}").expect("parse");
        let rendered = value.render();
        assert_eq!(parse(&rendered).expect("reparse"), value);
        assert_eq!(
            value
                .as_object()
                .and_then(|object| object.get("title"))
                .and_then(Json::as_str),
            Some("Tiaki — café ☕")
        );
    }
}
