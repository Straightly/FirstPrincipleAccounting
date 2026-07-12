//! Exact fixed-point amounts: `Decimal(18,8)` per Impl Spec §2.5.
//!
//! Stored as an `i128` count of 1e-8 units. All arithmetic is checked and
//! exact — no floats anywhere in the engine (money is never approximated).
//! Serialized as a canonical decimal string, e.g. `"12.34000000"`.

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::str::FromStr;

/// 1e8 — raw units per whole unit (8 decimal places).
pub const SCALE: i128 = 100_000_000;

/// Decimal(18,8): at most 18 significant digits total, so |raw| < 1e18.
pub const MAX_RAW: i128 = 1_000_000_000_000_000_000; // 1e18, exclusive bound

/// An exact fixed-point amount with 8 decimal places.
///
/// May be negative (nets and residuals need signs); journal-line amounts are
/// separately validated to be `>= 0` by the engine (Impl Spec §2.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Amount(i128);

impl Amount {
    pub const ZERO: Amount = Amount(0);

    /// Construct from raw 1e-8 units, enforcing the Decimal(18,8) bound.
    pub fn from_raw(raw: i128) -> Result<Amount, String> {
        if raw.abs() >= MAX_RAW {
            return Err(format!("amount out of Decimal(18,8) range: raw {raw}"));
        }
        Ok(Amount(raw))
    }

    /// Raw value in 1e-8 units.
    pub fn raw(&self) -> i128 {
        self.0
    }

    pub fn is_zero(&self) -> bool {
        self.0 == 0
    }

    pub fn is_negative(&self) -> bool {
        self.0 < 0
    }

    pub fn checked_add(self, other: Amount) -> Result<Amount, String> {
        let raw = self
            .0
            .checked_add(other.0)
            .ok_or_else(|| "amount overflow in addition".to_string())?;
        Amount::from_raw(raw)
    }

    pub fn checked_sub(self, other: Amount) -> Result<Amount, String> {
        let raw = self
            .0
            .checked_sub(other.0)
            .ok_or_else(|| "amount overflow in subtraction".to_string())?;
        Amount::from_raw(raw)
    }

    pub fn checked_neg(self) -> Result<Amount, String> {
        Amount::from_raw(-self.0)
    }
}

impl fmt::Display for Amount {
    /// Canonical form: optional sign, integer part, '.', exactly 8 digits.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sign = if self.0 < 0 { "-" } else { "" };
        let abs = self.0.unsigned_abs();
        let int = abs / (SCALE as u128);
        let frac = abs % (SCALE as u128);
        write!(f, "{sign}{int}.{frac:08}")
    }
}

impl FromStr for Amount {
    type Err = String;

    /// Parse a decimal string with at most 8 fractional digits.
    fn from_str(s: &str) -> Result<Amount, String> {
        let s = s.trim();
        let (negative, body) = match s.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, s),
        };
        let body = body.strip_prefix('+').unwrap_or(body);
        if body.is_empty() {
            return Err("empty amount".to_string());
        }
        let (int_part, frac_part) = match body.split_once('.') {
            Some((i, f)) => (i, f),
            None => (body, ""),
        };
        if int_part.is_empty() && frac_part.is_empty() {
            return Err(format!("malformed amount: {s:?}"));
        }
        if frac_part.len() > 8 {
            return Err(format!("more than 8 decimal places: {s:?}"));
        }
        if !int_part.chars().all(|c| c.is_ascii_digit())
            || !frac_part.chars().all(|c| c.is_ascii_digit())
        {
            return Err(format!("malformed amount: {s:?}"));
        }
        let int_val: i128 = if int_part.is_empty() {
            0
        } else {
            int_part
                .parse()
                .map_err(|_| format!("integer part out of range: {s:?}"))?
        };
        let mut frac_val: i128 = if frac_part.is_empty() {
            0
        } else {
            frac_part
                .parse()
                .map_err(|_| format!("fraction out of range: {s:?}"))?
        };
        for _ in frac_part.len()..8 {
            frac_val *= 10;
        }
        let raw = int_val
            .checked_mul(SCALE)
            .and_then(|v| v.checked_add(frac_val))
            .ok_or_else(|| format!("amount out of range: {s:?}"))?;
        let raw = if negative { -raw } else { raw };
        Amount::from_raw(raw)
    }
}

impl Serialize for Amount {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Amount {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Amount, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(D::Error::custom)
    }
}

/// Greatest common divisor for i128 (both operands taken absolute).
pub(crate) fn gcd(a: i128, b: i128) -> i128 {
    let (mut a, mut b) = (a.abs(), b.abs());
    while b != 0 {
        let t = a % b;
        a = b;
        b = t;
    }
    a
}

/// An exact rational number used for cross-unit balance checks (Impl Spec
/// §2.6). Kept reduced; all operations are checked and error on overflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Rational {
    num: i128,
    den: i128, // always > 0
}

impl Rational {
    pub const ZERO: Rational = Rational { num: 0, den: 1 };

    pub fn new(num: i128, den: i128) -> Result<Rational, String> {
        if den == 0 {
            return Err("zero denominator".to_string());
        }
        let (num, den) = if den < 0 { (-num, -den) } else { (num, den) };
        let g = gcd(num, den).max(1);
        Ok(Rational {
            num: num / g,
            den: den / g,
        })
    }

    pub fn is_zero(&self) -> bool {
        self.num == 0
    }

    /// self + (amount_raw × num / den), exactly.
    pub fn add_scaled(&self, amount_raw: i128, num: i128, den: i128) -> Result<Rational, String> {
        let term_num = amount_raw
            .checked_mul(num)
            .ok_or_else(|| "overflow in cross-unit valuation".to_string())?;
        let term = Rational::new(term_num, den)?;
        // a/b + c/d = (a·d + c·b) / (b·d)
        let n1 = self
            .num
            .checked_mul(term.den)
            .ok_or_else(|| "overflow in cross-unit valuation".to_string())?;
        let n2 = term
            .num
            .checked_mul(self.den)
            .ok_or_else(|| "overflow in cross-unit valuation".to_string())?;
        let num = n1
            .checked_add(n2)
            .ok_or_else(|| "overflow in cross-unit valuation".to_string())?;
        let den = self
            .den
            .checked_mul(term.den)
            .ok_or_else(|| "overflow in cross-unit valuation".to_string())?;
        Rational::new(num, den)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_display_round_trip() {
        for s in ["0.00000000", "1.50000000", "-2.25000000", "12345.00000001"] {
            let a: Amount = s.parse().unwrap();
            assert_eq!(a.to_string(), s);
        }
    }

    #[test]
    fn parse_flexible_forms() {
        assert_eq!("1.5".parse::<Amount>().unwrap().raw(), 150_000_000);
        assert_eq!("1".parse::<Amount>().unwrap().raw(), SCALE);
        assert_eq!(".5".parse::<Amount>().unwrap().raw(), 50_000_000);
        assert_eq!("-0.00000001".parse::<Amount>().unwrap().raw(), -1);
    }

    #[test]
    fn parse_rejects_garbage() {
        for s in ["", "-", ".", "1.2.3", "1e5", "1.123456789", "abc", "1 2"] {
            assert!(s.parse::<Amount>().is_err(), "should reject {s:?}");
        }
    }

    #[test]
    fn range_enforced() {
        assert!(Amount::from_raw(MAX_RAW).is_err());
        assert!(Amount::from_raw(-MAX_RAW).is_err());
        assert!(Amount::from_raw(MAX_RAW - 1).is_ok());
    }

    #[test]
    fn rational_balance() {
        // 3 units @ rate 2.0 vs 6 units: 3·2 − 6 = 0
        let r = Rational::ZERO
            .add_scaled(3 * SCALE, 2 * SCALE, SCALE)
            .unwrap()
            .add_scaled(-6 * SCALE, 1, 1)
            .unwrap();
        assert!(r.is_zero());
    }
}
