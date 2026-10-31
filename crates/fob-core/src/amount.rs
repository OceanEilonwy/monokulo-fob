//! XMR amounts: atomic units (piconero, 10^-12 XMR) to and from text.

use crate::error::{FobError, Result};

pub const ATOMIC_PER_XMR: u64 = 1_000_000_000_000;
/// The Monero sign in common use, before an amount typed in XMR.
pub const SIGN: &str = "ɱ";
const DECIMALS: usize = 12;

/// `amount` as XMR, every digit but no trailing zeros: "0.1421", "0.5",
/// "1", "0.000000000001".
pub fn format(amount: u64) -> String {
    let whole = amount / ATOMIC_PER_XMR;
    let frac = format!("{:0width$}", amount % ATOMIC_PER_XMR, width = DECIMALS);
    let frac = frac.trim_end_matches('0');
    if frac.is_empty() { whole.to_string() } else { format!("{whole}.{frac}") }
}

/// `amount` for a headline or a list: up to four decimals, cut not rounded,
/// so it never shows more than there is, and no trailing zeros ("0.5",
/// "2.4813", "1"). Amounts under 0.0001 show in full.
pub fn format_short(amount: u64) -> String {
    const TENTH_MILLI: u64 = ATOMIC_PER_XMR / 10_000;
    if amount != 0 && amount < TENTH_MILLI {
        return format(amount);
    }
    format(amount - amount % TENTH_MILLI)
}

/// Text someone typed ("0.5", ".25", "1,5") as atomic units.
pub fn parse(text: &str) -> Result<u64> {
    let text = text.trim().replace(',', ".");
    let (whole, frac) = text.split_once('.').unwrap_or((&text, ""));
    if whole.is_empty() && frac.is_empty()
        || !whole.chars().all(|c| c.is_ascii_digit())
        || !frac.chars().all(|c| c.is_ascii_digit())
        || frac.len() > DECIMALS
    {
        return Err(FobError::BadAmount);
    }
    let whole: u64 = if whole.is_empty() { 0 } else { whole.parse().map_err(|_| FobError::BadAmount)? };
    let frac: u64 = format!("{frac:0<12}").parse().map_err(|_| FobError::BadAmount)?;
    whole
        .checked_mul(ATOMIC_PER_XMR)
        .and_then(|w| w.checked_add(frac))
        .ok_or(FobError::BadAmount)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_like_the_designs() {
        assert_eq!(format(142_100_000_000), "0.1421");
        assert_eq!(format(2_481_300_000_000), "2.4813");
        assert_eq!(format(30_000_000), "0.00003");
        assert_eq!(format(ATOMIC_PER_XMR), "1");
        assert_eq!(format(0), "0");
        assert_eq!(format(1), "0.000000000001");
    }

    #[test]
    fn trailing_zeros_and_a_bare_dot_are_dropped() {
        assert_eq!(format(ATOMIC_PER_XMR / 2), "0.5");
        assert_eq!(format(653_000_000_000), "0.653");
        assert_eq!(format(10 * ATOMIC_PER_XMR), "10");
        assert_eq!(format_short(ATOMIC_PER_XMR / 2), "0.5");
        assert_eq!(format_short(653_000_000_000), "0.653");
        assert_eq!(format_short(ATOMIC_PER_XMR), "1");
        assert_eq!(format_short(1_000_099_999_999), "1");
        assert_eq!(format_short(2_481_300_000_000), "2.4813");
    }

    #[test]
    fn short_amounts_are_cut_to_four_decimals() {
        assert_eq!(format_short(19_878_240_000), "0.0198");
        assert_eq!(format_short(2_481_399_999_999), "2.4813");
        assert_eq!(format_short(30_000_000), "0.00003");
        assert_eq!(format_short(10_000_000), "0.00001");
        assert_eq!(format_short(0), "0");
    }

    #[test]
    fn parses_what_people_type() {
        assert_eq!(parse("0.5").unwrap(), ATOMIC_PER_XMR / 2);
        assert_eq!(parse(".25").unwrap(), ATOMIC_PER_XMR / 4);
        assert_eq!(parse("1,5").unwrap(), 3 * ATOMIC_PER_XMR / 2);
        assert_eq!(parse(" 2 ").unwrap(), 2 * ATOMIC_PER_XMR);
        assert_eq!(parse("0.000000000001").unwrap(), 1);
    }

    #[test]
    fn refuses_what_isnt_an_amount() {
        for bad in ["", ".", "abc", "1.2.3", "-1", "0.0000000000001", "99999999999"] {
            assert!(parse(bad).is_err(), "{bad} parsed");
        }
    }

    #[test]
    fn round_trips() {
        for a in [0, 1, 123_456_789, ATOMIC_PER_XMR, 18_400_000_000_000_000] {
            assert_eq!(parse(&format(a)).unwrap(), a);
        }
    }
}
