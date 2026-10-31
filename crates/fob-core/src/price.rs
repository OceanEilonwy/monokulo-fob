//! Prices: what XMR is worth in the money people think in, now and on the
//! day a payment moved.
//!
//! Three choices, made in Settings and kept in the wallet file
//! ([`Settings`]):
//!
//! - the **currency** amounts are also shown in, one of the 45 baked into
//!   [`currencies`], or none: XMR only, and Fob never asks for a price.
//! - the **provider** ([`Provider`]): whose prices, from which API.
//!   Kraken's public REST API ([`kraken`]) prices 6 of them, CoinGecko's
//!   keyless one ([`coingecko`]) all 45. Which provider prices which
//!   currency is the providers' own list ([`providers_for`]).
//! - the **transport** ([`Transport`]): how the requests travel. Straight
//!   over the internet ([`Transport::Web`]); Tor ([`Transport::Tor`]) has its
//!   place but isn't built yet, and says so.
//!
//! A provider is anything implementing [`Source`]; [`Provider`] picks one
//! by name for the settings. Adding one is a variant there and its own
//! module beside [`kraken`].
//!
//! Privacy:
//! - No requests at all with XMR only.
//! - A request names the pair or currency and nothing else: never a
//!   wallet's address, amount or payment, and only a plain `User-Agent:
//!   Fob`.
//! - Never a price for a payment's own moment, which would tell the
//!   provider when someone paid: history comes as the provider's whole
//!   daily window, at most once a day, and is looked up here.
//! - The current price is asked at most every 5 minutes while Fob is on
//!   screen, and on pull to refresh, Retry, a changed currency or provider
//!   and a money amount typed against a price 5 minutes old (at most once a
//!   minute for the same settings). Failures show as the toast of [`alert`].
//!
//! Prices are whole numbers: millionths of the currency for one XMR
//! (`price`), never floats. Values are rounded half up to the currency's
//! smallest unit (cents, or whole yen).

pub mod alert;
pub mod coingecko;
pub mod currencies;
pub mod kraken;

use std::{future::Future, ops::Range, sync::LazyLock, time::Duration};

use serde::{Deserialize, Serialize};
use serde_json::Value;

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
/// A current price older than this isn't recorded with a payment.
pub const FRESH: u64 = 60 * 60;
/// A current price older than this isn't shown, nor converts money typed:
/// amounts stay in XMR.
pub const SHOWN: u64 = 24 * 60 * 60;
/// A price older than this says when it's from where it converts an
/// amount: one refresh missed.
pub const AGED: u64 = 2 * REFRESH;
/// How often the daily history is asked for.
pub const HISTORY_EVERY: u64 = 24 * 60 * 60;
/// How long Price from waits for each provider as it opens.
pub const CHECK_TIMEOUT: Duration = Duration::from_secs(8);
/// Days of history kept per currency, and how many currencies' histories.
const HISTORY_DAYS: usize = 1_000;
const HISTORIES: usize = 3;
/// The most a price answer may be: Kraken's 720 days are about 60 kB,
/// CoinGecko's 365 about 30 kB.
const MAX_RESPONSE: usize = 1 << 20;
const DAY: u64 = 86_400;

pub const USER_AGENT: &str = "Fob";
const TOR_NOT_YET: &str = "prices over Tor aren't available yet";

/// Whose prices, in the order Fob prefers them: moving to a provider that
/// can price a currency takes the first that can.
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
    CoinGecko,
}

impl Provider {
    pub const ALL: [Provider; 2] = [Provider::Kraken, Provider::CoinGecko];

    /// Whether it prices XMR in `code`.
    pub fn prices(self, code: &str) -> bool {
        self.currencies().contains(&code)
    }

    /// The provider called `name` ("Kraken").
    pub fn named(name: &str) -> Option<Provider> {
        Provider::ALL.into_iter().find(|p| p.name() == name)
    }
}

/// The providers that price XMR in `code`, in [`Provider::ALL`]'s order.
pub fn providers_for(code: &str) -> Vec<Provider> {
    Provider::ALL.into_iter().filter(|p| p.prices(code)).collect()
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

    /// "Web", "Tor": its title on Price from's Ask over rows.
    pub fn name(self) -> &'static str {
        match self {
            Transport::Web => "Web",
            Transport::Tor => "Tor",
        }
    }

    /// The body at `url`, an `https://` address (never an onion one: Fob
    /// would resolve it through the phone's DNS).
    pub async fn get(self, url: &str) -> Result<Vec<u8>> {
        match self {
            Transport::Tor => Err(FobError::PriceFailed(Failure::TorNotYet)),
            Transport::Web => web(url).await,
        }
    }
}

/// Why a provider gave no price, in words for its line on Price from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    /// Couldn't be reached, didn't answer in time, or answered with an error.
    NotAnswering,
    /// Said it's asked too often (HTTP 429, or Kraken's rate limit).
    Busy,
    /// Answered, but not with a price Fob could read.
    BadAnswer,
    /// Asked over Tor, which isn't built yet.
    TorNotYet,
    /// Kept over Tor in a wallet file for a provider that doesn't answer
    /// over it: never asked over the web instead without the user's pick.
    NotOverTor,
}

impl Failure {
    pub fn text(self) -> &'static str {
        match self {
            Failure::NotAnswering => "Not answering",
            Failure::Busy => "Busy, try again soon",
            Failure::BadAnswer => "Answered without a price",
            Failure::TorNotYet => "Tor isn't built yet",
            Failure::NotOverTor => "Doesn't answer over Tor",
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
    let failed = |f: Failure| FobError::PriceFailed(f);
    let parsed = reqwest::Url::parse(url).map_err(|_| err(format!("{url} isn't an address")))?;
    if parsed.scheme() != "https" {
        return Err(err("prices are only asked over https".into()));
    }
    // An onion price source would be looked up through the phone's DNS.
    if crate::node::is_onion(parsed.host_str().unwrap_or("")) {
        return Err(err("Fob can't reach .onion price sources yet".into()));
    }
    let client = CLIENT.as_ref().map_err(|e| err(e.clone()))?;
    let mut response = client.get(parsed).send().await.map_err(|_| failed(Failure::NotAnswering))?;
    if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err(failed(Failure::Busy));
    }
    if !response.status().is_success() {
        return Err(failed(Failure::NotAnswering));
    }
    let mut body = vec![];
    while let Some(chunk) = response.chunk().await.map_err(|_| failed(Failure::NotAnswering))? {
        body.extend_from_slice(&chunk);
        if body.len() > MAX_RESPONSE {
            return Err(failed(Failure::BadAnswer));
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
    /// Whether it answers requests sent over `transport`.
    fn answers_over(&self, transport: Transport) -> bool;
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
            Provider::CoinGecko => coingecko::CoinGecko.name(),
        }
    }

    fn currencies(&self) -> &'static [&'static str] {
        match self {
            Provider::Kraken => kraken::Kraken.currencies(),
            Provider::CoinGecko => coingecko::CoinGecko.currencies(),
        }
    }

    fn answers_over(&self, transport: Transport) -> bool {
        match self {
            Provider::Kraken => kraken::Kraken.answers_over(transport),
            Provider::CoinGecko => coingecko::CoinGecko.answers_over(transport),
        }
    }

    async fn current(&self, currency: &str, transport: Transport) -> Result<u64> {
        match self {
            Provider::Kraken => kraken::Kraken.current(currency, transport).await,
            Provider::CoinGecko => coingecko::CoinGecko.current(currency, transport).await,
        }
    }

    async fn daily(&self, currency: &str, days: Range<i64>, transport: Transport) -> Result<Vec<(i64, u64)>> {
        match self {
            Provider::Kraken => kraken::Kraken.daily(currency, days, transport).await,
            Provider::CoinGecko => coingecko::CoinGecko.daily(currency, days, transport).await,
        }
    }

    fn history_days(&self) -> i64 {
        match self {
            Provider::Kraken => kraken::Kraken.history_days(),
            Provider::CoinGecko => coingecko::CoinGecko.history_days(),
        }
    }
}

/// How prices are shown, kept in the wallet file. A file from before
/// prices opens with the defaults; one from before currencies (an on/off
/// switch) as [`Stored`] reads it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "Stored")]
pub struct Settings {
    /// "Also show amounts in": an ISO 4217 code from [`currencies`] that
    /// `provider` prices, or `None` for XMR only, when no request is ever
    /// made.
    pub currency: Option<String>,
    pub provider: Provider,
    /// One `provider` answers over.
    pub transport: Transport,
    /// "Show first" the currency: Home's balance leads with it.
    pub fiat_first: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { currency: Some("USD".into()), provider: Provider::Kraken, transport: Transport::Web, fiat_first: false }
    }
}

/// [`Settings`] as any wallet file has them. Files from before currencies
/// had `on` beside a currency always set: off is XMR only. `currency`
/// missing (a file from before prices) is dollars; `null` is XMR only.
#[derive(Deserialize)]
#[serde(default)]
struct Stored {
    on: bool,
    currency: Option<String>,
    provider: Provider,
    transport: Transport,
    fiat_first: bool,
}

impl Default for Stored {
    fn default() -> Self {
        let d = Settings::default();
        Stored { on: true, currency: d.currency, provider: d.provider, transport: d.transport, fiat_first: d.fiat_first }
    }
}

impl From<Stored> for Settings {
    fn from(s: Stored) -> Self {
        let mut settings = Settings { currency: s.currency.filter(|_| s.on), provider: s.provider, transport: s.transport, fiat_first: s.fiat_first };
        // A code Fob no longer knows is XMR only; a provider that doesn't
        // price the currency moves as choosing would. The transport never
        // moves here: off Tor would show the phone's address unannounced,
        // so lookups fail saying why until one is picked.
        if settings.code().is_some_and(|c| currencies::find(c).is_none()) {
            settings.currency = None;
        }
        settings.fit_provider();
        settings
    }
}

impl Settings {
    /// The currency's code, or `None` for XMR only.
    pub fn code(&self) -> Option<&str> {
        self.currency.as_deref()
    }

    /// "Kraken · web", "CoinGecko · Tor": the provider and the Ask over
    /// row picked, by that row's name.
    pub fn source_text(&self) -> String {
        let transport = match self.transport {
            Transport::Web => "web",
            Transport::Tor => self.transport.name(),
        };
        format!("{} · {transport}", self.provider.name())
    }

    /// Moves the provider to the first that prices the currency, and the
    /// transport to the web when the provider doesn't answer over it.
    /// Returns (whether the provider moved, whether the transport did).
    fn fit(&mut self) -> (bool, bool) {
        let mut moved = (self.fit_provider(), false);
        if !self.provider.answers_over(self.transport) {
            self.transport = Transport::Web;
            moved.1 = true;
        }
        moved
    }

    /// Moves the provider to the first that prices the currency; whether it moved.
    fn fit_provider(&mut self) -> bool {
        if let Some(code) = self.code()
            && !self.provider.prices(code)
            && let Some(first) = providers_for(code).first()
        {
            self.provider = *first;
            return true;
        }
        false
    }

    /// "Also show amounts in" `code`, or XMR only (`None`). Moves to a
    /// provider that prices it when this one doesn't, and says so: "Kraken
    /// doesn't price New Zealand dollars, so Fob now asks CoinGecko".
    pub fn choose_currency(&mut self, code: Option<&str>) -> Result<Option<String>> {
        let Some(code) = code else {
            self.currency = None;
            return Ok(None);
        };
        let info = currencies::find(code).ok_or_else(|| FobError::Price(format!("Fob has no prices in {code}")))?;
        let before = self.provider;
        self.currency = Some(code.to_owned());
        let (provider, transport) = self.fit();
        Ok(provider.then(|| {
            let over = if transport { " over the web" } else { "" };
            format!("{} doesn't price {}, so Fob now asks {}{over}", before.name(), info.plural, self.provider.name())
        }))
    }

    /// Prices from `provider`, one that prices the currency. Moves to the
    /// web when it doesn't answer over Tor, and says so: "Kraken doesn't
    /// answer over Tor, so Fob now asks over the web".
    pub fn choose_provider(&mut self, provider: Provider) -> Result<Option<String>> {
        if let Some(code) = self.code()
            && !provider.prices(code)
        {
            return Err(FobError::Price(format!("{} doesn't price {}", provider.name(), currency(code).plural)));
        }
        let transport = self.transport;
        self.provider = provider;
        let (_, moved) = self.fit();
        Ok(moved.then(|| format!("{} doesn't answer {}, so Fob now asks over the web", provider.name(), transport.text())))
    }

    /// Asks `transport`, one that's built and the provider answers over.
    pub fn choose_transport(&mut self, transport: Transport) -> Result<()> {
        if !transport.available() {
            return Err(FobError::Price(TOR_NOT_YET.into()));
        }
        if !self.provider.answers_over(transport) {
            return Err(FobError::Price(format!("{} doesn't answer {}", self.provider.name(), transport.text())));
        }
        self.transport = transport;
        Ok(())
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

/// Whether a price kept for `currency` from `provider` is one `settings` show.
fn for_settings(settings: &Settings, currency: &str, provider: &str) -> bool {
    settings.code() == Some(currency) && settings.provider.name() == provider
}

impl Cache {
    /// The last current price kept for `settings`, however old.
    pub fn last(&self, settings: &Settings) -> Option<&Current> {
        self.current.as_ref().filter(|c| for_settings(settings, &c.currency, &c.provider))
    }

    /// The current price for `settings`, if asked within the last day.
    pub fn shown(&self, settings: &Settings, now: u64) -> Option<&Current> {
        self.last(settings).filter(|c| now.saturating_sub(c.at) < SHOWN)
    }

    fn history(&self, settings: &Settings) -> Option<&History> {
        self.history.iter().find(|h| for_settings(settings, &h.currency, &h.provider))
    }

    /// Keeps `days` as the history for `settings`, the newest `HISTORY_DAYS`
    /// of them, and at most `HISTORIES` currencies (the oldest asked go).
    fn keep_history(&mut self, settings: &Settings, mut days: Vec<(i64, u64)>, now: u64) {
        let Some(code) = settings.code() else { return };
        days.sort_by_key(|d| d.0);
        days.dedup_by_key(|d| d.0);
        if days.len() > HISTORY_DAYS {
            days.drain(..days.len() - HISTORY_DAYS);
        }
        self.history.retain(|h| !for_settings(settings, &h.currency, &h.provider));
        self.history.push(History { currency: code.to_owned(), provider: settings.provider.name().into(), fetched: now, days });
        self.history.sort_by_key(|h| std::cmp::Reverse(h.fetched));
        self.history.truncate(HISTORIES);
    }

    /// What XMR was worth at `time` for `settings`: the current price if it
    /// was asked within an hour of then, else that UTC day's price.
    pub fn value_at(&self, settings: &Settings, time: u64) -> Option<Record> {
        if let Some(c) = self.current.as_ref().filter(|c| for_settings(settings, &c.currency, &c.provider) && c.at.abs_diff(time) < FRESH) {
            return Some(Record { currency: c.currency.clone(), price: c.price, provider: c.provider.clone(), at: c.at, daily: false });
        }
        let history = self.history(settings)?;
        let day = (time / DAY) as i64;
        let i = history.days.binary_search_by_key(&day, |d| d.0).ok()?;
        Some(Record { currency: history.currency.clone(), price: history.days[i].1, provider: history.provider.clone(), at: day as u64 * DAY, daily: true })
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

    /// Its sign in a list: the symbol, or the code.
    pub fn sign(&self) -> String {
        self.symbol.map(str::to_owned).unwrap_or_else(|| self.code.clone())
    }
}

/// Settings' "Also show amounts in" value: "Australian dollar", or "XMR only".
pub fn shown_in_text(settings: &Settings) -> String {
    settings.code().map(|c| currency(c).text()).unwrap_or_else(|| XMR_ONLY.into())
}

const XMR_ONLY: &str = "XMR only";

/// Under "Also show amounts in": "Uses today's XMR price", or with XMR
/// only "Fob never looks up a price".
pub fn shown_in_line(settings: &Settings) -> &'static str {
    if settings.code().is_some() { "Uses today's XMR price" } else { NEVER_LOOKS }
}

const NEVER_LOOKS: &str = "Fob never looks up a price";

/// One way to ask on Price from's "Ask over".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransportRow {
    pub transport: Transport,
    /// "Web", "Tor"
    pub title: &'static str,
    /// "The provider sees this phone's IP address", "Kraken doesn't answer
    /// over Tor".
    pub line: String,
    /// It can be picked: built, and the provider answers over it.
    pub available: bool,
    pub chosen: bool,
}

/// "Ask over": Web, then Tor, greyed while the provider doesn't answer
/// over it or Fob hasn't Tor built in, its line saying which.
pub fn transport_rows(settings: &Settings) -> Vec<TransportRow> {
    [Transport::Web, Transport::Tor]
        .into_iter()
        .map(|t| {
            let line = match t {
                Transport::Web => "The provider sees this phone's IP address".to_owned(),
                Transport::Tor if !settings.provider.answers_over(t) => format!("{} doesn't answer {}", settings.provider.name(), t.text()),
                Transport::Tor if !t.available() => "Hides this phone's IP address · not built into Fob yet".to_owned(),
                Transport::Tor => "Hides this phone's IP address; slower".to_owned(),
            };
            TransportRow { transport: t, title: t.name(), line, available: t.available() && settings.provider.answers_over(t), chosen: t == settings.transport }
        })
        .collect()
}

/// One row of the "Also show amounts in" list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CurrencyRow {
    /// The code to choose, or `None` for XMR only.
    pub code: Option<String>,
    /// In the circle: "A$", "CHF", "ɱ".
    pub sign: String,
    /// "Australian dollar", "XMR only".
    pub name: String,
    /// "AUD · Kraken, CoinGecko", "Fob never looks up a price".
    pub line: String,
    pub chosen: bool,
}

/// A titled part of the list: "NONE", "THIS PHONE’S", "ALL 45".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CurrencyGroup {
    pub title: String,
    pub rows: Vec<CurrencyRow>,
}

/// The "Also show amounts in" list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CurrencyList {
    pub groups: Vec<CurrencyGroup>,
    /// Instead of the groups when a search finds nothing: "No currency
    /// matches “xyz”".
    pub empty: Option<String>,
}

fn currency_row(info: &currencies::Info, chosen: Option<&str>) -> CurrencyRow {
    let names: Vec<&str> = providers_for(info.code).iter().map(|p| p.name()).collect();
    CurrencyRow {
        code: Some(info.code.to_owned()),
        sign: currency(info.code).sign(),
        name: info.name.to_owned(),
        line: format!("{} · {}", info.code, names.join(", ")),
        chosen: chosen == Some(info.code),
    }
}

/// The "Also show amounts in" list: XMR only, the phone's own currency
/// (`phone`, its ISO 4217 code, when Fob has it), then all 45 A–Z by name;
/// `query` keeps only what it matches by name, code or symbol (whatever
/// the case or accents), and leaves out the phone's row, already among the
/// rest. `chosen` is the current code, `None` for XMR only.
pub fn currency_list(chosen: Option<&str>, phone: Option<&str>, query: &str) -> CurrencyList {
    let searching = !query.trim().is_empty();
    let mut groups = vec![];
    let xmr = currencies::fold(query.trim());
    if !searching || ["xmr only", amount::SIGN, "monero"].iter().any(|t| currencies::fold(t).contains(&xmr)) {
        let row = CurrencyRow {
            code: None,
            sign: amount::SIGN.into(),
            name: XMR_ONLY.into(),
            line: NEVER_LOOKS.into(),
            chosen: chosen.is_none(),
        };
        groups.push(CurrencyGroup { title: "NONE".into(), rows: vec![row] });
    }
    if !searching && let Some(info) = phone.and_then(|p| currencies::find(&p.trim().to_ascii_uppercase())) {
        groups.push(CurrencyGroup { title: "THIS PHONE’S".into(), rows: vec![currency_row(info, chosen)] });
    }
    let rows: Vec<CurrencyRow> = currencies::by_name().into_iter().filter(|c| currencies::matches(c, query)).map(|c| currency_row(c, chosen)).collect();
    if !rows.is_empty() {
        let title = if searching { format!("{} OF {}", rows.len(), currencies::TABLE.len()) } else { format!("ALL {}", currencies::TABLE.len()) };
        groups.push(CurrencyGroup { title, rows });
    }
    let empty = groups.is_empty().then(|| format!("No currency matches “{}”", query.trim()));
    CurrencyList { groups, empty }
}

/// One provider's line on Price from, checked as it opens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Check {
    /// "Kraken"
    pub name: &'static str,
    /// It answers over the web, and over Tor.
    pub web: bool,
    pub tor: bool,
    /// The provider prices come from now.
    pub chosen: bool,
    /// What it says XMR is worth: "A$845.10"; `None` when it failed or
    /// wasn't asked.
    pub rate: Option<String>,
    /// Why it gave no price: "Not answering", "Busy, try again soon".
    pub failure: Option<String>,
    /// It can be picked: it answered, or it wasn't asked because it
    /// doesn't answer over Tor (picking it moves to the web).
    pub can_pick: bool,
    /// For TalkBack: "Kraken, web only, A$845.10".
    pub label: String,
}

/// Asks each of `sources` (those pricing `currency`) for XMR's price now,
/// all at once, giving each `timeout`, over `transport`; one that doesn't
/// answer over it isn't asked (asking over the web while Tor is chosen
/// would show the phone's address). `chosen` names the current provider.
/// The lines come back in `sources`' order.
pub async fn check<S: Source + Clone + 'static>(sources: Vec<S>, currency: &Currency, transport: Transport, chosen: &str, timeout: Duration) -> Vec<Check> {
    let mut asks = tokio::task::JoinSet::new();
    for (i, source) in sources.iter().enumerate() {
        if source.answers_over(transport) {
            let (source, code) = (source.clone(), currency.code.clone());
            asks.spawn(async move {
                let answer = match tokio::time::timeout(timeout, source.current(&code, transport)).await {
                    Ok(Ok(p)) if p > 0 => Ok(p),
                    Ok(Ok(_)) => Err(Failure::BadAnswer),
                    Ok(Err(FobError::PriceFailed(f))) => Err(f),
                    Ok(Err(_)) | Err(_) => Err(Failure::NotAnswering),
                };
                (i, answer)
            });
        }
    }
    let mut answers: Vec<Option<std::result::Result<u64, Failure>>> = vec![None; sources.len()];
    while let Some(done) = asks.join_next().await {
        if let Ok((i, answer)) = done {
            answers[i] = Some(answer);
        }
    }
    sources
        .iter()
        .zip(answers)
        .map(|(source, answer)| {
            let (web, tor) = (source.answers_over(Transport::Web), source.answers_over(Transport::Tor));
            let rate = answer.and_then(|a| a.ok()).map(|p| value(ATOMIC_PER_XMR, p, currency));
            let failure = match answer {
                Some(Err(f)) => Some(f.text().to_owned()),
                // Asked but lost (its task panicked): as good as silent.
                None if source.answers_over(transport) => Some(Failure::NotAnswering.text().to_owned()),
                _ => None,
            };
            let ways = match (web, tor) {
                (true, true) => "web and Tor",
                (true, false) => "web only",
                (false, true) => "Tor only",
                (false, false) => "no way to ask",
            };
            let said = rate.clone().or_else(|| failure.as_ref().map(|f| f.to_lowercase())).unwrap_or_else(|| format!("not asked {}", transport.text()));
            Check {
                name: source.name(),
                web,
                tor,
                chosen: source.name() == chosen,
                can_pick: failure.is_none(),
                label: format!("{}, {ways}, {said}", source.name()),
                rate,
                failure,
            }
        })
        .collect()
}

/// A JSON number as a price in millionths, rounded half up: from its
/// digits when it has a plain decimal form, else (an exponent) from its
/// double. `None` for a negative or non-number.
pub(crate) fn micro_number(n: &Value) -> Option<u64> {
    if let Some(whole) = n.as_u64() {
        return whole.checked_mul(MICRO);
    }
    let text = n.as_number()?.to_string();
    micro(&text).or_else(|| {
        let f = n.as_f64().filter(|f| f.is_finite() && *f >= 0.0)? * MICRO as f64;
        (f < u64::MAX as f64).then(|| f.round() as u64)
    })
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

/// What an amount field converts with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldPrice<'a> {
    /// XMR only: no currency chosen.
    None,
    /// A currency chosen, but no price for it, or only one over a day old:
    /// the field takes XMR only, and says so.
    Missing(&'a Currency),
    /// A price, with its age once it isn't just asked ("price from 14:02").
    At(&'a Currency, u64, Option<&'a str>),
}

/// The field for `text` typed in money (`in_fiat`) or XMR, at `price` when
/// there is one (without, it takes XMR only).
pub fn amount_field(text: &str, in_fiat: bool, price: FieldPrice) -> AmountField {
    let (for_text, for_in_fiat) = (text.to_owned(), in_fiat);
    let text = text.trim();
    let (missing, age) = match price {
        FieldPrice::Missing(c) => (Some(c), None),
        FieldPrice::At(_, _, age) => (None, age),
        FieldPrice::None => (None, None),
    };
    let price = match price {
        FieldPrice::At(c, p, _) => Some((c, p)),
        _ => None,
    };
    let in_fiat = in_fiat && price.is_some();
    // "US dollars", "XMR".
    let unit_name = |fiat: bool| match price {
        Some((c, _)) if fiat && c.plural.is_empty() => c.code.clone(),
        Some((c, _)) if fiat => c.plural.to_owned(),
        _ => "XMR".to_owned(),
    };
    // The rate; once the price is aged, when it's from in its place, so
    // the line stays short.
    let from = |p: u64, c: &Currency| age.map_or_else(|| rate(p, c), str::to_owned);
    let line = match price {
        Some((c, p)) if text.is_empty() => Some(age.map_or_else(|| rate(p, c), |age| format!("{} · {age}", rate(p, c)))),
        Some((c, p)) if in_fiat => parse(text, c).ok().and_then(|m| to_atomic(m, p, c)).map(|a| format!("≈ {} XMR · {}", amount::format(a), from(p, c))),
        Some((c, p)) => amount::parse(text).ok().map(|a| format!("≈ {} · {}", value(a, p, c), from(p, c))),
        // Money can't be typed: why.
        None => missing.map(|c| format!("No {} price now · type the amount in XMR", c.code)),
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
pub(crate) fn grouped(whole: u64) -> String {
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
/// Never with XMR only.
pub(crate) fn current_due(cache: &Cache, settings: &Settings, now: u64, pulled: bool) -> bool {
    let wait = if pulled { PULL_REFRESH } else { REFRESH };
    match &cache.current {
        _ if settings.code().is_none() => false,
        Some(c) if for_settings(settings, &c.currency, &c.provider) => now.saturating_sub(c.at) >= wait,
        _ => true,
    }
}

/// Whether to ask for the daily history now: not asked today for these
/// settings. Never with XMR only.
pub(crate) fn history_due(cache: &Cache, settings: &Settings, now: u64) -> bool {
    settings.code().is_some() && cache.history(settings).is_none_or(|h| now.saturating_sub(h.fetched) >= HISTORY_EVERY)
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
        let f = amount_field("20", true, FieldPrice::At(&usd, p, None));
        assert_eq!((f.sign.as_deref(), f.unit.as_str(), f.in_fiat, f.can_switch), (Some("$"), "USD", true, true));
        assert_eq!(f.line.as_deref(), Some("≈ 0.1204 XMR · 1 XMR = $166.10"));
        assert_eq!((f.label.as_str(), f.switch_label.as_deref()), ("Amount in US dollars", Some("Amount in US dollars. Switch to XMR")));
        // In XMR: the sign becomes ɱ.
        let f = amount_field("0.1204", false, FieldPrice::At(&usd, p, None));
        assert_eq!((f.sign.as_deref(), f.unit.as_str(), f.in_fiat), (Some("ɱ"), "XMR", false));
        assert_eq!(f.line.as_deref(), Some("≈ $20.00 · 1 XMR = $166.10"));
        assert_eq!((f.label.as_str(), f.switch_label.as_deref()), ("Amount in XMR", Some("Amount in XMR. Switch to US dollars")));
        // Empty: the rate alone. Not an amount: says so.
        assert_eq!(amount_field(" ", true, FieldPrice::At(&usd, p, None)).line.as_deref(), Some("1 XMR = $166.10"));
        assert_eq!(amount_field("20.123", true, FieldPrice::At(&usd, p, None)).line.as_deref(), Some("That isn't an amount"));
        assert_eq!(amount_field("abc", false, FieldPrice::At(&usd, p, None)).line.as_deref(), Some("That isn't an amount"));
        // Euros, and a currency without a symbol: the chip names it, no sign.
        let f = amount_field("5", true, FieldPrice::At(&eur, p, None));
        assert_eq!((f.sign.as_deref(), f.unit.as_str(), f.label.as_str()), (Some("€"), "EUR", "Amount in euros"));
        let f = amount_field("5", true, FieldPrice::At(&chf, p, None));
        assert_eq!((f.sign, f.unit.as_str(), f.label.as_str()), (None, "CHF", "Amount in Swiss francs"));
        assert_eq!(f.line.as_deref(), Some("≈ 0.0301 XMR · 1 XMR = 166.10 CHF"));
        // No price: XMR only, no switch, nothing under it unless it's wrong.
        let f = amount_field("20", true, FieldPrice::None);
        assert_eq!((f.sign.as_deref(), f.unit.as_str(), f.in_fiat, f.can_switch, f.line, f.switch_label), (Some("ɱ"), "XMR", false, false, None, None));
        assert_eq!(amount_field("x", false, FieldPrice::None).line.as_deref(), Some("That isn't an amount"));
        // A currency chosen without a price to use: XMR only, saying why.
        let f = amount_field("", true, FieldPrice::Missing(&usd));
        assert_eq!((f.unit.as_str(), f.can_switch, f.line.as_deref()), ("XMR", false, Some("No USD price now · type the amount in XMR")));
        assert_eq!(amount_field("0.5", false, FieldPrice::Missing(&usd)).line.as_deref(), Some("No USD price now · type the amount in XMR"));
        assert_eq!(amount_field("x", false, FieldPrice::Missing(&usd)).line.as_deref(), Some("That isn't an amount"));
        // An aged price says when it's from, in place of the rate once there's an amount.
        let aged = FieldPrice::At(&usd, p, Some("price from 14:02"));
        assert_eq!(amount_field("", true, aged).line.as_deref(), Some("1 XMR = $166.10 · price from 14:02"));
        assert_eq!(amount_field("20", true, aged).line.as_deref(), Some("≈ 0.1204 XMR · price from 14:02"));
        assert_eq!(amount_field("0.1204", false, aged).line.as_deref(), Some("≈ $20.00 · price from 14:02"));
        assert!(amount_field("20", true, aged).can_switch);
    }

    #[test]
    fn settings_name_a_currency_or_give_its_code() {
        assert_eq!(usd().text(), "US dollar");
        assert_eq!(currency("XYZ").text(), "XYZ");
        assert_eq!((currency("CHF").sign(), currency("AUD").sign()), ("CHF".to_owned(), "A$".to_owned()));
        assert_eq!(shown_in_text(&Settings::default()), "US dollar");
        assert_eq!(shown_in_text(&Settings { currency: None, ..Settings::default() }), "XMR only");
    }

    #[test]
    fn the_amount_field_says_when_send_can_review_it() {
        let usd = usd();
        let p = 166_100_000;
        let sendable = |text: &str, in_fiat: bool, price: FieldPrice| amount_field(text, in_fiat, price).sendable;
        assert!(sendable("20", true, FieldPrice::At(&usd, p, None)));
        assert!(sendable("0.1204", false, FieldPrice::At(&usd, p, None)));
        // Typed in money without a price, it's read as XMR.
        assert!(sendable("20", true, FieldPrice::None));
        // It says what it was asked about, as asked.
        let f = amount_field(" 20", true, FieldPrice::None);
        assert_eq!((f.for_text.as_str(), f.for_in_fiat, f.in_fiat), (" 20", true, false));
        for (text, in_fiat) in [("", true), (" ", false), ("0", false), ("0.00", true), ("20.123", true), ("abc", false)] {
            assert!(!sendable(text, in_fiat, FieldPrice::At(&usd, p, None)), "{text:?}");
        }
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
        let eur = Settings { currency: Some("EUR".into()), ..settings.clone() };
        assert_eq!(cache.value_at(&eur, now), None);
        // Shown for a day, recorded with a payment for an hour.
        assert!(cache.shown(&settings, now + SHOWN - 1).is_some());
        assert!(cache.shown(&settings, now + SHOWN).is_none());
        assert!(cache.last(&settings).is_some());
        assert!(cache.value_at(&settings, now + FRESH).is_some_and(|r| r.daily));
        assert!(cache.shown(&eur, now).is_none() && cache.last(&eur).is_none());
    }

    #[test]
    fn history_is_bounded() {
        let mut cache = Cache::default();
        let days: Vec<(i64, u64)> = (0..HISTORY_DAYS as i64 + 50).map(|d| (d, 1)).collect();
        for (i, code) in ["USD", "EUR", "GBP", "CHF"].into_iter().enumerate() {
            let settings = Settings { currency: Some(code.into()), ..Settings::default() };
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
        assert!(current_due(&cache, &Settings { currency: Some("EUR".into()), ..settings.clone() }, now, false));
        // XMR only: never.
        let xmr = Settings { currency: None, ..settings.clone() };
        assert!(!current_due(&Cache::default(), &xmr, now, true));
        assert!(!history_due(&Cache::default(), &xmr, now));
        assert!(history_due(&cache, &settings, now));
        store_history(&mut cache, &settings, vec![], now);
        assert!(!history_due(&cache, &settings, now + HISTORY_EVERY - 1));
        assert!(history_due(&cache, &settings, now + HISTORY_EVERY));
    }

    #[test]
    fn whole_yen_and_thousandths_of_a_dinar_work_like_cents() {
        let (jpy, kwd) = (currency("JPY"), currency("KWD"));
        // 1 XMR at ¥93,477.40: whole yen, rounded half up.
        let p = 93_477_400_000;
        assert_eq!(to_minor(XMR, p, &jpy), 93_477);
        assert_eq!(value(XMR / 2, p, &jpy), "¥46,739");
        assert_eq!(rate(p, &jpy), "1 XMR = ¥93,477");
        assert_eq!((format(0, &jpy), plain(1_234, &jpy)), ("¥0".to_owned(), "1234".to_owned()));
        assert_eq!(parse("¥1500", &jpy).unwrap(), 1_500);
        assert_eq!(parse("1500 jpy", &jpy).unwrap(), 1_500);
        assert!(parse("1500.5", &jpy).is_err(), "no decimals in yen");
        let atomic = to_atomic(1_500, p, &jpy).unwrap();
        assert_eq!((amount::format(atomic), to_minor(atomic, p, &jpy)), ("0.01605".to_owned(), 1_500));
        // 1 XMR at 56.123 KWD: three decimals, no symbol.
        let p = 56_123_000;
        assert_eq!(to_minor(XMR, p, &kwd), 56_123);
        assert_eq!(value(XMR, p, &kwd), "56.123 KWD");
        assert_eq!(plain(1_050, &kwd), "1.050");
        assert_eq!(parse("1.05", &kwd).unwrap(), 1_050);
        assert_eq!(parse("1,005 KWD", &kwd).unwrap(), 1_005);
        assert!(parse("1.0005", &kwd).is_err());
        for minor in [1, 999, 1_005, 56_123] {
            let atomic = to_atomic(minor, p, &kwd).unwrap();
            assert_eq!(to_minor(atomic, p, &kwd), minor, "{minor}");
        }
        // The amount field takes them as typed.
        let f = amount_field("1500", true, FieldPrice::At(&jpy, 93_477_400_000, None));
        assert_eq!((f.sign.as_deref(), f.unit.as_str(), f.label.as_str()), (Some("¥"), "JPY", "Amount in Japanese yen"));
        assert_eq!(f.line.as_deref(), Some("≈ 0.01605 XMR · 1 XMR = ¥93,477"));
        assert_eq!(amount_field("1500.5", true, FieldPrice::At(&jpy, 93_477_400_000, None)).line.as_deref(), Some("That isn't an amount"));
        let f = amount_field("1.5", true, FieldPrice::At(&kwd, p, None));
        assert_eq!((f.sign, f.label.as_str(), f.line.as_deref()), (None, "Amount in Kuwaiti dinars", Some("≈ 0.02673 XMR · 1 XMR = 56.123 KWD")));
        assert!(f.sendable);
    }

    #[test]
    fn a_wallet_file_from_before_prices_gets_the_defaults() {
        let settings: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(settings, Settings::default());
        assert_eq!(settings.code(), Some("USD"), "dollars unless XMR only is chosen");
        assert_eq!(settings.source_text(), "Kraken · web");
        let coingecko_tor = Settings { provider: Provider::CoinGecko, transport: Transport::Tor, ..Settings::default() };
        assert_eq!(coingecko_tor.source_text(), "CoinGecko · Tor", "the Ask over row's name");
        let cache: Cache = serde_json::from_str("{}").unwrap();
        assert_eq!(cache, Cache::default());
    }

    #[test]
    fn a_wallet_file_from_before_currencies_opens_as_it_was() {
        let read = |json: &str| serde_json::from_str::<Settings>(json).unwrap();
        // Prices off: XMR only, whatever currency it had.
        let off = read(r#"{"on":false,"currency":"EUR","provider":"kraken","transport":"web","fiat_first":true}"#);
        assert_eq!(off, Settings { currency: None, provider: Provider::Kraken, transport: Transport::Web, fiat_first: true });
        // On, in dollars or euros: that currency, from Kraken.
        let on = read(r#"{"on":true,"currency":"EUR","provider":"kraken","transport":"web","fiat_first":false}"#);
        assert_eq!((on.code(), on.provider), (Some("EUR"), Provider::Kraken));
        assert_eq!(read(r#"{"on":true,"currency":"USD"}"#).code(), Some("USD"));
        // Written back the new way, without the switch, and read the same.
        let json = serde_json::to_string(&off).unwrap();
        assert_eq!(json, r#"{"currency":null,"provider":"kraken","transport":"web","fiat_first":true}"#);
        assert_eq!(read(&json), off);
        let json = serde_json::to_string(&on).unwrap();
        assert!(!json.contains("\"on\""), "{json}");
        assert_eq!(read(&json), on);
        // What no longer fits moves as choosing would.
        assert_eq!(read(r#"{"currency":"NZD","provider":"kraken"}"#).provider, Provider::CoinGecko);
        assert_eq!(read(r#"{"currency":"XYZ"}"#).code(), None);
        assert_eq!(read(r#"{"currency":"USD","provider":"coingecko"}"#).provider, Provider::CoinGecko);
    }

    #[test]
    fn ask_over_says_why_tor_is_greyed() {
        let mut s = Settings::default();
        let rows = transport_rows(&s);
        assert_eq!(rows.iter().map(|r| (r.title, r.available, r.chosen)).collect::<Vec<_>>(), [("Web", true, true), ("Tor", false, false)]);
        assert_eq!(rows[0].line, "The provider sees this phone's IP address");
        assert_eq!(rows[1].line, "Kraken doesn't answer over Tor");
        // CoinGecko answers over Tor, but Fob hasn't Tor built in: still
        // greyed, saying so.
        s.choose_provider(Provider::CoinGecko).unwrap();
        let tor = &transport_rows(&s)[1];
        assert_eq!((tor.line.as_str(), tor.available), ("Hides this phone's IP address · not built into Fob yet", false));
        assert_eq!(shown_in_line(&s), "Uses today's XMR price");
        s.choose_currency(None).unwrap();
        assert_eq!(shown_in_line(&s), "Fob never looks up a price");
    }

    #[test]
    fn choosing_a_currency_moves_to_a_provider_that_prices_it() {
        let mut s = Settings::default();
        assert_eq!(s.choose_currency(Some("AUD")).unwrap(), None, "Kraken prices it");
        assert_eq!(s.provider, Provider::Kraken);
        assert_eq!(s.choose_currency(Some("NZD")).unwrap().as_deref(), Some("Kraken doesn't price New Zealand dollars, so Fob now asks CoinGecko"));
        assert_eq!((s.code(), s.provider), (Some("NZD"), Provider::CoinGecko));
        // CoinGecko prices all 45, so it stays.
        assert_eq!(s.choose_currency(Some("USD")).unwrap(), None);
        assert_eq!(s.provider, Provider::CoinGecko);
        // XMR only keeps the provider for later.
        assert_eq!(s.choose_currency(None).unwrap(), None);
        assert_eq!((s.code(), s.provider), (None, Provider::CoinGecko));
        assert!(s.choose_currency(Some("XYZ")).is_err());
        assert_eq!(s.code(), None, "unchanged by a refusal");
    }

    #[test]
    fn choosing_a_provider_without_tor_moves_to_the_web() {
        // Tor isn't built, so only a file can hold it: as it would be chosen.
        let mut s = Settings { currency: Some("EUR".into()), provider: Provider::CoinGecko, transport: Transport::Tor, fiat_first: false };
        assert_eq!(s.choose_provider(Provider::Kraken).unwrap().as_deref(), Some("Kraken doesn't answer over Tor, so Fob now asks over the web"));
        assert_eq!((s.provider, s.transport), (Provider::Kraken, Transport::Web));
        assert_eq!(s.choose_provider(Provider::CoinGecko).unwrap(), None);
        // One that doesn't price the currency is refused.
        s.choose_currency(Some("NZD")).unwrap();
        assert_eq!(s.choose_provider(Provider::Kraken).unwrap_err().to_string(), "Kraken doesn't price New Zealand dollars");
        assert_eq!(s.provider, Provider::CoinGecko);
        // With XMR only, any.
        s.choose_currency(None).unwrap();
        assert_eq!(s.choose_provider(Provider::Kraken).unwrap(), None);
    }

    #[test]
    fn tor_is_refused_until_built_and_for_a_provider_without_it() {
        let mut s = Settings::default();
        assert_eq!(s.choose_transport(Transport::Tor).unwrap_err().to_string(), "prices over Tor aren't available yet");
        s.choose_transport(Transport::Web).unwrap();
        assert!(Provider::CoinGecko.answers_over(Transport::Tor) && !Provider::Kraken.answers_over(Transport::Tor));
        assert!(Provider::ALL.iter().all(|p| p.answers_over(Transport::Web)));
    }

    #[test]
    fn each_currency_says_who_prices_it() {
        assert_eq!(providers_for("AUD"), [Provider::Kraken, Provider::CoinGecko]);
        assert_eq!(providers_for("NZD"), [Provider::CoinGecko]);
        assert!(providers_for("XYZ").is_empty());
        // Every currency has a provider, and every provider's are in the table.
        assert!(currencies::CODES.iter().all(|c| !providers_for(c).is_empty()));
        assert!(Provider::ALL.iter().flat_map(|p| p.currencies()).all(|c| currencies::find(c).is_some()));
        assert_eq!(Provider::named("CoinGecko"), Some(Provider::CoinGecko));
        assert_eq!(Provider::named("Bitstamp"), None);
    }

    #[test]
    fn the_currency_list_puts_xmr_only_and_the_phone_s_first() {
        let list = currency_list(Some("AUD"), Some("aud"), "");
        let titles: Vec<&str> = list.groups.iter().map(|g| g.title.as_str()).collect();
        assert_eq!(titles, ["NONE", "THIS PHONE’S", "ALL 45"]);
        let xmr = &list.groups[0].rows[0];
        assert_eq!((xmr.code.as_deref(), xmr.sign.as_str(), xmr.name.as_str(), xmr.line.as_str(), xmr.chosen), (None, "ɱ", "XMR only", "Fob never looks up a price", false));
        let phone = &list.groups[1].rows[0];
        assert_eq!((phone.sign.as_str(), phone.name.as_str(), phone.line.as_str(), phone.chosen), ("A$", "Australian dollar", "AUD · Kraken, CoinGecko", true));
        let all = &list.groups[2].rows;
        assert_eq!(all.len(), 45);
        assert_eq!(all[0].name, "Argentine peso");
        assert_eq!(all[0].line, "ARS · CoinGecko");
        assert_eq!(all.iter().filter(|r| r.chosen).count(), 1);
        // A phone currency Fob doesn't have, or none: no such row.
        assert_eq!(currency_list(None, Some("XOF"), "").groups.len(), 2);
        assert!(currency_list(None, None, "").groups[0].rows[0].chosen, "XMR only chosen");
        // Searching: matches only, no phone row.
        let found = currency_list(Some("AUD"), Some("AUD"), "dollar");
        assert_eq!(found.groups.len(), 1);
        assert_eq!(found.groups[0].title, format!("{} OF 45", found.groups[0].rows.len()));
        assert!(found.groups[0].rows.iter().all(|r| r.name.contains("dollar")));
        let zloty = currency_list(None, None, "ZLOTY");
        assert_eq!(zloty.groups[0].rows.iter().map(|r| r.code.as_deref()).collect::<Vec<_>>(), [Some("PLN")]);
        assert_eq!(currency_list(None, None, "xmr").groups[0].title, "NONE");
        let none = currency_list(None, None, " qqq ");
        assert_eq!((none.groups.len(), none.empty.as_deref()), (0, Some("No currency matches “qqq”")));
    }

    /// A provider for checking: an answer after a wait, or a failure.
    #[derive(Clone)]
    struct Fake {
        name: &'static str,
        tor: bool,
        answer: std::result::Result<u64, Failure>,
        wait: Duration,
        asked: std::sync::Arc<std::sync::atomic::AtomicU64>,
    }

    impl Fake {
        fn new(name: &'static str, tor: bool, answer: std::result::Result<u64, Failure>) -> Fake {
            Fake { name, tor, answer, wait: Duration::from_millis(10), asked: Default::default() }
        }
    }

    impl Source for Fake {
        fn name(&self) -> &'static str {
            self.name
        }

        fn currencies(&self) -> &'static [&'static str] {
            &["AUD"]
        }

        fn answers_over(&self, transport: Transport) -> bool {
            transport == Transport::Web || self.tor
        }

        async fn current(&self, _: &str, _: Transport) -> Result<u64> {
            self.asked.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            tokio::time::sleep(self.wait).await;
            self.answer.map_err(FobError::PriceFailed)
        }

        async fn daily(&self, _: &str, _: Range<i64>, _: Transport) -> Result<Vec<(i64, u64)>> {
            Ok(vec![])
        }

        fn history_days(&self) -> i64 {
            1
        }
    }

    #[tokio::test(start_paused = true)]
    async fn price_from_checks_every_provider_at_once() {
        let aud = currency("AUD");
        let slow = Fake { wait: Duration::from_secs(60), ..Fake::new("Slow", true, Ok(1)) };
        let sources = vec![Fake::new("Kraken", false, Ok(845_100_000)), Fake::new("CoinGecko", true, Ok(845_020_000)), Fake::new("Busy", true, Err(Failure::Busy)), slow];
        let started = tokio::time::Instant::now();
        let lines = check(sources, &aud, Transport::Web, "Kraken", CHECK_TIMEOUT).await;
        assert_eq!(started.elapsed(), CHECK_TIMEOUT, "all at once, the slow one cut off");
        type Line<'a> = (&'a str, Option<&'a str>, Option<&'a str>, bool, bool);
        let view: Vec<Line> = lines.iter().map(|l| (l.name, l.rate.as_deref(), l.failure.as_deref(), l.chosen, l.can_pick)).collect();
        assert_eq!(
            view,
            [
                ("Kraken", Some("A$845.10"), None, true, true),
                ("CoinGecko", Some("A$845.02"), None, false, true),
                ("Busy", None, Some("Busy, try again soon"), false, false),
                ("Slow", None, Some("Not answering"), false, false),
            ]
        );
        assert_eq!((lines[0].web, lines[0].tor, lines[1].tor), (true, false, true));
        assert_eq!(lines[0].label, "Kraken, web only, A$845.10");
        assert_eq!(lines[3].label, "Slow, web and Tor, not answering");
    }

    #[tokio::test(start_paused = true)]
    async fn over_tor_a_provider_without_it_is_never_asked() {
        let kraken = Fake::new("Kraken", false, Ok(845_100_000));
        let asked = kraken.asked.clone();
        let lines = check(vec![kraken, Fake::new("CoinGecko", true, Ok(845_020_000))], &currency("AUD"), Transport::Tor, "CoinGecko", CHECK_TIMEOUT).await;
        assert_eq!(asked.load(std::sync::atomic::Ordering::Relaxed), 0, "no request over the web while Tor is chosen");
        assert_eq!((lines[0].rate.as_deref(), lines[0].failure.as_deref(), lines[0].can_pick), (None, None, true));
        assert_eq!(lines[0].label, "Kraken, web only, not asked over Tor");
        assert_eq!(lines[1].rate.as_deref(), Some("A$845.02"));
    }

    #[test]
    fn json_numbers_become_millionths() {
        let n = |text: &str| micro_number(&serde_json::from_str(text).unwrap());
        assert_eq!(n("844.02"), Some(844_020_000));
        assert_eq!(n("93477"), Some(93_477_000_000));
        assert_eq!(n("51561.32217818517"), Some(51_561_322_178));
        assert_eq!(n("1.5e-7"), Some(0));
        assert_eq!(n("2.5e-6"), Some(3));
        assert_eq!(n("1e3"), Some(1_000_000_000));
        assert_eq!(n("-1"), None);
        assert_eq!(n("\"1\""), None);
    }

    #[tokio::test]
    async fn tor_says_it_isnt_there_yet_and_onions_are_refused() {
        let tor = Transport::Tor.get("https://api.kraken.com/0/public/Time").await.unwrap_err();
        assert_eq!(tor.to_string(), "Tor isn't built yet");
        let onion = Transport::Web.get("https://pricesxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx.onion/x").await.unwrap_err();
        assert!(onion.to_string().contains(".onion"), "{onion}");
        let plain = Transport::Web.get("http://api.kraken.com/0/public/Time").await.unwrap_err();
        assert!(plain.to_string().contains("https"), "{plain}");
    }
}
