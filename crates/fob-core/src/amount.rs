//! XMR amounts: atomic units (piconero, 10^-12 XMR) to and from text.

use crate::error::{FobError, Result};

pub const ATOMIC_PER_XMR: u64 = 1_000_000_000_000;
const DECIMALS: usize = 12;

/// `amount` as XMR with at least `min_decimals` places and no trailing zeros
/// past them: `format(142_100_000_000, 4)` is "0.1421".
pub fn format(amount: u64, min_decimals: usize) -> String {
    let whole = amount / ATOMIC_PER_XMR;
    let frac = format!("{:012}", amount % ATOMIC_PER_XMR);
    let keep = frac.trim_end_matches('0').len().max(min_decimals.min(DECIMALS));
    if keep == 0 {
        whole.to_string()
    } else {
        format!("{whole}.{}", &frac[..keep])
    }
}

/// `amount` for a headline or a list: four decimals, cut not rounded, so it
/// never shows more than there is. Amounts under 0.0001 show in full.
pub fn format_short(amount: u64) -> String {
    const TENTH_MILLI: u64 = ATOMIC_PER_XMR / 10_000;
    if amount != 0 && amount < TENTH_MILLI {
        return format(amount, 4);
    }
    format(amount - amount % TENTH_MILLI, 4)
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
        assert_eq!(format(142_100_000_000, 4), "0.1421");
        assert_eq!(format(2_481_300_000_000, 4), "2.4813");
        assert_eq!(format(30_000_000, 4), "0.00003");
        assert_eq!(format(ATOMIC_PER_XMR, 0), "1");
        assert_eq!(format(0, 4), "0.0000");
        assert_eq!(format(1, 0), "0.000000000001");
    }

    #[test]
    fn short_amounts_are_cut_to_four_decimals() {
        assert_eq!(format_short(19_878_240_000), "0.0198");
        assert_eq!(format_short(2_481_399_999_999), "2.4813");
        assert_eq!(format_short(30_000_000), "0.00003");
        assert_eq!(format_short(0), "0.0000");
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
            assert_eq!(parse(&format(a, 0)).unwrap(), a);
        }
    }
}
