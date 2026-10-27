//! Byte-compatible imitation of System.Text.Json's indented output with the default encoder, so sidecars written by
//! this implementation look exactly like those written by the .NET collector.

use chrono::{DateTime, Timelike, Utc};

pub enum JsonValue<'a> {
    Str(&'a str),
    /// Already-formatted JSON (numbers) or a string that needs escaping (dates are passed through `Raw` quoted).
    Raw(String),
    Null,
}

#[cfg(windows)]
const NEW_LINE: &str = "\r\n";
#[cfg(not(windows))]
const NEW_LINE: &str = "\n";

/// `{` NEWLINE `  "Name": value,` ... NEWLINE `}` with no trailing newline (JsonSerializerOptions.WriteIndented).
pub fn write_indented_object(fields: &[(&str, JsonValue)]) -> String {
    let mut out = String::with_capacity(512);
    out.push('{');
    for (i, (name, value)) in fields.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(NEW_LINE);
        out.push_str("  ");
        escape_into(name, &mut out);
        out.push_str(": ");
        match value {
            JsonValue::Str(s) => escape_into(s, &mut out),
            JsonValue::Raw(r) => out.push_str(r),
            JsonValue::Null => out.push_str("null"),
        }
    }
    out.push_str(NEW_LINE);
    out.push('}');
    out
}

/// JavaScriptEncoder.Default: only printable ASCII outside the HTML-sensitive set stays literal.
pub fn escape_into(value: &str, out: &mut String) {
    out.push('"');
    for unit in value.encode_utf16() {
        match unit {
            0x08 => out.push_str("\\b"),
            0x09 => out.push_str("\\t"),
            0x0A => out.push_str("\\n"),
            0x0C => out.push_str("\\f"),
            0x0D => out.push_str("\\r"),
            0x5C => out.push_str("\\\\"),
            0x22 | 0x26 | 0x27 | 0x2B | 0x3C | 0x3E | 0x60 => push_unicode_escape(unit, out),
            0x20..=0x7E => out.push(char::from(unit as u8)),
            _ => push_unicode_escape(unit, out),
        }
    }
    out.push('"');
}

fn push_unicode_escape(unit: u16, out: &mut String) {
    out.push_str(&format!("\\u{unit:04X}"));
}

/// DateTimeOffset as System.Text.Json writes it: `2026-10-17T13:10:31.9482894+00:00` (fraction trimmed).
pub fn format_date_time_offset(value: DateTime<Utc>) -> String {
    let ticks = value.nanosecond() % 1_000_000_000 / 100;
    let mut text = value.format("%Y-%m-%dT%H:%M:%S").to_string();
    if ticks > 0 {
        let fraction = format!("{ticks:07}");
        text.push('.');
        text.push_str(fraction.trim_end_matches('0'));
    }
    text.push_str("+00:00");
    format!("\"{text}\"")
}

/// serde adapter for `DateTimeOffset` fields of the sidecars.
pub mod date_time_offset {
    use chrono::{DateTime, Utc};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(value: &DateTime<Utc>, serializer: S) -> Result<S::Ok, S::Error> {
        let quoted = super::format_date_time_offset(*value);
        serializer.serialize_str(quoted.trim_matches('"'))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<DateTime<Utc>, D::Error> {
        let text = String::deserialize(deserializer)?;
        DateTime::parse_from_rfc3339(&text)
            .map(|d| d.with_timezone(&Utc))
            .map_err(|e| serde::de::Error::custom(format!("invalid DateTimeOffset '{text}': {e}")))
    }
}
