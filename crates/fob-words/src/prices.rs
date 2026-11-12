//! Prices, in words.

use fob_monero::{amount::Unit, fiat::PriceAge};
use fob_price::{AmountField, Check, CurrencyHeading, CurrencyLine, CurrencyList, Failure, Moved, PriceFrom, PriceFromIntro, Settings, Source, Transport, TransportNote, UnderField};

use crate::{Device, device};

/// "over the internet", "over Tor".
pub fn over(transport: Transport) -> &'static str {
    match transport {
        Transport::Web => "over the internet",
        Transport::Tor => "over Tor",
    }
}

/// "Web", "Tor": its title on Price from's Ask over rows.
pub fn transport_name(transport: Transport) -> &'static str {
    match transport {
        Transport::Web => "Web",
        Transport::Tor => "Tor",
    }
}

/// Why a provider gave no price, for its line on Price from.
pub fn failure(failure: Failure) -> &'static str {
    match failure {
        Failure::NotAnswering => "Not answering",
        Failure::Busy => "Busy, try again soon",
        Failure::BadAnswer => "Answered without a price",
        Failure::TorNotYet => "Tor isn't built yet",
        Failure::NotOverTor => "Doesn't answer over Tor",
    }
}

/// "Kraken · web", "CoinGecko · Tor": the provider and the Ask over row
/// picked, by that row's name.
pub fn source_text(settings: &Settings) -> String {
    let transport = match settings.transport {
        Transport::Web => "web",
        Transport::Tor => transport_name(settings.transport),
    };
    format!("{} · {transport}", settings.provider.name())
}

/// What a choice moved: "Kraken doesn't price New Zealand dollars, so Fob now
/// asks CoinGecko", "Kraken doesn't answer over Tor, so Fob now asks over the web".
pub fn moved(moved: Moved) -> String {
    match moved {
        Moved::NotPriced { from, currency, to, to_web } => {
            let over_web = if to_web { " over the web" } else { "" };
            format!("{from} doesn't price {currency}, so Fob now asks {to}{over_web}")
        }
        Moved::NotOverTransport { provider, transport } => format!("{provider} doesn't answer {}, so Fob now asks over the web", over(transport)),
    }
}

const NEVER_LOOKS: &str = "Fob never looks up a price";

/// Price from's words above its providers: the intro, its currency's plural
/// in bold between `before` and `after` ("Prices in **Australian dollars**,
/// checked as this page opens. Tap a provider to pick it."), or with XMR only
/// the one sentence in `before`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Intro {
    pub before: String,
    pub bold: Option<String>,
    pub after: String,
}

pub fn intro(intro: PriceFromIntro) -> Intro {
    match intro {
        PriceFromIntro::In(plural) => Intro { before: "Prices in ".into(), bold: Some(plural), after: format!(", checked as this page opens. {} a provider to pick it.", crate::capitalised(crate::device().press())) },
        PriceFromIntro::XmrOnly => Intro { before: format!("With XMR as the default currency, {NEVER_LOOKS}."), bold: None, after: String::new() },
    }
}

/// What TalkBack says once Price from's checks are in: "Kraken A$845.10,
/// CoinGecko not answering".
pub fn checks_summary(checks: &[Check]) -> String {
    checks
        .iter()
        .map(|c| match (&c.rate, c.failure) {
            (Some(rate), _) => format!("{} {rate}", c.name),
            (None, Some(why)) => format!("{} {}", c.name, failure(why).to_lowercase()),
            (None, None) => format!("{} not asked", c.name),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// A provider's line for TalkBack: "Kraken, web only, A$845.10".
pub fn check_label(c: &Check) -> String {
    let ways = match (c.web, c.tor) {
        (true, true) => "web and Tor",
        (true, false) => "web only",
        (false, true) => "Tor only",
        (false, false) => "no way to ask",
    };
    let said = c.rate.clone().or_else(|| c.failure.map(|f| failure(f).to_lowercase())).unwrap_or_else(|| format!("not asked {}", over(c.transport)));
    format!("{}, {ways}, {said}", c.name)
}

/// "AUD · Kraken, CoinGecko", "Fob never looks up a price".
pub fn currency_line(line: &CurrencyLine) -> String {
    match line {
        CurrencyLine::NeverLooksUp => NEVER_LOOKS.into(),
        CurrencyLine::PricedBy { code, providers } => format!("{code} · {}", providers.join(", ")),
    }
}

/// Instead of the list when a search finds nothing: "No currency matches “xyz”".
pub fn no_match(list: &CurrencyList) -> Option<String> {
    list.no_match.as_ref().map(|query| format!("No currency matches “{query}”"))
}

/// What's under an amount field, and for TalkBack.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    pub typed_in: Unit,
    pub sign: Option<String>,
    pub unit: String,
    pub can_switch: bool,
    /// Under the field: "≈ 0.1204 XMR · 1 XMR = $166.10", "≈ $20.00 · 1
    /// XMR = $166.10", the rate alone while it's empty, "That isn't an
    /// amount"; nothing in XMR without a price.
    pub line: Option<String>,
    /// The line without the rate: "≈ 0.1204 XMR", "That isn't an amount", or
    /// nothing while it's empty. For a field with no room for the rate.
    pub conversion: Option<String>,
    /// For TalkBack: "Amount in US dollars", "Amount in XMR".
    pub label: String,
    /// The chip for TalkBack: "Amount in XMR. Switch to US dollars".
    pub switch_label: Option<String>,
    pub sendable: bool,
    pub for_text: String,
    pub for_typed_in: Unit,
}

/// "price from 14:02", "price from yesterday 14:02", "price from 29 Oct".
pub fn from_when(from: &PriceFrom) -> String {
    match from {
        PriceFrom::Today(time) => format!("price from {time}"),
        PriceFrom::Yesterday(time) => format!("price from yesterday {time}"),
        PriceFrom::Date(date) => format!("price from {date}"),
    }
}

pub fn field(f: AmountField) -> Field {
    let no_price = |code: &str| format!("No {code} price now · type the amount in XMR");
    let (line, conversion) = match &f.under {
        UnderField::Nothing => (None, None),
        UnderField::Rate { rate, from } => (Some(from.as_ref().map_or_else(|| rate.clone(), |from| format!("{rate} · {}", from_when(from)))), None),
        UnderField::Converts { to, rate, from } => match from {
            // Once the price is aged, when it's from in the rate's place, so the line stays short.
            Some(from) => (Some(format!("≈ {to} · {}", from_when(from))), Some(format!("≈ {to} · {}", from_when(from)))),
            None => (Some(format!("≈ {to} · {rate}")), Some(format!("≈ {to}"))),
        },
        UnderField::NoPrice { code } => (Some(no_price(code)), Some(no_price(code))),
        UnderField::NotAnAmount => (Some("That isn't an amount".to_owned()), Some("That isn't an amount".to_owned())),
    };
    Field {
        typed_in: f.typed_in,
        sign: f.sign,
        unit: f.unit,
        can_switch: f.can_switch,
        line,
        conversion,
        label: format!("Amount in {}", f.unit_name),
        switch_label: f.other_unit_name.map(|other| format!("Amount in {}. Switch to {other}", f.unit_name)),
        sendable: f.sendable,
        for_text: f.for_text,
        for_typed_in: f.for_typed_in,
    }
}

/// Who failed and how: "Kraken didn't answer", "CoinGecko is busy".
pub fn reason(failure: Failure, provider: &str) -> String {
    match failure {
        Failure::NotAnswering => format!("{provider} didn't answer"),
        Failure::Busy => format!("{provider} is busy"),
        Failure::BadAnswer => format!("{provider} answered without a price"),
        Failure::TorNotYet => "Tor isn't built yet".into(),
        Failure::NotOverTor => format!("{provider} doesn't answer over Tor"),
    }
}

/// The price kept, when a lookup fails.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PriceWas {
    /// New enough to use, from this time.
    Using(PriceFrom),
    /// One too old to use.
    TooOld,
    /// None ever kept.
    NeverKept,
}

/// The toast's words.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Toast {
    /// "Couldn't get the USD price", "Couldn't get 29 Oct's USD price".
    pub title: String,
    /// "Kraken didn't answer · using the 14:02 price", "CoinGecko didn't
    /// answer · no NZD price yet", "CoinGecko is busy · shown as not
    /// available".
    pub line: String,
    /// While Retry asks: "Trying again…", "Asking Kraken for the USD price".
    pub retrying_title: String,
    pub retrying_line: String,
}

/// "the 14:02 price", "yesterday's 14:02 price", "29 Oct's price".
fn the_price(from: &PriceFrom) -> String {
    match from {
        PriceFrom::Today(time) => format!("the {time} price"),
        PriceFrom::Yesterday(time) => format!("yesterday's {time} price"),
        PriceFrom::Date(date) => format!("{date}'s price"),
    }
}

/// The toast for a failed lookup: of `day`'s price when it was only the
/// history that failed, of a money amount typed when `typing`.
pub fn toast(failure: Failure, provider: &str, currency: &str, day: Option<&str>, typing: bool, was: &PriceWas) -> Toast {
    // "the USD price", "29 Oct's USD price".
    let what = match day {
        Some(day) => format!("{day}'s {currency} price"),
        None => format!("the {currency} price"),
    };
    let reason = reason(failure, provider);
    let xmr = if typing { "type the amount in XMR" } else { "amounts stay in XMR" };
    let line = match was {
        _ if day.is_some() => format!("{reason} · shown as not available"),
        PriceWas::Using(from) => format!("{reason} · using {}", the_price(from)),
        // One too old to use: the reason, and what that means.
        PriceWas::TooOld => format!("{reason} · {xmr}"),
        // None ever kept: still why, and that there's none yet.
        PriceWas::NeverKept if typing => format!("{reason} · {xmr}"),
        PriceWas::NeverKept => format!("{reason} · no {currency} price yet"),
    };
    Toast { title: format!("Couldn't get {what}"), line, retrying_title: "Trying again…".into(), retrying_line: format!("Asking {provider} for {what}") }
}

/// Retry's toast with XMR only.
pub fn prices_off() -> String {
    "Prices are off".into()
}

/// Retry's toast when it worked: "USD price updated".
pub fn updated(code: &str) -> String {
    format!("{code} price updated")
}

/// Retry's toast when it found one day's price: "29 Oct's USD price found".
pub fn day_found(day: &str, code: &str) -> String {
    format!("{day}'s {code} price found")
}

/// Beside an amount converted at an old price: "40 min old", "2 h old".
pub fn age(age: PriceAge) -> String {
    match age {
        PriceAge::Minutes(minutes) => format!("{minutes} min old"),
        PriceAge::Hours(hours) => format!("{hours} h old"),
    }
}

/// Under Send's sheet: "1.8592 XMR ready to spend · $308.70", the money only
/// with a price from within the day, and its age once it's missed a refresh
/// ("… · $308.70 · 2 h old").
pub fn ready_line(amount: &str, worth: Option<(String, Option<PriceAge>)>) -> String {
    let xmr = format!("{amount} XMR ready to spend");
    match worth {
        Some((worth, Some(old))) => format!("{xmr} · {worth} · {}", age(old)),
        Some((worth, None)) => format!("{xmr} · {worth}"),
        None => xmr,
    }
}

/// What choosing a way of asking for prices means.
pub fn transport_note(note: TransportNote) -> String {
    transport_note_on(device(), note)
}

fn transport_note_on(device: Device, note: TransportNote) -> String {
    let device = device.noun();
    match note {
        TransportNote::SeesAddress => format!("The provider sees this {device}'s IP address"),
        TransportNote::NotAnswered { provider, transport } => format!("{provider} doesn't answer {}", over(transport)),
        TransportNote::NotBuilt => format!("Hides this {device}'s IP address · not built into Fob yet"),
        TransportNote::HidesAddress => format!("Hides this {device}'s IP address; slower"),
    }
}

/// A part of the "Default currency" list: "MONERO", "THIS PHONE'S", "ALL 45".
pub fn currency_heading(heading: CurrencyHeading) -> String {
    currency_heading_on(device(), heading)
}

fn currency_heading_on(device: Device, heading: CurrencyHeading) -> String {
    match heading {
        CurrencyHeading::Monero => "MONERO".into(),
        CurrencyHeading::ThisDevice => format!("THIS {}’S", device.noun().to_uppercase()),
        CurrencyHeading::All { count } => format!("ALL {count}"),
        CurrencyHeading::Found { count, of } => format!("{count} OF {of}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fob_monero::{amount::Typed::{self, Fiat, Xmr}, fiat::{Currency, currency}};
    use fob_price::{Cache, FieldPrice, Provider, currency_list, error::PriceError, transport_rows};

    fn amount_field(typed: Typed, price: FieldPrice) -> Field {
        field(fob_price::amount_field(typed, price))
    }

    fn price_from_intro(s: &Settings) -> Intro {
        intro(fob_price::price_from_intro(s))
    }

    fn blank_field(price: FieldPrice) -> Field {
        field(fob_price::blank_field(price))
    }


    fn usd() -> Currency {
        currency("USD")
    }

    fn eur() -> Currency {
        currency("EUR")
    }

    /// A provider for checking: an answer after a wait, or a failure.
    #[test]
    fn price_from_says_what_each_provider_answered() {
        let line = |name, web, tor, rate: Option<&str>, failure, transport| Check { name, web, tor, chosen: false, rate: rate.map(str::to_owned), failure, can_pick: failure.is_none(), transport };
        let lines = [
            line("Kraken", true, false, Some("A$845.10"), None, Transport::Web),
            line("CoinGecko", true, true, Some("A$845.02"), None, Transport::Web),
            line("Busy", true, true, None, Some(Failure::Busy), Transport::Web),
            line("Slow", true, true, None, Some(Failure::NotAnswering), Transport::Web),
        ];
        assert_eq!(check_label(&lines[0]), "Kraken, web only, A$845.10");
        assert_eq!(check_label(&lines[3]), "Slow, web and Tor, not answering");
        // Said once they're in, for TalkBack.
        assert_eq!(checks_summary(&lines), "Kraken A$845.10, CoinGecko A$845.02, Busy busy, try again soon, Slow not answering");
        assert_eq!(check_label(&line("Kraken", true, false, None, None, Transport::Tor)), "Kraken, web only, not asked over Tor");
        assert_eq!(failure(Failure::BadAnswer), "Answered without a price");
        assert_eq!(failure(Failure::TorNotYet), "Tor isn't built yet");
        assert_eq!(failure(Failure::NotOverTor), "Doesn't answer over Tor");
    }

    #[test]
    fn prices_say_what_they_always_said_on_a_phone() {
        let on = |note| transport_note_on(Device::Phone, note);
        assert_eq!(on(TransportNote::SeesAddress), "The provider sees this phone's IP address");
        assert_eq!(on(TransportNote::NotAnswered { provider: "Kraken", transport: Transport::Tor }), "Kraken doesn't answer over Tor");
        assert_eq!(on(TransportNote::NotBuilt), "Hides this phone's IP address · not built into Fob yet");
        assert_eq!(on(TransportNote::HidesAddress), "Hides this phone's IP address; slower");
        let heading = |h| currency_heading_on(Device::Phone, h);
        assert_eq!(heading(CurrencyHeading::Monero), "MONERO");
        assert_eq!(heading(CurrencyHeading::ThisDevice), "THIS PHONE’S");
        assert_eq!(heading(CurrencyHeading::All { count: 45 }), "ALL 45");
        assert_eq!(heading(CurrencyHeading::Found { count: 3, of: 45 }), "3 OF 45");
    }

    #[test]
    fn on_a_computer_they_name_the_computer() {
        assert_eq!(transport_note_on(Device::Computer, TransportNote::SeesAddress), "The provider sees this computer's IP address");
        assert_eq!(currency_heading_on(Device::Computer, CurrencyHeading::ThisDevice), "THIS COMPUTER’S");
    }

    #[test]
    fn the_amount_field_reads_like_toggle_b() {
        let (usd, eur, chf) = (usd(), eur(), currency("CHF"));
        let p = 166_100_000;
        // In dollars, as the board: "$ 20 [USD ⇅]", "≈ 0.1204 XMR · 1 XMR = $166.10".
        let f = amount_field(Fiat("20"), FieldPrice::At(&usd, p, None));
        assert_eq!((f.sign.as_deref(), f.unit.as_str(), f.typed_in, f.can_switch), (Some("$"), "USD", Unit::Fiat, true));
        assert_eq!(f.line.as_deref(), Some("≈ 0.1204 XMR · 1 XMR = $166.10"));
        assert_eq!((f.label.as_str(), f.switch_label.as_deref()), ("Amount in US dollars", Some("Amount in US dollars. Switch to XMR")));
        // In XMR: the sign becomes ɱ.
        let f = amount_field(Xmr("0.1204"), FieldPrice::At(&usd, p, None));
        assert_eq!((f.sign.as_deref(), f.unit.as_str(), f.typed_in), (Some("ɱ"), "XMR", Unit::Xmr));
        assert_eq!(f.line.as_deref(), Some("≈ $20.00 · 1 XMR = $166.10"));
        assert_eq!((f.label.as_str(), f.switch_label.as_deref()), ("Amount in XMR", Some("Amount in XMR. Switch to US dollars")));
        // Empty: the rate alone. Not an amount: says so.
        assert_eq!(amount_field(Fiat(" "), FieldPrice::At(&usd, p, None)).line.as_deref(), Some("1 XMR = $166.10"));
        assert_eq!(amount_field(Fiat("20.123"), FieldPrice::At(&usd, p, None)).line.as_deref(), Some("That isn't an amount"));
        assert_eq!(amount_field(Xmr("abc"), FieldPrice::At(&usd, p, None)).line.as_deref(), Some("That isn't an amount"));
        // Euros, and a currency without a symbol: the chip names it, no sign.
        let f = amount_field(Fiat("5"), FieldPrice::At(&eur, p, None));
        assert_eq!((f.sign.as_deref(), f.unit.as_str(), f.label.as_str()), (Some("€"), "EUR", "Amount in euros"));
        let f = amount_field(Fiat("5"), FieldPrice::At(&chf, p, None));
        assert_eq!((f.sign, f.unit.as_str(), f.label.as_str()), (None, "CHF", "Amount in Swiss francs"));
        assert_eq!(f.line.as_deref(), Some("≈ 0.0301 XMR · 1 XMR = 166.10 CHF"));
        // No price: XMR only, no switch, nothing under it unless it's wrong.
        let f = amount_field(Fiat("20"), FieldPrice::None);
        assert_eq!((f.sign.as_deref(), f.unit.as_str(), f.typed_in, f.can_switch, f.line, f.switch_label), (Some("ɱ"), "XMR", Unit::Xmr, false, None, None));
        assert_eq!(amount_field(Xmr("x"), FieldPrice::None).line.as_deref(), Some("That isn't an amount"));
        // A currency chosen without a price to use: XMR only, saying why.
        let f = amount_field(Fiat(""), FieldPrice::Missing(&usd));
        assert_eq!((f.unit.as_str(), f.can_switch, f.line.as_deref()), ("XMR", false, Some("No USD price now · type the amount in XMR")));
        assert_eq!(amount_field(Xmr("0.5"), FieldPrice::Missing(&usd)).line.as_deref(), Some("No USD price now · type the amount in XMR"));
        assert_eq!(amount_field(Xmr("x"), FieldPrice::Missing(&usd)).line.as_deref(), Some("That isn't an amount"));
        // An aged price says when it's from, in place of the rate once there's an amount.
        let from = PriceFrom::Today("14:02".into());
        let aged = FieldPrice::At(&usd, p, Some(&from));
        assert_eq!(amount_field(Fiat(""), aged).line.as_deref(), Some("1 XMR = $166.10 · price from 14:02"));
        assert_eq!(amount_field(Fiat("20"), aged).line.as_deref(), Some("≈ 0.1204 XMR · price from 14:02"));
        assert_eq!(amount_field(Xmr("0.1204"), aged).line.as_deref(), Some("≈ $20.00 · price from 14:02"));
        assert!(amount_field(Fiat("20"), aged).can_switch);
    }

    /// A form opens blank, in money while there's a price: the whole field
    /// (sign, chip, rate) is there to draw on the first frame.
    #[test]
    fn a_blank_field_opens_in_money_only_with_a_price() {
        let usd = usd();
        let f = blank_field(FieldPrice::At(&usd, 166_100_000, None));
        assert_eq!((f.sign.as_deref(), f.unit.as_str(), f.typed_in, f.can_switch), (Some("$"), "USD", Unit::Fiat, true));
        assert_eq!(f.line.as_deref(), Some("1 XMR = $166.10"));
        assert_eq!((f.for_text.as_str(), f.for_typed_in, f.sendable), ("", Unit::Fiat, false));
        // No price yet: XMR, saying why; XMR chosen: XMR, nothing under it.
        let f = blank_field(FieldPrice::Missing(&usd));
        assert_eq!((f.unit.as_str(), f.typed_in, f.can_switch, f.for_typed_in), ("XMR", Unit::Xmr, false, Unit::Xmr));
        assert_eq!(f.line.as_deref(), Some("No USD price now · type the amount in XMR"));
        let f = blank_field(FieldPrice::None);
        assert_eq!((f.sign.as_deref(), f.unit.as_str(), f.typed_in, f.line), (Some("ɱ"), "XMR", Unit::Xmr, None));
    }

    #[test]
    fn settings_name_a_currency_or_give_its_code() {
        assert_eq!(usd().text(), "US dollar");
        assert_eq!(currency("XYZ").text(), "XYZ");
        assert_eq!((currency("CHF").sign(), currency("AUD").sign()), ("CHF".to_owned(), "A$".to_owned()));
    }

    #[test]
    fn the_amount_field_says_when_send_can_review_it() {
        let usd = usd();
        let p = 166_100_000;
        let sendable = |typed: Typed, price: FieldPrice| amount_field(typed, price).sendable;
        assert!(sendable(Fiat("20"), FieldPrice::At(&usd, p, None)));
        assert!(sendable(Xmr("0.1204"), FieldPrice::At(&usd, p, None)));
        // Typed in money without a price, it's read as XMR.
        assert!(sendable(Fiat("20"), FieldPrice::None));
        // It says what it was asked about, as asked.
        let f = amount_field(Fiat(" 20"), FieldPrice::None);
        assert_eq!((f.for_text.as_str(), f.for_typed_in, f.typed_in), (" 20", Unit::Fiat, Unit::Xmr));
        for typed in [Fiat(""), Xmr(" "), Xmr("0"), Fiat("0.00"), Fiat("20.123"), Xmr("abc")] {
            assert!(!sendable(typed, FieldPrice::At(&usd, p, None)), "{typed:?}");
        }
    }

    #[test]
    fn whole_yen_and_thousandths_of_a_dinar_work_like_cents() {
        let (jpy, kwd) = (currency("JPY"), currency("KWD"));
        let p = 56_123_000;
        // The amount field takes them as typed.
        let f = amount_field(Fiat("1500"), FieldPrice::At(&jpy, 93_477_400_000, None));
        assert_eq!((f.sign.as_deref(), f.unit.as_str(), f.label.as_str()), (Some("¥"), "JPY", "Amount in Japanese yen"));
        assert_eq!(f.line.as_deref(), Some("≈ 0.01605 XMR · 1 XMR = ¥93,477"));
        assert_eq!(amount_field(Fiat("1500.5"), FieldPrice::At(&jpy, 93_477_400_000, None)).line.as_deref(), Some("That isn't an amount"));
        let f = amount_field(Fiat("1.5"), FieldPrice::At(&kwd, p, None));
        assert_eq!((f.sign, f.label.as_str(), f.line.as_deref()), (None, "Amount in Kuwaiti dinars", Some("≈ 0.02673 XMR · 1 XMR = 56.123 KWD")));
        assert!(f.sendable);
    }

    #[test]
    fn a_wallet_file_from_before_prices_gets_the_defaults() {
        let settings: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(settings, Settings::default());
        assert_eq!(settings.code(), Some("USD"), "dollars unless XMR only is chosen");
        assert_eq!(source_text(&settings), "Kraken · web");
        let coingecko_tor = Settings { provider: Provider::CoinGecko, transport: Transport::Tor, ..Settings::default() };
        assert_eq!(source_text(&coingecko_tor), "CoinGecko · Tor", "the Ask over row's name");
        let cache: Cache = serde_json::from_str("{}").unwrap();
        assert_eq!(cache, Cache::default());
    }

    #[test]
    fn ask_over_says_why_tor_is_greyed() {
        let mut s = Settings::default();
        let rows = transport_rows(&s);
        assert_eq!(rows.iter().map(|r| (transport_name(r.transport), r.available, r.chosen)).collect::<Vec<_>>(), [("Web", true, true), ("Tor", false, false)]);
        assert_eq!(rows[0].note, TransportNote::SeesAddress);
        assert_eq!(rows[1].note, TransportNote::NotAnswered { provider: "Kraken", transport: Transport::Tor });
        // CoinGecko answers over Tor, but Fob hasn't Tor built in: still
        // greyed, saying so.
        s.choose_provider(Provider::CoinGecko).unwrap();
        let tor = &transport_rows(&s)[1];
        assert_eq!((tor.note, tor.available), (TransportNote::NotBuilt, false));
    }

    #[test]
    fn choosing_a_currency_moves_to_a_provider_that_prices_it() {
        let mut s = Settings::default();
        assert_eq!(s.choose_currency(Some("AUD")).unwrap(), None, "Kraken prices it");
        assert_eq!(s.provider, Provider::Kraken);
        assert_eq!(s.choose_currency(Some("NZD")).unwrap().map(moved).as_deref(), Some("Kraken doesn't price New Zealand dollars, so Fob now asks CoinGecko"));
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
        assert_eq!(s.choose_provider(Provider::Kraken).unwrap().map(moved).as_deref(), Some("Kraken doesn't answer over Tor, so Fob now asks over the web"));
        assert_eq!((s.provider, s.transport), (Provider::Kraken, Transport::Web));
        assert_eq!(s.choose_provider(Provider::CoinGecko).unwrap(), None);
        // One that doesn't price the currency is refused.
        s.choose_currency(Some("NZD")).unwrap();
        assert_eq!(s.choose_provider(Provider::Kraken).unwrap_err(), PriceError::NotPriced { provider: "Kraken", currency: "New Zealand dollars" });
        assert_eq!(s.provider, Provider::CoinGecko);
        // With XMR only, any.
        s.choose_currency(None).unwrap();
        assert_eq!(s.choose_provider(Provider::Kraken).unwrap(), None);
    }

    #[test]
    fn the_currency_list_puts_monero_and_the_phone_s_first() {
        let list = currency_list(Some("AUD"), Some("aud"), "");
        let headings: Vec<_> = list.groups.iter().map(|g| g.heading).collect();
        assert_eq!(headings, [CurrencyHeading::Monero, CurrencyHeading::ThisDevice, CurrencyHeading::All { count: 45 }]);
        let xmr = &list.groups[0].rows[0];
        assert_eq!((xmr.code.as_deref(), xmr.sign.as_str(), xmr.name.as_str(), currency_line(&xmr.line).as_str(), xmr.chosen), (None, "ɱ", "Monero (XMR)", "Fob never looks up a price", false));
        let phone = &list.groups[1].rows[0];
        assert_eq!((phone.sign.as_str(), phone.name.as_str(), currency_line(&phone.line).as_str(), phone.chosen), ("A$", "Australian dollar", "AUD · Kraken, CoinGecko", true));
        let all = &list.groups[2].rows;
        assert_eq!(all.len(), 45);
        assert_eq!(all[0].name, "Argentine peso");
        assert_eq!(currency_line(&all[0].line), "ARS · CoinGecko");
        assert_eq!(all.iter().filter(|r| r.chosen).count(), 1);
        // A phone currency Fob doesn't have, or none: no such row.
        assert_eq!(currency_list(None, Some("XOF"), "").groups.len(), 2);
        assert!(currency_list(None, None, "").groups[0].rows[0].chosen, "XMR chosen");
        // Searching: matches only, no phone row.
        let found = currency_list(Some("AUD"), Some("AUD"), "dollar");
        assert_eq!(found.groups.len(), 1);
        assert_eq!(found.groups[0].heading, CurrencyHeading::Found { count: found.groups[0].rows.len(), of: 45 });
        assert!(found.groups[0].rows.iter().all(|r| r.name.contains("dollar")));
        let zloty = currency_list(None, None, "ZLOTY");
        assert_eq!(zloty.groups[0].rows.iter().map(|r| r.code.as_deref()).collect::<Vec<_>>(), [Some("PLN")]);
        assert_eq!(currency_list(None, None, "xmr").groups[0].heading, CurrencyHeading::Monero);
        assert_eq!(currency_list(None, None, "monero").groups[0].rows[0].code, None);
        assert_eq!(currency_list(None, None, "ɱ").groups[0].heading, CurrencyHeading::Monero);
        let none = currency_list(None, None, " qqq ");
        assert_eq!((none.groups.len(), no_match(&none).as_deref()), (0, Some("No currency matches “qqq”")));
    }

    #[test]
    fn price_from_s_intro_is_rust_s() {
        let mut s = Settings { currency: Some("AUD".into()), ..Settings::default() };
        let intro = price_from_intro(&s);
        assert_eq!((intro.before.as_str(), intro.bold.as_deref(), intro.after.as_str()), ("Prices in ", Some("Australian dollars"), ", checked as this page opens. Tap a provider to pick it."));
        s.currency = None;
        assert_eq!(price_from_intro(&s), Intro { before: "With XMR as the default currency, Fob never looks up a price.".into(), bold: None, after: String::new() });
    }
}
