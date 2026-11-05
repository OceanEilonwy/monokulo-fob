//! Money beside XMR: how each currency is written, and converting between it
//! and XMR at a price. No lookups: what a price is comes from elsewhere.

pub mod currencies;

use serde::{Deserialize, Serialize};

use crate::{amount::{ATOMIC_PER_XMR, grouped}, error::MoneroError, error::Result};

/// Prices are millionths of the currency per XMR.
pub const MICRO: u64 = 1_000_000;

/// The price recorded with a payment: what one XMR was worth then.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    pub currency: String,
    /// Millionths of the currency per XMR.
    pub price: u64,
    pub provider: String,
    /// When the price is from: when it was asked, or the start of its UTC
    /// day for a daily price.
    pub at: u64,
    /// A day's price from the history rather than the price at the moment.
    #[serde(default)]
    pub daily: bool,
}

/// A currency as Fob writes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Currency {
    /// ISO 4217: "USD".
    pub code: String,
    /// Written before the amount when there is one.
    pub symbol: Option<&'static str>,
    /// "US dollar"; empty for one Fob doesn't know.
    pub name: &'static str,
    /// "US dollars", "euros", as in "Amount in euros"; empty for one Fob
    /// doesn't know.
    pub plural: &'static str,
    /// Digits after the point in its smallest unit.
    pub decimals: u32,
    /// Split up's smallest note, in whole units ([`currencies::Info::note`]).
    pub note: u64,
}

/// How Fob writes `code`: as [`currencies`] has it, or, for one it doesn't
/// know (only ever in a record from elsewhere), by its code with two
/// decimals.
pub fn currency(code: &str) -> Currency {
    match currencies::find(code) {
        Some(c) => Currency { code: c.code.to_owned(), symbol: c.symbol, name: c.name, plural: c.plural, decimals: c.decimals, note: c.note },
        None => Currency { code: code.to_owned(), symbol: None, name: "", plural: "", decimals: 2, note: 5 },
    }
}

impl Currency {
    /// Settings' row: its name, or its code when Fob doesn't know one ("US
    /// dollar", "XYZ").
    pub fn text(&self) -> String {
        if self.name.is_empty() { self.code.clone() } else { self.name.to_owned() }
    }

    /// Short, as Settings' row has it: "AUD · A$", or the code alone
    /// without a symbol or with one that is the code ("CHF").
    pub fn short(&self) -> String {
        match self.symbol {
            Some(symbol) if symbol != self.code => format!("{} · {symbol}", self.code),
            _ => self.code.clone(),
        }
    }

    /// Its sign in a list: the symbol, or the code.
    pub fn sign(&self) -> String {
        self.symbol.map(str::to_owned).unwrap_or_else(|| self.code.clone())
    }
}

fn pow10(n: u32) -> u128 {
    10u128.pow(n)
}

/// `atomic` XMR in the currency's smallest unit (cents) at `price`, rounded
/// half up.
pub fn to_minor(atomic: u64, price: u64, currency: &Currency) -> u64 {
    let num = atomic as u128 * price as u128 * pow10(currency.decimals);
    let den = ATOMIC_PER_XMR as u128 * MICRO as u128;
    u64::try_from((num + den / 2) / den).unwrap_or(u64::MAX)
}

/// The XMR, in atomic units, worth `minor` at `price`: as few decimals as
/// still come to the same cents (at least 4, the way Fob shows amounts), so
/// a payment code asks for "0.1204" rather than "0.120409391932". `None`
/// at a price of 0.
pub fn to_atomic(minor: u64, price: u64, currency: &Currency) -> Option<u64> {
    if price == 0 {
        return None;
    }
    let num = minor as u128 * ATOMIC_PER_XMR as u128 * MICRO as u128;
    let den = price as u128 * pow10(currency.decimals);
    let exact = u64::try_from((num + den / 2) / den).ok()?;
    for decimals in 4..12 {
        let step = 10u64.pow(12 - decimals);
        let rounded = (exact + step / 2) / step * step;
        if rounded > 0 && to_minor(rounded, price, currency) == minor {
            return Some(rounded);
        }
    }
    Some(exact)
}

/// `minor` units as digits: "1,234.56", "12.50"; `group` puts commas in.
fn digits(minor: u64, currency: &Currency, group: bool) -> String {
    let unit = 10u64.pow(currency.decimals);
    let whole = if group { grouped(minor / unit) } else { (minor / unit).to_string() };
    if currency.decimals == 0 { whole } else { format!("{whole}.{:0width$}", minor % unit, width = currency.decimals as usize) }
}

/// An amount of money, the same whatever the phone's language: the symbol
/// first when the currency has one ("$412.06", "€12.50", "$1,234.00"),
/// otherwise the code after ("412.06 CHF"); a point before the cents,
/// commas between thousands, always all the decimals.
pub fn format(minor: u64, currency: &Currency) -> String {
    let digits = digits(minor, currency, true);
    match currency.symbol {
        Some(symbol) => format!("{symbol}{digits}"),
        None => format!("{digits} {}", currency.code),
    }
}

/// As [`format`], for typing into: no commas, no symbol ("1234.50").
pub fn plain(minor: u64, currency: &Currency) -> String {
    digits(minor, currency, false)
}

/// What `atomic` XMR is worth at `price`: "$83.03".
pub fn value(atomic: u64, price: u64, currency: &Currency) -> String {
    format(to_minor(atomic, price, currency), currency)
}

/// "1 XMR = $166.06".
pub fn rate(price: u64, currency: &Currency) -> String {
    format!("1 XMR = {}", value(ATOMIC_PER_XMR, price, currency))
}

/// Money someone typed ("12.5", "12,50", "$12.50", "12.50 USD") in the
/// currency's smallest unit. A comma is a decimal point, as XMR amounts
/// take it; no thousands separators; no more decimals than the currency
/// has.
pub fn parse(text: &str, currency: &Currency) -> Result<u64> {
    let mut text = text.trim();
    if let Some(symbol) = currency.symbol {
        text = text.strip_prefix(symbol).unwrap_or(text).trim_start();
    }
    let cut = text.len().saturating_sub(currency.code.len());
    if text.is_char_boundary(cut) && text[cut..].eq_ignore_ascii_case(&currency.code) {
        text = text[..cut].trim_end();
    }
    let text = text.replace(',', ".");
    let (whole, frac) = text.split_once('.').unwrap_or((&text, ""));
    if whole.is_empty() && frac.is_empty()
        || !whole.chars().all(|c| c.is_ascii_digit())
        || !frac.chars().all(|c| c.is_ascii_digit())
        || frac.len() > currency.decimals as usize
    {
        return Err(MoneroError::BadAmount);
    }
    let unit = 10u64.pow(currency.decimals);
    let whole: u64 = if whole.is_empty() { 0 } else { whole.parse().map_err(|_| MoneroError::BadAmount)? };
    let frac: u64 = if frac.is_empty() { 0 } else { format!("{frac:0<width$}", width = currency.decimals as usize).parse().map_err(|_| MoneroError::BadAmount)? };
    whole.checked_mul(unit).and_then(|w| w.checked_add(frac)).ok_or(MoneroError::BadAmount)
}

/// How old a price is, once it's missed a refresh. Shown prices are under a
/// day old, so never days.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PriceAge {
    Minutes(u64),
    Hours(u64),
}

/// How a payment's value moved since it moved, for a client to say.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    Same,
    /// By how much, and in tenths of a percent as "4.6"; `None` from nothing.
    Up { by: String, percent: Option<String> },
    Down { by: String, percent: Option<String> },
}

/// The change between a payment's value then and now.
pub fn change(then: u64, now: u64, currency: &Currency) -> Change {
    if then == now {
        return Change::Same;
    }
    let diff = now.abs_diff(then);
    let percent = (then != 0).then(|| {
        // Tenths of a percent, rounded half up.
        let tenths = (diff as u128 * 1000 + then as u128 / 2) / then as u128;
        format!("{}.{}", tenths / 10, tenths % 10)
    });
    let by = format(diff, currency);
    if now > then { Change::Up { by, percent } } else { Change::Down { by, percent } }
}

#[cfg(test)]
mod tests {
    use super::*;

    const XMR: u64 = ATOMIC_PER_XMR;

    fn usd() -> Currency {
        currency("USD")
    }

    fn eur() -> Currency {
        currency("EUR")
    }

    fn gbp() -> Currency {
        currency("GBP")
    }

    #[test]
    fn money_is_written_one_way_whatever_the_language() {
        assert_eq!(format(41_206, &usd()), "$412.06");
        assert_eq!(format(1_250, &eur()), "€12.50");
        assert_eq!(format(41_206, &currency("CHF")), "412.06 CHF");
        assert_eq!(format(41_206, &gbp()), "£412.06");
        assert_eq!(format(123_456_700, &usd()), "$1,234,567.00");
        assert_eq!(format(5, &usd()), "$0.05");
        assert_eq!(format(0, &usd()), "$0.00");
        assert_eq!(plain(123_450, &usd()), "1234.50");
        assert_eq!(currency("JPY").symbol, Some("¥"));
        assert_eq!(eur().symbol, Some("€"));
        assert_eq!(currency("XYZ").name, "");
        assert_eq!(format(41_206, &currency("XYZ")), "412.06 XYZ");
    }

    #[test]
    fn values_round_half_up_to_the_cent() {
        // 0.5 XMR at $166.06 is $83.03.
        assert_eq!(value(XMR / 2, 166_060_000, &usd()), "$83.03");
        // 0.1 XMR at $0.125 is $0.0125: half a cent rounds up.
        assert_eq!(to_minor(XMR / 10, 125_000, &usd()), 1);
        assert_eq!(to_minor(XMR / 10, 124_999, &usd()), 1);
        assert_eq!(to_minor(XMR / 10, 49_999, &usd()), 0);
        assert_eq!(to_minor(XMR / 10, 50_000, &usd()), 1);
        assert_eq!(rate(166_060_000, &usd()), "1 XMR = $166.06");
        // A whole wallet at a silly price stays in range.
        assert_eq!(to_minor(18_400_000 * XMR, 1_000_000 * MICRO, &usd()), 1_840_000_000_000_000);
    }

    #[test]
    fn fiat_converts_to_as_few_xmr_decimals_as_keep_the_cents() {
        // $20 at $166.10 is 0.120409… XMR: 0.1204 is still $20.00.
        let atomic = to_atomic(2_000, 166_100_000, &usd()).unwrap();
        assert_eq!(crate::amount::format(atomic), "0.1204");
        // $20 at $600: 0.0333 is $19.98, so a fifth decimal.
        let atomic = to_atomic(2_000, 600_000_000, &usd()).unwrap();
        assert_eq!(crate::amount::format(atomic), "0.03333");
        assert_eq!(to_minor(atomic, 600_000_000, &usd()), 2_000);
        assert_eq!(to_atomic(1, 0, &usd()), None);
        for minor in [1, 99, 2_000, 1_234_567] {
            for price in [1_000_000, 166_100_000, 593_910_000, 12_345_678_901] {
                let atomic = to_atomic(minor, price, &usd()).unwrap();
                assert_eq!(to_minor(atomic, price, &usd()), minor, "{minor} at {price}");
            }
        }
    }

    #[test]
    fn typed_money_is_read() {
        assert_eq!(parse("12.5", &usd()).unwrap(), 1_250);
        assert_eq!(parse("12,50", &usd()).unwrap(), 1_250);
        assert_eq!(parse("$12.50", &usd()).unwrap(), 1_250);
        assert_eq!(parse(" 12.50 usd ", &usd()).unwrap(), 1_250);
        assert_eq!(parse("€3", &eur()).unwrap(), 300);
        assert_eq!(parse(".5", &usd()).unwrap(), 50);
        for bad in ["", ".", "1.234", "1,234.56", "-1", "abc", "€3", "99999999999999999999"] {
            assert!(parse(bad, &usd()).is_err(), "{bad} parsed");
        }
    }

    #[test]
    fn changes_since_then_say_by_how_much() {
        assert_eq!(change(7_900, 8_263, &usd()), Change::Up { by: "$3.63".into(), percent: Some("4.6".into()) });
        assert_eq!(change(15_000, 14_880, &usd()), Change::Down { by: "$1.20".into(), percent: Some("0.8".into()) });
        assert_eq!(change(100, 100, &usd()), Change::Same);
        assert_eq!(change(0, 100, &usd()), Change::Up { by: "$1.00".into(), percent: None });
    }
}
