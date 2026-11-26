//! Fixed-point money: every value is a whole number of ten-thousandths.

use std::fmt;
use std::ops::Add;
use std::str::FromStr;

use serde::{Serialize, Serializer};
use thiserror::Error;

const DECIMAL_PLACES: usize = 4;
const UNITS_PER_WHOLE: i64 = 10_000;

/// A strictly positive sum of money, as carried by a deposit or withdrawal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Amount(i64);

#[derive(Debug, Error, PartialEq, Eq)]
pub enum AmountError {
    #[error("amount is not an unsigned decimal number")]
    Malformed,
    #[error("amount has more than four decimal places")]
    TooPrecise,
    #[error("amount is too large")]
    TooLarge,
    #[error("amount must be greater than zero")]
    NotPositive,
}

impl FromStr for Amount {
    type Err = AmountError;

    fn from_str(text: &str) -> Result<Self, AmountError> {
        let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));

        if whole.is_empty() || !is_all_digits(whole) || !is_all_digits(fraction) {
            return Err(AmountError::Malformed);
        }
        if fraction.len() > DECIMAL_PLACES {
            return Err(AmountError::TooPrecise);
        }

        let units = whole_units(whole)?
            .checked_add(fraction_units(fraction)?)
            .ok_or(AmountError::TooLarge)?;

        match units {
            0 => Err(AmountError::NotPositive),
            units => Ok(Amount(units)),
        }
    }
}

fn is_all_digits(text: &str) -> bool {
    text.bytes().all(|byte| byte.is_ascii_digit())
}

fn whole_units(digits: &str) -> Result<i64, AmountError> {
    digits
        .parse::<i64>()
        .ok()
        .and_then(|whole| whole.checked_mul(UNITS_PER_WHOLE))
        .ok_or(AmountError::TooLarge)
}

fn fraction_units(digits: &str) -> Result<i64, AmountError> {
    if digits.is_empty() {
        return Ok(0);
    }
    let missing_places = (DECIMAL_PLACES - digits.len()) as u32;
    digits
        .parse::<i64>()
        .map(|fraction| fraction * 10_i64.pow(missing_places))
        .map_err(|_| AmountError::Malformed)
}

/// A signed account balance. Unlike an [`Amount`] it may be negative: disputing
/// a deposit that was already spent leaves the client owing money.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Balance(i64);

#[derive(Debug, Error, PartialEq, Eq)]
#[error("balance would overflow")]
pub struct BalanceOverflow;

impl Balance {
    pub const ZERO: Balance = Balance(0);

    pub fn plus(self, amount: Amount) -> Result<Balance, BalanceOverflow> {
        self.0
            .checked_add(amount.0)
            .map(Balance)
            .ok_or(BalanceOverflow)
    }

    pub fn minus(self, amount: Amount) -> Result<Balance, BalanceOverflow> {
        self.0
            .checked_sub(amount.0)
            .map(Balance)
            .ok_or(BalanceOverflow)
    }

    pub fn covers(self, amount: Amount) -> bool {
        self.0 >= amount.0
    }
}

impl Add for Balance {
    type Output = Balance;

    fn add(self, other: Balance) -> Balance {
        Balance(self.0 + other.0)
    }
}

impl fmt::Display for Balance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sign = if self.0 < 0 { "-" } else { "" };
        let units = self.0.unsigned_abs();
        let per_whole = UNITS_PER_WHOLE.unsigned_abs();
        write!(
            f,
            "{sign}{}.{:0DECIMAL_PLACES$}",
            units / per_whole,
            units % per_whole
        )
    }
}

impl Serialize for Balance {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn amount(text: &str) -> Amount {
        text.parse().unwrap()
    }

    fn balance_of(text: &str) -> Balance {
        Balance::ZERO.plus(amount(text)).unwrap()
    }

    #[test]
    fn parses_every_supported_precision() {
        assert_eq!(amount("1"), Amount(10_000));
        assert_eq!(amount("1."), Amount(10_000));
        assert_eq!(amount("1.5"), Amount(15_000));
        assert_eq!(amount("1.05"), Amount(10_500));
        assert_eq!(amount("0.0001"), Amount(1));
        assert_eq!(amount("0012.3400"), Amount(123_400));
    }

    #[test]
    fn rejects_text_that_is_not_an_unsigned_decimal() {
        for text in [
            "", ".", ".5", "-1", "+1", "1e3", "1.2.3", "1,5", "abc", " 1",
        ] {
            assert_eq!(
                text.parse::<Amount>(),
                Err(AmountError::Malformed),
                "{text:?}"
            );
        }
    }

    #[test]
    fn rejects_more_than_four_decimal_places() {
        assert_eq!("1.00001".parse::<Amount>(), Err(AmountError::TooPrecise));
    }

    #[test]
    fn rejects_zero() {
        assert_eq!("0".parse::<Amount>(), Err(AmountError::NotPositive));
        assert_eq!("0.0000".parse::<Amount>(), Err(AmountError::NotPositive));
    }

    #[test]
    fn accepts_the_largest_representable_amount_and_nothing_above_it() {
        assert_eq!(amount("922337203685477.5807"), Amount(i64::MAX));
        assert_eq!(
            "922337203685477.5808".parse::<Amount>(),
            Err(AmountError::TooLarge)
        );
        assert_eq!(
            "99999999999999999999".parse::<Amount>(),
            Err(AmountError::TooLarge)
        );
    }

    #[test]
    fn displays_four_decimal_places() {
        assert_eq!(Balance::ZERO.to_string(), "0.0000");
        assert_eq!(balance_of("1.5").to_string(), "1.5000");
        assert_eq!(balance_of("0.0001").to_string(), "0.0001");
        assert_eq!(balance_of("1234.5678").to_string(), "1234.5678");
    }

    #[test]
    fn displays_negative_balances_with_a_single_leading_sign() {
        let owed = Balance::ZERO.minus(amount("0.5")).unwrap();
        assert_eq!(owed.to_string(), "-0.5000");
    }

    #[test]
    fn arithmetic_reports_overflow_instead_of_wrapping() {
        let largest = balance_of("922337203685477.5807");
        assert_eq!(largest.plus(amount("0.0001")), Err(BalanceOverflow));

        let smallest = Balance(i64::MIN);
        assert_eq!(smallest.minus(amount("0.0001")), Err(BalanceOverflow));
    }

    #[test]
    fn covers_amounts_up_to_and_including_the_balance() {
        let balance = balance_of("2");
        assert!(balance.covers(amount("2")));
        assert!(!balance.covers(amount("2.0001")));
    }
}
