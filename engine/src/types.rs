//! Small shared value types: dates, timestamps, and the engine clock.

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::str::FromStr;

/// A calendar date, `YYYY-MM-DD`. Stored as its ISO string, whose
/// lexicographic order equals chronological order, so `Ord` is derived.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date(String);

impl Date {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn is_leap_year(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

impl FromStr for Date {
    type Err = String;

    fn from_str(s: &str) -> Result<Date, String> {
        let bytes = s.as_bytes();
        if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
            return Err(format!("date must be YYYY-MM-DD: {s:?}"));
        }
        let all_digits = |r: std::ops::Range<usize>| bytes[r].iter().all(u8::is_ascii_digit);
        if !all_digits(0..4) || !all_digits(5..7) || !all_digits(8..10) {
            return Err(format!("date must be YYYY-MM-DD: {s:?}"));
        }
        let year: i32 = s[0..4].parse().unwrap();
        let month: u32 = s[5..7].parse().unwrap();
        let day: u32 = s[8..10].parse().unwrap();
        if !(1..=12).contains(&month) {
            return Err(format!("month out of range: {s:?}"));
        }
        let days_in_month = match month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 if is_leap_year(year) => 29,
            2 => 28,
            _ => unreachable!(),
        };
        if day < 1 || day > days_in_month {
            return Err(format!("day out of range: {s:?}"));
        }
        Ok(Date(s.to_string()))
    }
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for Date {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Date {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Date, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(D::Error::custom)
    }
}

/// Milliseconds since the Unix epoch. System-set on every event.
pub type TimestampMs = i64;

/// Source of "now" for system-set timestamps. Injectable so tests are
/// deterministic; replay never consults the clock (it uses recorded times).
pub trait Clock: Send + Sync {
    fn now_ms(&self) -> TimestampMs;
}

/// Wall-clock time.
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> TimestampMs {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
    }
}

/// Fixed, manually advanced time for tests.
pub struct FixedClock(pub std::sync::atomic::AtomicI64);

impl FixedClock {
    pub fn new(start_ms: TimestampMs) -> FixedClock {
        FixedClock(std::sync::atomic::AtomicI64::new(start_ms))
    }
}

impl Clock for FixedClock {
    fn now_ms(&self) -> TimestampMs {
        // Each reading advances by 1ms so event times are strictly increasing.
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_dates() {
        for s in ["2026-01-01", "2024-02-29", "1999-12-31"] {
            assert!(s.parse::<Date>().is_ok(), "should accept {s:?}");
        }
    }

    #[test]
    fn invalid_dates() {
        for s in [
            "2026-13-01",
            "2026-00-10",
            "2026-02-30",
            "2025-02-29",
            "2026-1-1",
            "20260101",
            "2026-01-32",
            "abcd-ef-gh",
        ] {
            assert!(s.parse::<Date>().is_err(), "should reject {s:?}");
        }
    }

    #[test]
    fn ordering_is_chronological() {
        let a: Date = "2026-01-31".parse().unwrap();
        let b: Date = "2026-02-01".parse().unwrap();
        assert!(a < b);
    }
}
