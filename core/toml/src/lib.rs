//! A strict subset of TOML: tables, string/integer/boolean values, and arrays
//! of strings or integers. That is exactly the manifest grammar, and nothing
//! more, so manifests cannot smuggle in surprising constructs.

use std::collections::BTreeMap;
use std::fmt;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    String(String),
    Integer(i64),
    Bool(bool),
    Array(Vec<Value>),
}

impl Value {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Integer(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Array(values) => Some(values),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Document {
    tables: BTreeMap<String, BTreeMap<String, Value>>,
}

impl Document {
    pub fn table(&self, name: &str) -> Option<&BTreeMap<String, Value>> {
        self.tables.get(name)
    }

    pub fn tables(&self) -> &BTreeMap<String, BTreeMap<String, Value>> {
        &self.tables
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TomlError {
    pub line: usize,
    pub message: String,
}

impl fmt::Display for TomlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "toml error on line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for TomlError {}

pub fn parse(text: &str) -> Result<Document, TomlError> {
    let mut document = Document::default();
    let mut current = String::new();

    for (index, raw) in text.lines().enumerate() {
        let line_number = index + 1;
        let line = strip_comment(raw);
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if let Some(rest) = line.strip_prefix('[') {
            let name = rest
                .strip_suffix(']')
                .ok_or_else(|| error(line_number, "unterminated table header"))?;
            let name = name.trim();
            if name.is_empty() || name.contains(char::is_whitespace) {
                return Err(error(line_number, "invalid table name"));
            }
            current = name.to_string();
            document.tables.entry(current.clone()).or_default();
            continue;
        }

        let (key, raw_value) =
            split_assignment(line).ok_or_else(|| error(line_number, "expected 'key = value'"))?;
        let key = key.trim();
        if key.is_empty() || key.contains(char::is_whitespace) {
            return Err(error(line_number, "invalid key"));
        }
        let value = parse_value(raw_value.trim(), line_number)?;
        document
            .tables
            .entry(current.clone())
            .or_default()
            .insert(key.to_string(), value);
    }

    Ok(document)
}

fn error(line: usize, message: impl Into<String>) -> TomlError {
    TomlError {
        line,
        message: message.into(),
    }
}

/// Remove a `#` comment that is not inside a string.
fn strip_comment(line: &str) -> String {
    let mut out = String::new();
    let mut in_string = false;
    let mut escaped = false;
    for character in line.chars() {
        if in_string {
            out.push(character);
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
            continue;
        }
        if character == '"' {
            in_string = true;
            out.push(character);
            continue;
        }
        if character == '#' {
            break;
        }
        out.push(character);
    }
    out
}

/// Split at the first `=` that is not inside a string.
fn split_assignment(line: &str) -> Option<(&str, &str)> {
    let mut in_string = false;
    let mut escaped = false;
    for (index, character) in line.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
            continue;
        }
        if character == '"' {
            in_string = true;
            continue;
        }
        if character == '=' {
            return Some((&line[..index], &line[index + 1..]));
        }
    }
    None
}

fn parse_value(text: &str, line: usize) -> Result<Value, TomlError> {
    if text.is_empty() {
        return Err(error(line, "missing value"));
    }
    if text.starts_with('"') {
        let (value, rest) = parse_string(text, line)?;
        if !rest.trim().is_empty() {
            return Err(error(line, "unexpected characters after string"));
        }
        return Ok(Value::String(value));
    }
    if text.starts_with('[') {
        return parse_array(text, line);
    }
    if text == "true" {
        return Ok(Value::Bool(true));
    }
    if text == "false" {
        return Ok(Value::Bool(false));
    }
    let value: i64 = text
        .parse()
        .map_err(|_| error(line, format!("unsupported value '{text}'")))?;
    Ok(Value::Integer(value))
}

/// Parse a quoted string, returning it and the remaining text.
fn parse_string(text: &str, line: usize) -> Result<(String, &str), TomlError> {
    let bytes = text.as_bytes();
    debug_assert_eq!(bytes.first(), Some(&b'"'));
    let mut out = String::new();
    let mut index = 1;
    while index < bytes.len() {
        match bytes[index] {
            b'"' => return Ok((out, &text[index + 1..])),
            b'\\' => {
                index += 1;
                let escaped = *bytes
                    .get(index)
                    .ok_or_else(|| error(line, "unterminated escape"))?;
                match escaped {
                    b'"' => out.push('"'),
                    b'\\' => out.push('\\'),
                    b'n' => out.push('\n'),
                    b't' => out.push('\t'),
                    b'r' => out.push('\r'),
                    other => {
                        return Err(error(
                            line,
                            format!("unsupported escape '\\{}'", other as char),
                        ));
                    }
                }
                index += 1;
            }
            byte => {
                if byte < 0x80 {
                    out.push(byte as char);
                    index += 1;
                } else {
                    let width = match byte {
                        0xC0..=0xDF => 2,
                        0xE0..=0xEF => 3,
                        0xF0..=0xF7 => 4,
                        _ => return Err(error(line, "invalid UTF-8 in string")),
                    };
                    let end = index + width;
                    if end > bytes.len() {
                        return Err(error(line, "truncated UTF-8 in string"));
                    }
                    let text = std::str::from_utf8(&bytes[index..end])
                        .map_err(|_| error(line, "invalid UTF-8 in string"))?;
                    out.push_str(text);
                    index = end;
                }
            }
        }
    }
    Err(error(line, "unterminated string"))
}

fn parse_array(text: &str, line: usize) -> Result<Value, TomlError> {
    let inner = text
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .ok_or_else(|| error(line, "unterminated array"))?;
    let mut values = Vec::new();
    let mut rest = inner.trim();
    if rest.is_empty() {
        return Ok(Value::Array(values));
    }
    loop {
        rest = rest.trim_start();
        if rest.is_empty() {
            return Err(error(line, "trailing comma in array"));
        }
        if rest.starts_with('"') {
            let (value, remaining) = parse_string(rest, line)?;
            values.push(Value::String(value));
            rest = remaining.trim_start();
        } else {
            let end = rest.find(',').unwrap_or(rest.len());
            let token = rest[..end].trim();
            let value: i64 = token
                .parse()
                .map_err(|_| error(line, format!("unsupported array value '{token}'")))?;
            values.push(Value::Integer(value));
            rest = &rest[end..];
        }
        if rest.is_empty() {
            break;
        }
        if let Some(after) = rest.strip_prefix(',') {
            rest = after;
            continue;
        }
        return Err(error(line, "expected ',' or ']' in array"));
    }
    Ok(Value::Array(values))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_manifest() {
        let text = r#"
# comment
[plugin]
id = "org.tiaki.panel.clock"
version = "1.2.0"
api = "1"

[capabilities]
requires = ["ui.surface.create", "time.read"]

[resources]
cpu_weight = 100
memory_max = "64M"
"#;
        let document = parse(text).expect("parse");
        let plugin = document.table("plugin").expect("plugin");
        assert_eq!(
            plugin.get("id").and_then(Value::as_str),
            Some("org.tiaki.panel.clock")
        );
        let capabilities = document.table("capabilities").expect("capabilities");
        assert_eq!(
            capabilities
                .get("requires")
                .and_then(Value::as_array)
                .map(|values| values.len()),
            Some(2)
        );
        let resources = document.table("resources").expect("resources");
        assert_eq!(
            resources.get("cpu_weight").and_then(Value::as_i64),
            Some(100)
        );
    }

    #[test]
    fn ignores_hashes_inside_strings() {
        let document = parse("[plugin]\nid = \"a#b\" # tail\n").expect("parse");
        assert_eq!(
            document
                .table("plugin")
                .and_then(|table| table.get("id"))
                .and_then(Value::as_str),
            Some("a#b")
        );
    }

    #[test]
    fn rejects_unsupported_syntax() {
        assert!(parse("[plugin]\nid = 1.5\n").is_err());
        assert!(parse("[plugin\nid = \"x\"\n").is_err());
    }
}
