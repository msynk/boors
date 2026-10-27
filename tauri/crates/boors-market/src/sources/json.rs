//! Lenient serde adapters for source JSON. TSETMC sends integral quantities as `123.0`, insCodes as strings or
//! numbers (beyond 2^53), and placeholders such as `"-"` where a number belongs. These adapters read every shape the
//! .NET parsers accept and fall back to a neutral value where .NET would throw on malformed data.

use std::str::FromStr;

use rust_decimal::prelude::{FromPrimitive, ToPrimitive};
use serde::de::{self, Deserializer, IgnoredAny, Visitor};

use crate::Decimal;

/// C# `decimal.TryParse(text, NumberStyles.Float, InvariantCulture)`.
pub fn parse_decimal_float(text: &str) -> Option<Decimal> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    if text.contains(['e', 'E']) {
        return Decimal::from_scientific(text).ok();
    }
    let text = text.strip_prefix('+').unwrap_or(text);
    Decimal::from_str(text).ok()
}

/// An f64 parsed by serde_json (with `float_roundtrip`) back to the decimal the JSON text spelled.
pub fn decimal_from_f64(value: f64) -> Option<Decimal> {
    if !value.is_finite() {
        return None;
    }
    Decimal::from_str(&value.to_string()).ok().or_else(|| Decimal::from_f64(value))
}

#[derive(Clone, Copy)]
enum Number {
    Decimal(Decimal),
    Missing,
}

struct NumberVisitor {
    strings: bool,
}

impl<'de> Visitor<'de> for NumberVisitor {
    type Value = Number;

    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("a number")
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Number, E> {
        Ok(Number::Decimal(Decimal::from(v)))
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Number, E> {
        Ok(Number::Decimal(Decimal::from(v)))
    }

    fn visit_i128<E: de::Error>(self, v: i128) -> Result<Number, E> {
        Ok(Decimal::from_i128(v).map_or(Number::Missing, Number::Decimal))
    }

    fn visit_u128<E: de::Error>(self, v: u128) -> Result<Number, E> {
        Ok(Decimal::from_u128(v).map_or(Number::Missing, Number::Decimal))
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Number, E> {
        Ok(decimal_from_f64(v).map_or(Number::Missing, Number::Decimal))
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<Number, E> {
        if self.strings {
            Ok(parse_decimal_float(v).map_or(Number::Missing, Number::Decimal))
        } else {
            // .NET would throw on a string here; a malformed field reads as missing instead.
            Ok(Number::Missing)
        }
    }

    fn visit_bool<E: de::Error>(self, _: bool) -> Result<Number, E> {
        Ok(Number::Missing)
    }

    fn visit_unit<E: de::Error>(self) -> Result<Number, E> {
        Ok(Number::Missing)
    }

    fn visit_none<E: de::Error>(self) -> Result<Number, E> {
        Ok(Number::Missing)
    }

    fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Result<Number, D::Error> {
        deserializer.deserialize_any(self)
    }

    fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Number, A::Error> {
        while seq.next_element::<IgnoredAny>()?.is_some() {}
        Ok(Number::Missing)
    }

    fn visit_map<A: de::MapAccess<'de>>(self, mut map: A) -> Result<Number, A::Error> {
        while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
        Ok(Number::Missing)
    }
}

fn number<'de, D: Deserializer<'de>>(deserializer: D, strings: bool) -> Result<Number, D::Error> {
    deserializer.deserialize_any(NumberVisitor { strings })
}

/// `decimal` field: numbers (integral or not); null or garbage → 0.
pub fn decimal<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Decimal, D::Error> {
    Ok(match number(deserializer, true)? {
        Number::Decimal(d) => d,
        Number::Missing => Decimal::ZERO,
    })
}

/// `[JsonConverter(typeof(FlexibleDecimalConverter))] decimal?`: a number, a numeric string, or a placeholder → null.
pub fn flex_decimal<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Decimal>, D::Error> {
    Ok(match number(deserializer, true)? {
        Number::Decimal(d) => Some(d),
        Number::Missing => None,
    })
}

/// `int` field; a non-integral number truncates, garbage → 0.
pub fn int<'de, D: Deserializer<'de>>(deserializer: D) -> Result<i32, D::Error> {
    Ok(match number(deserializer, true)? {
        Number::Decimal(d) => d.trunc().to_i32().unwrap_or(0),
        Number::Missing => 0,
    })
}

/// `long` field.
pub fn long<'de, D: Deserializer<'de>>(deserializer: D) -> Result<i64, D::Error> {
    Ok(match number(deserializer, true)? {
        Number::Decimal(d) => d.trunc().to_i64().unwrap_or(0),
        Number::Missing => 0,
    })
}

/// `[JsonNumberHandling(AllowReadingFromString)] long?`.
pub fn opt_long<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<i64>, D::Error> {
    Ok(match number(deserializer, true)? {
        Number::Decimal(d) if d.fract().is_zero() => d.to_i64(),
        _ => None,
    })
}

/// `int?` (with or without AllowReadingFromString).
pub fn opt_int<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<i32>, D::Error> {
    Ok(match number(deserializer, true)? {
        Number::Decimal(d) if d.fract().is_zero() => d.to_i32(),
        _ => None,
    })
}

struct StringVisitor;

impl<'de> Visitor<'de> for StringVisitor {
    type Value = Option<String>;

    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("a string")
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
        Ok(Some(v.to_string()))
    }

    fn visit_string<E: de::Error>(self, v: String) -> Result<Self::Value, E> {
        Ok(Some(v))
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Self::Value, E> {
        Ok(Some(v.to_string()))
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> {
        Ok(Some(v.to_string()))
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Self::Value, E> {
        Ok(Some(v.to_string()))
    }

    fn visit_bool<E: de::Error>(self, v: bool) -> Result<Self::Value, E> {
        Ok(Some(if v { "true".into() } else { "false".into() }))
    }

    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_any(self)
    }

    fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        while seq.next_element::<IgnoredAny>()?.is_some() {}
        Ok(None)
    }

    fn visit_map<A: de::MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
        Ok(None)
    }
}

/// `string?` field; scalars of other kinds are read as their text.
pub fn opt_string<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    deserializer.deserialize_any(StringVisitor)
}

struct BoolVisitor;

impl<'de> Visitor<'de> for BoolVisitor {
    type Value = bool;

    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("a boolean")
    }

    fn visit_bool<E: de::Error>(self, v: bool) -> Result<bool, E> {
        Ok(v)
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<bool, E> {
        Ok(v != 0)
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<bool, E> {
        Ok(v != 0)
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<bool, E> {
        Ok(v != 0.0)
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<bool, E> {
        Ok(v.trim().eq_ignore_ascii_case("true"))
    }

    fn visit_unit<E: de::Error>(self) -> Result<bool, E> {
        Ok(false)
    }

    fn visit_none<E: de::Error>(self) -> Result<bool, E> {
        Ok(false)
    }

    fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Result<bool, D::Error> {
        deserializer.deserialize_any(self)
    }

    fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<bool, A::Error> {
        while seq.next_element::<IgnoredAny>()?.is_some() {}
        Ok(false)
    }

    fn visit_map<A: de::MapAccess<'de>>(self, mut map: A) -> Result<bool, A::Error> {
        while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
        Ok(false)
    }
}

/// `bool` field.
pub fn boolean<'de, D: Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> {
    deserializer.deserialize_any(BoolVisitor)
}

/// A list that may be missing, null or not a list at all (→ empty).
pub fn list<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    let value = Option::<Vec<T>>::deserialize(deserializer)?;
    Ok(value.unwrap_or_default())
}

/// Parses a JSON body into `T`; an empty body yields `None` (C# `Deserialize` on an empty span returns null).
pub fn parse<'a, T: serde::Deserialize<'a>>(json: &'a [u8]) -> crate::Result<Option<T>> {
    let json = json.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(json);
    if json.iter().all(u8::is_ascii_whitespace) {
        return Ok(None);
    }
    if json.trim_ascii() == b"null" {
        return Ok(None);
    }
    serde_json::from_slice::<T>(json).map(Some).map_err(|e| crate::Error::Format(format!("Invalid JSON: {e}")))
}

use serde::Deserialize as _;

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct Sample {
        #[serde(default, deserialize_with = "decimal")]
        a: Decimal,
        #[serde(default, deserialize_with = "flex_decimal")]
        b: Option<Decimal>,
        #[serde(default, deserialize_with = "opt_long")]
        c: Option<i64>,
        #[serde(default, deserialize_with = "int")]
        d: i32,
    }

    #[test]
    fn shapes_are_tolerated() {
        let s: Sample = serde_json::from_str(r#"{"a":3713955.9,"b":"-","c":"46348559193224090","d":1517.0}"#).unwrap();
        assert_eq!(Decimal::from_str("3713955.9").unwrap(), s.a);
        assert_eq!(None, s.b);
        assert_eq!(Some(46348559193224090), s.c);
        assert_eq!(1517, s.d);

        let s: Sample = serde_json::from_str(r#"{"a":null,"b":"9.6","c":46348559193224090,"d":null}"#).unwrap();
        assert_eq!(Decimal::ZERO, s.a);
        assert_eq!(Some(Decimal::from_str("9.6").unwrap()), s.b);
        assert_eq!(Some(46348559193224090), s.c);
        assert_eq!(0, s.d);
    }
}
