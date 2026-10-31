//! Prices: what XMR is worth in the money people think in, now and on the
//! day a payment moved.
//!
//! Two choices, made independently in Settings and kept in the wallet file
//! ([`Settings`]):
//!
//! - the **provider** ([`Provider`]): whose prices, from which API. Kraken's
//!   public REST API today ([`kraken`]).
//! - the **transport** ([`Transport`]): how the requests travel. Straight
//!   over the internet ([`Transport::Web`]); Tor ([`Transport::Tor`]) has its
//!   place but isn't built yet, and says so.
//!
//! A provider is anything implementing [`Source`]; [`Provider`] picks one
//! by name for the settings. Adding one is a variant there and its own
//! module beside [`kraken`].
//!
//! Privacy:
//! - No requests at all while prices are off.
//! - A request names the pair and nothing else: never a wallet's address,
//!   amount or payment, and only a plain `User-Agent: Fob`.
//! - Never a price for a payment's own moment, which would tell the
//!   provider when someone paid: history comes as the provider's whole
//!   daily window in one request, at most once a day, and is looked up here.
//! - The current price is asked at most every 5 minutes while Fob is on
//!   screen, and on pull to refresh (at most once a minute).
//!
//! Prices are whole numbers: millionths of the currency for one XMR
//! (`price`), never floats. Values are rounded half up to the currency's
//! smallest unit (cents).

pub mod kraken;

use std::{future::Future, ops::Range, sync::LazyLock, time::Duration};

use serde::{Deserialize, Serialize};

use crate::{
    amount::{self, ATOMIC_PER_XMR},
    error::{FobError, Result},
};

/// Prices are millionths of the currency per XMR.
pub const MICRO: u64 = 1_000_000;
/// How often the current price is asked while Fob is on screen.
pub const REFRESH: u64 = 5 * 60;
/// The soonest pull to refresh asks again.
pub const PULL_REFRESH: u64 = 60;
/// A current price older than this isn't shown, nor recorded with a payment.
pub const FRESH: u64 = 60 * 60;
/// How often the daily history is asked for.
pub const HISTORY_EVERY: u64 = 24 * 60 * 60;
/// Days of history kept per currency, and how many currencies' histories.
const HISTORY_DAYS: usize = 1_000;
const HISTORIES: usize = 3;
/// The most a price answer may be: Kraken's 720 days are about 60 kB.
const MAX_RESPONSE: usize = 1 << 20;
const DAY: u64 = 86_400;

pub const USER_AGENT: &str = "Fob";
const TOR_NOT_YET: &str = "prices over Tor aren't available yet";

/// Whose prices.
///
/// Extension point: a `Monokulo` provider, asking a Monokulo app on the
/// phone through its app integration for the price it uses at the till,
/// would be a variant here with its own module implementing [`Source`]. It
/// isn't built.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    #[default]
    Kraken,
}

/// How price requests travel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Transport {
    /// Straight over the internet: HTTPS with rustls, as Fob talks to nodes.
    #[default]
    Web,
    /// Over Tor. Not built yet: every request fails saying so. An arti-based
    /// transport goes here.
    Tor,
}

impl Transport {
    pub fn available(self) -> bool {
        self == Transport::Web
    }

    /// "over the internet", "over Tor".
    pub fn text(self) -> &'static str {
        match self {
            Transport::Web => "over the internet",
            Transport::Tor => "over Tor",
        }
    }

    /// The body at `url`, an `https://` address (never an onion one: Fob
    /// would resolve it through the phone's DNS).
    pub async fn get(self, url: &str) -> Result<Vec<u8>> {
        match self {
            Transport::Tor => Err(FobError::Price(TOR_NOT_YET.into())),
            Transport::Web => web(url).await,
        }
    }
}

static CLIENT: LazyLock<std::result::Result<reqwest::Client, String>> = LazyLock::new(|| {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())
});

async fn web(url: &str) -> Result<Vec<u8>> {
    let err = |e: String| FobError::Price(e);
    let parsed = reqwest::Url::parse(url).map_err(|_| err(format!("{url} isn't an address")))?;
    if parsed.scheme() != "https" {
        return Err(err("prices are only asked over https".into()));
    }
    // An onion price source would be looked up through the phone's DNS.
    if crate::node::is_onion(parsed.host_str().unwrap_or("")) {
        return Err(err("Fob can't reach .onion price sources yet".into()));
    }
    let client = CLIENT.as_ref().map_err(|e| err(e.clone()))?;
    let mut response = client.get(parsed).send().await.map_err(|e| err(format!("couldn't reach the price source: {}", e.without_url())))?;
    if !response.status().is_success() {
        return Err(err(format!("the price source answered {}", response.status())));
    }
    let mut body = vec![];
    while let Some(chunk) = response.chunk().await.map_err(|e| err(e.without_url().to_string()))? {
        body.extend_from_slice(&chunk);
        if body.len() > MAX_RESPONSE {
            return Err(err("the price source's answer was too large".into()));
        }
    }
    Ok(body)
}

/// A provider of prices.
pub trait Source: Send + Sync {
    /// Its name, as recorded with prices and shown: "Kraken".
    fn name(&self) -> &'static str;
    /// The currencies it prices XMR in, by ISO 4217 code.
    fn currencies(&self) -> &'static [&'static str];
    /// XMR's price now, in millionths of `currency`.
    fn current(&self, currency: &str, transport: Transport) -> impl Future<Output = Result<u64>> + Send;
    /// XMR's daily prices for the days (since the epoch, UTC) in `days`, as
    /// far as the provider has them. Callers pass whole windows (the most
    /// the provider keeps), never a range around one payment.
    fn daily(&self, currency: &str, days: Range<i64>, transport: Transport) -> impl Future<Output = Result<Vec<(i64, u64)>>> + Send;
    /// How many days back [`daily`](Self::daily) reaches.
    fn history_days(&self) -> i64;
}

impl Source for Provider {
    fn name(&self) -> &'static str {
        match self {
            Provider::Kraken => kraken::Kraken.name(),
        }
    }

    fn currencies(&self) -> &'static [&'static str] {
        match self {
            Provider::Kraken => kraken::Kraken.currencies(),
        }
    }

    async fn current(&self, currency: &str, transport: Transport) -> Result<u64> {
        match self {
            Provider::Kraken => kraken::Kraken.current(currency, transport).await,
        }
    }

    async fn daily(&self, currency: &str, days: Range<i64>, transport: Transport) -> Result<Vec<(i64, u64)>> {
        match self {
            Provider::Kraken => kraken::Kraken.daily(currency, days, transport).await,
        }
    }

    fn history_days(&self) -> i64 {
        match self {
            Provider::Kraken => kraken::Kraken.history_days(),
        }
    }
}

/// How prices are shown, kept in the wallet file. A file from before
/// prices opens with these defaults.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// "Show prices". Off: no request is ever made.
    pub on: bool,
    /// ISO 4217 code, one the provider lists.
    pub currency: String,
    pub provider: Provider,
    pub transport: Transport,
    /// "Show amounts in" the currency first: Home's balance leads with it.
    pub fiat_first: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { on: true, currency: "USD".into(), provider: Provider::Kraken, transport: Transport::Web, fiat_first: false }
    }
}

impl Settings {
    /// "Kraken · over the internet".
    pub fn source_text(&self) -> String {
        format!("{} · {}", self.provider.name(), self.transport.text())
    }
}

/// The last current price asked for.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Current {
    pub currency: String,
    pub provider: String,
    pub price: u64,
    /// When it was asked for, seconds since the epoch.
    pub at: u64,
}

/// One currency's daily prices from one provider.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct History {
    pub currency: String,
    pub provider: String,
    /// When it was asked for.
    pub fetched: u64,
    /// (day since the epoch, UTC; price), oldest first.
    pub days: Vec<(i64, u64)>,
}

/// Prices kept in the wallet file between runs.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Cache {
    pub current: Option<Current>,
    pub history: Vec<History>,
}

impl Cache {
    /// The current price for `settings`, if asked within the last hour.
    pub fn fresh(&self, settings: &Settings, now: u64) -> Option<&Current> {
        self.current
            .as_ref()
            .filter(|c| c.currency == settings.currency && c.provider == settings.provider.name() && now.saturating_sub(c.at) < FRESH)
    }

    fn history(&self, settings: &Settings) -> Option<&History> {
        self.history.iter().find(|h| h.currency == settings.currency && h.provider == settings.provider.name())
    }

    /// Keeps `days` as the history for `settings`, the newest `HISTORY_DAYS`
    /// of them, and at most `HISTORIES` currencies (the oldest asked go).
    fn keep_history(&mut self, settings: &Settings, mut days: Vec<(i64, u64)>, now: u64) {
        days.sort_by_key(|d| d.0);
        days.dedup_by_key(|d| d.0);
        if days.len() > HISTORY_DAYS {
            days.drain(..days.len() - HISTORY_DAYS);
        }
        self.history.retain(|h| !(h.currency == settings.currency && h.provider == settings.provider.name()));
        self.history.push(History { currency: settings.currency.clone(), provider: settings.provider.name().into(), fetched: now, days });
        self.history.sort_by_key(|h| std::cmp::Reverse(h.fetched));
        self.history.truncate(HISTORIES);
    }

    /// What XMR was worth at `time` for `settings`: the current price if it
    /// was asked within an hour of then, else that UTC day's price.
    pub fn value_at(&self, settings: &Settings, time: u64) -> Option<Record> {
        if let Some(c) = self.current.as_ref().filter(|c| c.currency == settings.currency && c.provider == settings.provider.name() && c.at.abs_diff(time) < FRESH) {
            return Some(Record { currency: c.currency.clone(), price: c.price, provider: c.provider.clone(), at: c.at, daily: false });
        }
        let history = self.history(settings)?;
        let day = (time / DAY) as i64;
        let i = history.days.binary_search_by_key(&day, |d| d.0).ok()?;
        Some(Record { currency: settings.currency.clone(), price: history.days[i].1, provider: history.provider.clone(), at: day as u64 * DAY, daily: true })
    }
}

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
    /// "US dollars", "euros", as in "Amount in euros"; the code for one Fob
    /// doesn't know.
    pub plural: &'static str,
    /// Digits after the point in its smallest unit.
    pub decimals: u32,
}

/// The currencies Fob knows how to write: (code, symbol, name, plural).
/// Only the dollar and the euro have their symbol: either is unmistakable,
/// while "£" or "kr" would want more care than a fallback to the code.
const KNOWN: &[(&str, &str, &str, &str)] = &[("USD", "$", "US dollar", "US dollars"), ("EUR", "€", "Euro", "euros")];

/// How Fob writes `code`. One it doesn't know is written by its code, with
/// two decimals.
pub fn currency(code: &str) -> Currency {
    let known = KNOWN.iter().find(|k| k.0 == code);
    Currency {
        code: code.to_owned(),
        symbol: known.map(|k| k.1),
        name: known.map(|k| k.2).unwrap_or(""),
        plural: known.map(|k| k.3).unwrap_or(""),
        decimals: 2,
    }
}

/// An amount field (toggle B), every word ready to draw: a sign before
/// what's typed, a chip naming the unit that switches it while there's a
/// price, and a line under it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AmountField {
    /// Typing in money: there's a price and the field is switched to it.
    pub in_fiat: bool,
    /// Before what's typed: "$", "€", or "ɱ" for XMR; none for a currency
    /// without a symbol, which the chip names.
    pub sign: Option<String>,
    /// The chip: "USD", "XMR".
    pub unit: String,
    /// The chip switches: there's a price to convert with.
    pub can_switch: bool,
    /// Under the field: "≈ 0.1204 XMR · 1 XMR = $166.10", "≈ $20.00 · 1
    /// XMR = $166.10", the rate alone while it's empty, "That isn't an
    /// amount"; nothing in XMR without a price.
    pub line: Option<String>,
    /// For TalkBack: "Amount in US dollars", "Amount in XMR".
    pub label: String,
    /// The chip for TalkBack: "Amount in XMR. Switch to US dollars".
    pub switch_label: Option<String>,
    /// What's typed comes to more than nothing: Send can review it.
    pub sendable: bool,
    /// The text and unit asked about: the field is only this while they're
    /// still what it holds (a switch changes both before a new one comes),
    /// so nothing is sent or shared from one that's behind.
    pub for_text: String,
    pub for_in_fiat: bool,
}

/// The field for `text` typed in money (`in_fiat`) or XMR, at `price` when
/// there is one (without, it takes XMR only).
pub fn amount_field(text: &str, in_fiat: bool, price: Option<(&Currency, u64)>) -> AmountField {
    let (for_text, for_in_fiat) = (text.to_owned(), in_fiat);
    let text = text.trim();
    let in_fiat = in_fiat && price.is_some();
    // "US dollars", "XMR".
    let unit_name = |fiat: bool| match price {
        Some((c, _)) if fiat && c.plural.is_empty() => c.code.clone(),
        Some((c, _)) if fiat => c.plural.to_owned(),
        _ => "XMR".to_owned(),
    };
    let line = match price {
        _ if text.is_empty() => price.map(|(c, p)| rate(p, c)),
        Some((c, p)) if in_fiat => parse(text, c).ok().and_then(|m| to_atomic(m, p, c)).map(|a| format!("≈ {} XMR · {}", amount::format(a), rate(p, c))),
        Some((c, p)) => amount::parse(text).ok().map(|a| format!("≈ {} · {}", value(a, p, c), rate(p, c))),
        None => None,
    };
    // What it comes to in XMR, the way Send converts it.
    let atomic = match price {
        Some((c, p)) if in_fiat => parse(text, c).ok().and_then(|m| to_atomic(m, p, c)),
        _ => amount::parse(text).ok(),
    };
    let valid = text.is_empty() || atomic.is_some();
    AmountField {
        in_fiat,
        sendable: atomic.is_some_and(|a| a > 0),
        sign: match price {
            Some((c, _)) if in_fiat => c.symbol.map(str::to_owned),
            _ => Some(amount::SIGN.to_owned()),
        },
        unit: match price {
            Some((c, _)) if in_fiat => c.code.clone(),
            _ => "XMR".into(),
        },
        can_switch: price.is_some(),
        line: if valid { line } else { Some("That isn't an amount".into()) },
        label: format!("Amount in {}", unit_name(in_fiat)),
        switch_label: price.map(|_| format!("Amount in {}. Switch to {}", unit_name(in_fiat), unit_name(!in_fiat))),
        for_text,
        for_in_fiat,
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

/// Digits with a comma every three: "1,234".
fn grouped(whole: u64) -> String {
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

/// `minor` units as digits: "1,234.56", "12.50"; `group` puts commas in.
fn digits(minor: u64, currency: &Currency, group: bool) -> String {
    let unit = 10u64.pow(currency.decimals);
    let whole = if group { grouped(minor / unit) } else { (minor / unit).to_string() };
    if currency.decimals == 0 { whole } else { format!("{whole}.{:0width$}", minor % unit, width = currency.decimals as usize) }
}

/// An amount of money, the same whatever the phone's language: the symbol
/// first when the currency has one ("$412.06", "€12.50", "$1,234.00"),
/// otherwise the code after ("412.06 GBP"); a point before the cents,
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
        return Err(FobError::BadAmount);
    }
    let unit = 10u64.pow(currency.decimals);
    let whole: u64 = if whole.is_empty() { 0 } else { whole.parse().map_err(|_| FobError::BadAmount)? };
    let frac: u64 = if frac.is_empty() { 0 } else { format!("{frac:0<width$}", width = currency.decimals as usize).parse().map_err(|_| FobError::BadAmount)? };
    whole.checked_mul(unit).and_then(|w| w.checked_add(frac)).ok_or(FobError::BadAmount)
}

/// A decimal price as text ("593.91000000") in millionths, rounded half up.
pub(crate) fn micro(text: &str) -> Option<u64> {
    let (whole, frac) = text.trim().split_once('.').unwrap_or((text.trim(), ""));
    if whole.is_empty() || !whole.chars().all(|c| c.is_ascii_digit()) || !frac.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let whole: u64 = whole.parse().ok()?;
    let mut digits: Vec<u64> = frac.bytes().map(|b| u64::from(b - b'0')).collect();
    digits.resize(7, 0);
    let six = digits[..6].iter().fold(0, |n, d| n * 10 + d);
    let up = u64::from(digits[6] >= 5);
    whole.checked_mul(MICRO)?.checked_add(six + up)
}

/// The change between a payment's value then and now: "Up $3.63 (4.6%)
/// since then", "Down $1.20 (0.8%) since then", "No change since then",
/// and whether it went up.
pub fn change(then: u64, now: u64, currency: &Currency) -> (String, bool) {
    if then == now {
        return ("No change since then".into(), false);
    }
    let up = now > then;
    let diff = now.abs_diff(then);
    let percent = if then == 0 {
        String::new()
    } else {
        // Tenths of a percent, rounded half up.
        let tenths = (diff as u128 * 1000 + then as u128 / 2) / then as u128;
        format!(" ({}.{}%)", tenths / 10, tenths % 10)
    };
    (format!("{} {}{percent} since then", if up { "Up" } else { "Down" }, format(diff, currency)), up)
}

/// The UTC day `time` falls in.
pub fn day(time: u64) -> i64 {
    (time / DAY) as i64
}

/// The window of days a provider's history is asked for, ending today.
pub(crate) fn window(source: &impl Source, now: u64) -> Range<i64> {
    let today = day(now);
    today - source.history_days() + 1..today + 1
}

/// Whether to ask for the current price now: none yet for these settings,
/// or the last one is older than 5 minutes (a minute for pull to refresh).
pub(crate) fn current_due(cache: &Cache, settings: &Settings, now: u64, pulled: bool) -> bool {
    let wait = if pulled { PULL_REFRESH } else { REFRESH };
    match &cache.current {
        Some(c) if c.currency == settings.currency && c.provider == settings.provider.name() => now.saturating_sub(c.at) >= wait,
        _ => true,
    }
}

/// Whether to ask for the daily history now: not asked today for these
/// settings.
pub(crate) fn history_due(cache: &Cache, settings: &Settings, now: u64) -> bool {
    cache.history(settings).is_none_or(|h| now.saturating_sub(h.fetched) >= HISTORY_EVERY)
}

pub(crate) fn store_history(cache: &mut Cache, settings: &Settings, days: Vec<(i64, u64)>, now: u64) {
    cache.keep_history(settings, days, now);
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
    fn the_amount_field_reads_like_toggle_b() {
        let (usd, eur, chf) = (usd(), eur(), currency("CHF"));
        let p = 166_100_000;
        // In dollars, as the board: "$ 20 [USD ⇅]", "≈ 0.1204 XMR · 1 XMR = $166.10".
        let f = amount_field("20", true, Some((&usd, p)));
        assert_eq!((f.sign.as_deref(), f.unit.as_str(), f.in_fiat, f.can_switch), (Some("$"), "USD", true, true));
        assert_eq!(f.line.as_deref(), Some("≈ 0.1204 XMR · 1 XMR = $166.10"));
        assert_eq!((f.label.as_str(), f.switch_label.as_deref()), ("Amount in US dollars", Some("Amount in US dollars. Switch to XMR")));
        // In XMR: the sign becomes ɱ.
        let f = amount_field("0.1204", false, Some((&usd, p)));
        assert_eq!((f.sign.as_deref(), f.unit.as_str(), f.in_fiat), (Some("ɱ"), "XMR", false));
        assert_eq!(f.line.as_deref(), Some("≈ $20.00 · 1 XMR = $166.10"));
        assert_eq!((f.label.as_str(), f.switch_label.as_deref()), ("Amount in XMR", Some("Amount in XMR. Switch to US dollars")));
        // Empty: the rate alone. Not an amount: says so.
        assert_eq!(amount_field(" ", true, Some((&usd, p))).line.as_deref(), Some("1 XMR = $166.10"));
        assert_eq!(amount_field("20.123", true, Some((&usd, p))).line.as_deref(), Some("That isn't an amount"));
        assert_eq!(amount_field("abc", false, Some((&usd, p))).line.as_deref(), Some("That isn't an amount"));
        // Euros, and a currency without a symbol: the chip names it, no sign.
        let f = amount_field("5", true, Some((&eur, p)));
        assert_eq!((f.sign.as_deref(), f.unit.as_str(), f.label.as_str()), (Some("€"), "EUR", "Amount in euros"));
        let f = amount_field("5", true, Some((&chf, p)));
        assert_eq!((f.sign, f.unit.as_str(), f.label.as_str()), (None, "CHF", "Amount in CHF"));
        assert_eq!(f.line.as_deref(), Some("≈ 0.0301 XMR · 1 XMR = 166.10 CHF"));
        // No price: XMR only, no switch, nothing under it unless it's wrong.
        let f = amount_field("20", true, None);
        assert_eq!((f.sign.as_deref(), f.unit.as_str(), f.in_fiat, f.can_switch, f.line, f.switch_label), (Some("ɱ"), "XMR", false, false, None, None));
        assert_eq!(amount_field("x", false, None).line.as_deref(), Some("That isn't an amount"));
    }

    #[test]
    fn the_amount_field_says_when_send_can_review_it() {
        let usd = usd();
        let p = 166_100_000;
        let sendable = |text: &str, in_fiat: bool, price: Option<(&Currency, u64)>| amount_field(text, in_fiat, price).sendable;
        assert!(sendable("20", true, Some((&usd, p))));
        assert!(sendable("0.1204", false, Some((&usd, p))));
        // Typed in money without a price, it's read as XMR.
        assert!(sendable("20", true, None));
        // It says what it was asked about, as asked.
        let f = amount_field(" 20", true, None);
        assert_eq!((f.for_text.as_str(), f.for_in_fiat, f.in_fiat), (" 20", true, false));
        for (text, in_fiat) in [("", true), (" ", false), ("0", false), ("0.00", true), ("20.123", true), ("abc", false)] {
            assert!(!sendable(text, in_fiat, Some((&usd, p))), "{text:?}");
        }
    }

    #[test]
    fn money_is_written_one_way_whatever_the_language() {
        assert_eq!(format(41_206, &usd()), "$412.06");
        assert_eq!(format(1_250, &eur()), "€12.50");
        assert_eq!(format(41_206, &gbp()), "412.06 GBP");
        assert_eq!(format(123_456_700, &usd()), "$1,234,567.00");
        assert_eq!(format(5, &usd()), "$0.05");
        assert_eq!(format(0, &usd()), "$0.00");
        assert_eq!(plain(123_450, &usd()), "1234.50");
        assert_eq!(currency("JPY").symbol, None);
        assert_eq!(eur().symbol, Some("€"));
        assert_eq!(gbp().name, "");
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
    fn decimal_prices_become_millionths() {
        assert_eq!(micro("593.91000000"), Some(593_910_000));
        assert_eq!(micro("166.1"), Some(166_100_000));
        assert_eq!(micro("0.0000005"), Some(1));
        assert_eq!(micro("0.0000004"), Some(0));
        assert_eq!(micro("12"), Some(12_000_000));
        assert_eq!(micro("1e3"), None);
        assert_eq!(micro("-1"), None);
        assert_eq!(micro(""), None);
    }

    #[test]
    fn changes_since_then_read_as_words() {
        assert_eq!(change(7_900, 8_263, &usd()), ("Up $3.63 (4.6%) since then".into(), true));
        assert_eq!(change(15_000, 14_880, &usd()), ("Down $1.20 (0.8%) since then".into(), false));
        assert_eq!(change(100, 100, &usd()).0, "No change since then");
    }

    #[test]
    fn a_value_comes_from_the_moment_or_else_the_day() {
        let settings = Settings::default();
        let now = 1_793_448_000;
        let mut cache = Cache { current: Some(Current { currency: "USD".into(), provider: "Kraken".into(), price: 166_000_000, at: now }), history: vec![] };
        store_history(&mut cache, &settings, vec![(day(now) - 1, 160_000_000), (day(now), 165_000_000)], now);
        // Within the hour: the price of the moment.
        assert_eq!(cache.value_at(&settings, now - 600).map(|r| (r.price, r.daily)), Some((166_000_000, false)));
        // Earlier today and yesterday: the day's price.
        assert_eq!(cache.value_at(&settings, now - 2 * 3600).map(|r| (r.price, r.daily)), Some((165_000_000, true)));
        assert_eq!(cache.value_at(&settings, now - DAY).map(|r| r.price), Some(160_000_000));
        // Before the history: not available.
        assert_eq!(cache.value_at(&settings, now - 3 * DAY), None);
        // In another currency, nothing.
        let eur = Settings { currency: "EUR".into(), ..settings.clone() };
        assert_eq!(cache.value_at(&eur, now), None);
        assert!(cache.fresh(&settings, now + FRESH - 1).is_some());
        assert!(cache.fresh(&settings, now + FRESH).is_none());
    }

    #[test]
    fn history_is_bounded() {
        let mut cache = Cache::default();
        let days: Vec<(i64, u64)> = (0..HISTORY_DAYS as i64 + 50).map(|d| (d, 1)).collect();
        for (i, code) in ["USD", "EUR", "GBP", "CHF"].into_iter().enumerate() {
            let settings = Settings { currency: code.into(), ..Settings::default() };
            store_history(&mut cache, &settings, days.clone(), i as u64);
        }
        assert_eq!(cache.history.len(), HISTORIES);
        assert!(cache.history.iter().all(|h| h.days.len() == HISTORY_DAYS && h.days[0].0 == 50));
        assert!(!cache.history.iter().any(|h| h.currency == "USD"), "the oldest asked went");
    }

    #[test]
    fn requests_wait_their_turn() {
        let settings = Settings::default();
        let now = 1_793_448_000;
        let mut cache = Cache::default();
        assert!(current_due(&cache, &settings, now, false));
        cache.current = Some(Current { currency: "USD".into(), provider: "Kraken".into(), price: 1, at: now });
        assert!(!current_due(&cache, &settings, now + REFRESH - 1, false));
        assert!(current_due(&cache, &settings, now + REFRESH, false));
        assert!(!current_due(&cache, &settings, now + PULL_REFRESH - 1, true));
        assert!(current_due(&cache, &settings, now + PULL_REFRESH, true));
        // Another currency is asked at once.
        assert!(current_due(&cache, &Settings { currency: "EUR".into(), ..settings.clone() }, now, false));
        assert!(history_due(&cache, &settings, now));
        store_history(&mut cache, &settings, vec![], now);
        assert!(!history_due(&cache, &settings, now + HISTORY_EVERY - 1));
        assert!(history_due(&cache, &settings, now + HISTORY_EVERY));
    }

    #[test]
    fn an_old_wallet_file_gets_the_defaults() {
        let settings: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(settings, Settings::default());
        assert!(settings.on, "prices are on unless turned off");
        assert_eq!(settings.source_text(), "Kraken · over the internet");
        let cache: Cache = serde_json::from_str("{}").unwrap();
        assert_eq!(cache, Cache::default());
    }

    #[tokio::test]
    async fn tor_says_it_isnt_there_yet_and_onions_are_refused() {
        let tor = Transport::Tor.get("https://api.kraken.com/0/public/Time").await.unwrap_err();
        assert_eq!(tor.to_string(), "prices over Tor aren't available yet");
        let onion = Transport::Web.get("https://pricesxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx.onion/x").await.unwrap_err();
        assert!(onion.to_string().contains(".onion"), "{onion}");
        let plain = Transport::Web.get("http://api.kraken.com/0/public/Time").await.unwrap_err();
        assert!(plain.to_string().contains("https"), "{plain}");
    }
}
