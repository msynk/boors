//! Persian text normalization (C# `PersianText`, built on DNTPersianUtils.Core 7.0.2) and Iran time.
//!
//! The character tables below were extracted from DNTPersianUtils.Core 7.0.2 by running `ApplyCorrectYeKe` and
//! `ToEnglishNumbers` over every BMP character, so the behaviour is identical, not approximate.

use std::collections::HashSet;
use std::sync::LazyLock;

use chrono::{DateTime, Datelike, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, Offset, TimeZone, Utc};
use chrono_tz::Asia::Tehran;

use crate::Date;

/// Arabic and other Ye/Ke variants that DNTPersianUtils' `ApplyCorrectYeKe` rewrites.
fn correct_ye_ke(c: char) -> char {
    match c {
        '\u{0620}' | '\u{063D}' | '\u{063E}' | '\u{063F}' | '\u{0649}' | '\u{064A}' | '\u{0678}' | '\u{06CD}' | '\u{06CE}' | '\u{06D0}' | '\u{06D1}' => {
            '\u{06CC}'
        }
        '\u{0643}' => '\u{06A9}',
        _ => c,
    }
}

/// Persian (U+06F0..) and Arabic-Indic (U+0660..) digits become ASCII (`ToEnglishNumbers`).
fn english_digit(c: char) -> char {
    match c {
        '\u{06F0}'..='\u{06F9}' => char::from(b'0' + (c as u32 - 0x06F0) as u8),
        '\u{0660}'..='\u{0669}' => char::from(b'0' + (c as u32 - 0x0660) as u8),
        _ => c,
    }
}

/// .NET `string.IsNullOrWhiteSpace` (same character set as Rust's `char::is_whitespace`).
pub fn is_blank(value: &str) -> bool {
    value.chars().all(char::is_whitespace)
}

/// Arabic ي/ك become Persian ی/ک, digits become ASCII, and surrounding whitespace is trimmed.
/// Returns `None` for blank input.
pub fn normalize(value: Option<&str>) -> Option<String> {
    let value = value?;
    if is_blank(value) {
        return None;
    }

    let mapped: String = value.chars().map(|c| english_digit(correct_ye_ke(c))).collect();
    Some(mapped.trim().to_string())
}

/// [`normalize`] for a non-optional string.
pub fn normalize_str(value: &str) -> Option<String> {
    normalize(Some(value))
}

/// The spelling TSETMC stores internally: Persian ی/ک replaced by Arabic ي/ك.
pub fn to_arabic_letters(value: &str) -> String {
    value
        .chars()
        .map(|c| match c {
            'ی' => 'ي',
            'ک' => 'ك',
            _ => c,
        })
        .collect()
}

/// A comparison key for symbol and name matching: normalized, with ZWNJ and spaces removed.
pub fn match_key(value: &str) -> String {
    normalize_str(value).unwrap_or_default().replace(['\u{200C}', ' '], "")
}

/// Parses a Jalali timestamp such as "۱۴۰۵/۰۷/۰۷ ۱۸:۵۵:۳۴" or "1405/07/07" into an Iran-local timestamp.
pub fn parse_jalali(value: Option<&str>) -> Option<DateTime<FixedOffset>> {
    let text = normalize(value)?;
    let parts: Vec<&str> = text.split(' ').filter(|p| !p.is_empty()).collect();
    let date: Vec<&str> = parts.first()?.split(['/', '-']).collect();
    if date.len() != 3 {
        return None;
    }
    let year = parse_int(date[0])?;
    let month = parse_int(date[1])?;
    let day = parse_int(date[2])?;

    let (mut hour, mut minute, mut second) = (0, 0, 0);
    if parts.len() > 1 {
        let time: Vec<&str> = parts[1].split(':').collect();
        if time.len() < 2 {
            return None;
        }
        hour = parse_int(time[0])?;
        minute = parse_int(time[1])?;
        if time.len() > 2 {
            second = parse_int(time[2])?;
        }
    }

    let date = jalali_to_gregorian(year, month, day)?;
    let time = NaiveTime::from_hms_opt(u32::try_from(hour).ok()?, u32::try_from(minute).ok()?, u32::try_from(second).ok()?)?;
    Some(iran_offset(date.and_time(time)))
}

/// .NET `int.TryParse(s, InvariantCulture)`: optional surrounding whitespace and a leading sign.
fn parse_int(value: &str) -> Option<i32> {
    value.trim().parse::<i32>().ok()
}

// ------------------------------------------------------------------------------------------------ Jalali calendar

const BREAKS: [i32; 20] = [-61, 9, 38, 199, 426, 686, 756, 818, 1111, 1181, 1210, 1635, 2060, 2097, 2192, 2262, 2324, 2394, 2456, 3178];

fn div(a: i32, b: i32) -> i32 {
    a / b
}

fn modulo(a: i32, b: i32) -> i32 {
    a - (a / b) * b
}

/// Borkowski's algorithm (jalaali-js): Gregorian year, March day of 1 Farvardin and the leap indicator (0 = leap).
fn jal_cal(jy: i32) -> Option<(i32, i32, i32)> {
    let bl = BREAKS.len();
    let gy = jy + 621;
    let mut leap_j = -14;
    let mut jp = BREAKS[0];
    if jy < jp || jy >= BREAKS[bl - 1] {
        return None;
    }

    let mut jump = 0;
    for &jm in BREAKS.iter().skip(1) {
        jump = jm - jp;
        if jy < jm {
            break;
        }
        leap_j += div(jump, 33) * 8 + div(modulo(jump, 33), 4);
        jp = jm;
    }

    let mut n = jy - jp;
    leap_j += div(n, 33) * 8 + div(modulo(n, 33) + 3, 4);
    if modulo(jump, 33) == 4 && jump - n == 4 {
        leap_j += 1;
    }
    let leap_g = div(gy, 4) - div((div(gy, 100) + 1) * 3, 4) - 150;
    let march = 20 + leap_j - leap_g;

    if jump - n < 6 {
        n = n - jump + div(jump + 4, 33) * 33;
    }
    let mut leap = modulo(modulo(n + 1, 33) - 1, 4);
    if leap == -1 {
        leap = 4;
    }
    Some((gy, march, leap))
}

/// True when the Jalali year has 366 days.
pub fn is_jalali_leap_year(year: i32) -> bool {
    jal_cal(year).is_some_and(|(_, _, leap)| leap == 0)
}

/// Days in a Jalali month.
pub fn jalali_month_length(year: i32, month: i32) -> i32 {
    match month {
        1..=6 => 31,
        7..=11 => 30,
        12 if is_jalali_leap_year(year) => 30,
        _ => 29,
    }
}

/// Jalali → Gregorian; `None` for out-of-range values (like .NET `PersianCalendar.ToDateTime` throwing).
pub fn jalali_to_gregorian(year: i32, month: i32, day: i32) -> Option<NaiveDate> {
    if !(1..=12).contains(&month) || day < 1 || day > jalali_month_length(year, month) {
        return None;
    }
    let (gy, march, _) = jal_cal(year)?;
    let farvardin1 = NaiveDate::from_ymd_opt(gy, 3, u32::try_from(march).ok()?)?;
    let offset = (month - 1) * 31 - div(month, 7) * (month - 7) + day - 1;
    farvardin1.checked_add_signed(chrono::Duration::days(i64::from(offset)))
}

/// Gregorian → Jalali (year, month, day).
pub fn gregorian_to_jalali(date: NaiveDate) -> (i32, i32, i32) {
    let mut jy = date.year() - 621;
    let farvardin1 = |y: i32| jalali_to_gregorian(y, 1, 1);
    if farvardin1(jy).is_some_and(|start| date < start) {
        jy -= 1;
    }
    let start = farvardin1(jy).unwrap_or(date);
    let mut days = (date - start).num_days() as i32;
    let mut month = 1;
    while month < 12 && days >= jalali_month_length(jy, month) {
        days -= jalali_month_length(jy, month);
        month += 1;
    }
    (jy, month, days + 1)
}

// ------------------------------------------------------------------------------------------------ Iran time

/// Iran wall-clock time → instant, using the Asia/Tehran rules (DST before 2022). For a skipped or repeated hour,
/// .NET `TimeZoneInfo.GetUtcOffset` answers the standard offset; so does this.
pub fn iran_offset(local: NaiveDateTime) -> DateTime<FixedOffset> {
    let standard = FixedOffset::east_opt(3 * 3600 + 1800).unwrap_or_else(|| Utc.fix());
    let offset = match Tehran.offset_from_local_datetime(&local) {
        chrono::LocalResult::Single(offset) => offset.fix(),
        _ => standard,
    };
    match offset.from_local_datetime(&local) {
        chrono::LocalResult::Single(value) => value,
        _ => standard.from_utc_datetime(&(local - chrono::Duration::seconds(i64::from(standard.local_minus_utc())))),
    }
}

/// The current Iran-local time.
pub fn iran_now() -> DateTime<FixedOffset> {
    to_iran(Utc::now())
}

/// Converts an instant to Iran local time.
pub fn to_iran(instant: DateTime<Utc>) -> DateTime<FixedOffset> {
    instant.with_timezone(&Tehran).fixed_offset()
}

/// Today's date in Iran.
pub fn iran_today() -> Date {
    iran_now().date_naive()
}

// ------------------------------------------------------------------------------------------------ holidays

/// Official holidays that fall on Saturday–Wednesday, exported from DNTPersianUtils.Core 7.0.2's `IsHoliday`
/// table (Jalali 1395–1405; the library knows no other years).
static HOLIDAYS: LazyLock<HashSet<NaiveDate>> =
    LazyLock::new(|| include_str!("../data/holidays.txt").lines().filter_map(|line| NaiveDate::parse_from_str(line.trim(), "%Y-%m-%d").ok()).collect());

/// DNTPersianUtils `IsHoliday(convertToIranTimeZone: false)` for weekdays other than Thursday/Friday.
pub fn is_official_holiday(date: NaiveDate) -> bool {
    HOLIDAYS.contains(&date)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_fixes_arabic_letters_digits_and_padding() {
        assert_eq!(Some("فولاد مبارکه 1405".to_string()), normalize(Some("  فولاد مباركه ۱۴۰۵  ")));
        assert_eq!(None, normalize(Some("   ")));
        assert_eq!(None, normalize(None));
    }

    #[test]
    fn to_arabic_letters_produces_tsetmc_internal_spelling() {
        assert_eq!("كرمان", to_arabic_letters("کرمان"));
    }

    #[test]
    fn match_key_ignores_zwnj_and_spaces() {
        assert_eq!(match_key("سیمان آبیک"), match_key("سيمان\u{200C}آبيك"));
    }

    #[test]
    fn parse_jalali_returns_iran_local_time() {
        let cases = [("۱۴۰۵/۰۷/۰۷ ۱۸:۵۵:۳۴", (2026, 9, 29, 18, 55, 34)), ("1405/01/01", (2026, 3, 21, 0, 0, 0)), ("1405/07/18", (2026, 10, 10, 0, 0, 0))];
        let tehran = FixedOffset::east_opt(12600).unwrap();
        for (text, (y, mo, d, h, mi, s)) in cases {
            let expected = tehran.with_ymd_and_hms(y, mo, d, h, mi, s).unwrap();
            let parsed = parse_jalali(Some(text)).unwrap();
            assert_eq!(expected, parsed, "{text}");
            assert_eq!(12600, parsed.offset().local_minus_utc(), "{text}");
        }
    }

    #[test]
    fn parse_jalali_rejects_garbage() {
        assert_eq!(None, parse_jalali(Some("not a date")));
        assert_eq!(None, parse_jalali(Some("1405/13/01")));
    }

    #[test]
    fn jalali_round_trips() {
        let date = NaiveDate::from_ymd_opt(2026, 10, 17).unwrap();
        assert_eq!((1405, 7, 25), gregorian_to_jalali(date));
        assert_eq!(Some(date), jalali_to_gregorian(1405, 7, 25));
    }
}
