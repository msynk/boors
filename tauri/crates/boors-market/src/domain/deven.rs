//! TSETMC encodes dates as `dEven` integers (YYYYMMDD, Gregorian) and times as `hEven` integers (HHMMSS without
//! leading zeros, e.g. 91530 = 09:15:30). Storage keeps these; conversions happen at the edges.

use chrono::{Datelike, NaiveDate, NaiveTime, Timelike};

use crate::{Date, Error, Result, Time};

/// `YYYYMMDD` → date.
pub fn to_date(d_even: i32) -> Result<Date> {
    if !(10_000_101..=99_991_231).contains(&d_even) {
        return Err(Error::Format(format!("Expected a YYYYMMDD integer, got {d_even}.")));
    }
    NaiveDate::from_ymd_opt(d_even / 10000, (d_even / 100 % 100) as u32, (d_even % 100) as u32)
        .ok_or_else(|| Error::Format(format!("Expected a YYYYMMDD integer, got {d_even}.")))
}

/// `None` for 0 or negative values.
pub fn to_date_or_none(d_even: i32) -> Result<Option<Date>> {
    if d_even <= 0 { Ok(None) } else { to_date(d_even).map(Some) }
}

pub fn from_date(date: Date) -> i32 {
    date.year() * 10000 + date.month() as i32 * 100 + date.day() as i32
}

/// `hEven` → time of day; `None` for 0 (no trade / not set) and out-of-range values.
pub fn to_time(h_even: i32) -> Option<Time> {
    if h_even <= 0 {
        return None;
    }
    let (hours, minutes, seconds) = (h_even / 10000, h_even / 100 % 100, h_even % 100);
    if hours > 23 || minutes > 59 || seconds > 59 {
        return None;
    }
    NaiveTime::from_hms_opt(hours as u32, minutes as u32, seconds as u32)
}

/// Time of day → `hEven`.
pub fn from_time(time: Time) -> i32 {
    (time.hour() * 10000 + time.minute() * 100 + time.second()) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deven_converts_dates_and_times() {
        assert_eq!(NaiveDate::from_ymd_opt(2026, 10, 14).unwrap(), to_date(20261014).unwrap());
        assert_eq!(20261014, from_date(NaiveDate::from_ymd_opt(2026, 10, 14).unwrap()));
        assert_eq!(NaiveTime::from_hms_opt(9, 15, 30), to_time(91530));
        assert_eq!(None, to_time(0));
        assert_eq!(None, to_date_or_none(0).unwrap());
        assert!(to_date(1).is_err());
    }
}
