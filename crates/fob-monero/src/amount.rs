//! XMR amounts: atomic units (piconero, 10^-12 XMR) to and from text.

use crate::error::{MoneroError, Result};

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

/// `text` with its decimal comma, if it has one, made a point. A comma
/// followed by exactly three digits after a non-zero whole ("1,000") is
/// refused: it reads as a thousands separator as readily as a decimal comma,
/// and taking it as a decimal turns a thousand into one. A two-decimal
/// currency already refused it for having too many decimals, so this makes
/// XMR and the three-decimal currencies (KWD, BHD) say the same. "0,001"
/// stays a decimal, since no thousands start with zero.
pub(crate) fn decimal_point(text: &str) -> Result<String> {
    if let Some((whole, frac)) = text.split_once(',') {
        let thousands = frac.len() == 3 && frac.chars().all(|c| c.is_ascii_digit()) && whole.chars().any(|c| c != '0');
        if thousands {
            return Err(MoneroError::BadAmount);
        }
    }
    Ok(text.replace(',', "."))
}

/// Text someone typed ("0.5", ".25", "1,5") as atomic units.
pub fn parse(text: &str) -> Result<u64> {
    let text = decimal_point(text.trim())?;
    let (whole, frac) = text.split_once('.').unwrap_or((&text, ""));
    if whole.is_empty() && frac.is_empty()
        || !whole.chars().all(|c| c.is_ascii_digit())
        || !frac.chars().all(|c| c.is_ascii_digit())
        || frac.len() > DECIMALS
    {
        return Err(MoneroError::BadAmount);
    }
    let whole: u64 = if whole.is_empty() { 0 } else { whole.parse().map_err(|_| MoneroError::BadAmount)? };
    let frac: u64 = format!("{frac:0<12}").parse().map_err(|_| MoneroError::BadAmount)?;
    whole
        .checked_mul(ATOMIC_PER_XMR)
        .and_then(|w| w.checked_add(frac))
        .ok_or(MoneroError::BadAmount)
}

/// Digits with a comma every three: "1,234".
pub fn grouped(whole: u64) -> String {
    let digits = whole.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// A sum of amounts that stops at the most a `u64` holds instead of
/// wrapping round: they come from a node or server, which could make them up.
pub fn total(amounts: impl IntoIterator<Item = u64>) -> u64 {
    amounts.into_iter().fold(0, u64::saturating_add)
}

/// What an amount is typed in: XMR, or money converted at a price.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Unit {
    #[default]
    Xmr,
    Fiat,
}

impl Unit {
    /// The unit a field's chip switches to.
    pub fn other(self) -> Unit {
        match self {
            Unit::Xmr => Unit::Fiat,
            Unit::Fiat => Unit::Xmr,
        }
    }
}

/// Text someone typed as an amount, with what it's typed in, so the two
/// can't come apart or be swapped on the way down.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Typed<'a> {
    Xmr(&'a str),
    Fiat(&'a str),
}

impl<'a> Typed<'a> {
    pub fn new(text: &'a str, unit: Unit) -> Typed<'a> {
        match unit {
            Unit::Xmr => Typed::Xmr(text),
            Unit::Fiat => Typed::Fiat(text),
        }
    }

    /// The text as typed, untrimmed.
    pub fn text(self) -> &'a str {
        match self {
            Typed::Xmr(t) | Typed::Fiat(t) => t,
        }
    }

    pub fn unit(self) -> Unit {
        match self {
            Typed::Xmr(_) => Unit::Xmr,
            Typed::Fiat(_) => Unit::Fiat,
        }
    }

    /// The same amount trimmed, or none when nothing but space is typed.
    pub fn trimmed(self) -> Option<Typed<'a>> {
        let t = self.text().trim();
        (!t.is_empty()).then(|| Typed::new(t, self.unit()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_total_stops_at_the_most_a_u64_holds() {
        assert_eq!(total([1, 2, 3]), 6);
        assert_eq!(total([u64::MAX, 1, 2]), u64::MAX);
        assert_eq!(total([]), 0);
    }

    #[test]
    fn a_typed_amount_keeps_its_unit() {
        assert_eq!((Typed::new("20", Unit::Fiat).text(), Typed::new("20", Unit::Fiat).unit()), ("20", Unit::Fiat));
        assert_eq!(Typed::new(" 0.5 ", Unit::Xmr), Typed::Xmr(" 0.5 "));
        assert_eq!(Typed::Fiat(" 20 ").trimmed(), Some(Typed::Fiat("20")));
        assert_eq!(Typed::Xmr("  ").trimmed(), None);
        assert_eq!((Unit::Xmr.other(), Unit::Fiat.other(), Unit::default()), (Unit::Fiat, Unit::Xmr, Unit::Xmr));
    }

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
    fn a_comma_that_could_be_thousands_is_refused_not_read_as_a_decimal() {
        // "1,000" was 1 XMR; it could as well mean a thousand, so neither.
        for bad in ["1,000", "12,500", " 1,000 ", "1,000,000"] {
            assert!(parse(bad).is_err(), "{bad} parsed");
        }
        // The point stays unambiguous, and other decimal commas still work.
        assert_eq!(parse("1.000").unwrap(), ATOMIC_PER_XMR);
        assert_eq!(parse("0,001").unwrap(), ATOMIC_PER_XMR / 1000);
        assert_eq!(parse(",001").unwrap(), ATOMIC_PER_XMR / 1000);
        assert_eq!(parse("1,25").unwrap(), 5 * ATOMIC_PER_XMR / 4);
        assert_eq!(parse("1,0005").unwrap(), 1_000_500_000_000);
    }

    #[test]
    fn round_trips() {
        for a in [0, 1, 123_456_789, ATOMIC_PER_XMR, 18_400_000_000_000_000] {
            assert_eq!(parse(&format(a)).unwrap(), a);
        }
    }
}
