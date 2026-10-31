//! Kraken's public REST API (<https://docs.kraken.com/api/>): no account,
//! no key. `Ticker` for the price now, `OHLC` at a daily interval for the
//! history, which reaches back 720 days and no further.
//!
//! Kraken trades XMR against dollars and euros only. For a few more
//! currencies it multiplies XMR/USD by its own exchange market between the
//! dollar and that currency: two requests for the price now, two for the
//! history, joined by day.

use std::ops::Range;

use serde_json::Value;

use super::{Failure, MICRO, Source, Transport, micro};
use crate::error::{FobError, Result};

const API: &str = "https://api.kraken.com/0/public";

/// How Kraken prices XMR in a currency.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    /// An XMR market of its own: the pair to ask for, the key it answers
    /// under.
    Direct { pair: &'static str, key: &'static str },
    /// XMR/USD times an exchange market: its pair and key, and whether it
    /// quotes the currency per dollar (USD/JPY: multiply) rather than
    /// dollars per unit of it (AUD/USD: divide).
    ViaUsd { pair: &'static str, key: &'static str, per_dollar: bool },
}

/// XMR/USD's pair and key, which the exchange routes start from.
const XMR_USD: (&str, &str) = ("XMRUSD", "XXMRZUSD");

/// The currencies Kraken prices, as its `AssetPairs` listed the markets on
/// 2026-10-31. Its other XMR pairs are against coins (USDC, USDT, XBT). It
/// also lists USD/AED, but that market has never traded (an empty `OHLC`,
/// a last price of 0), so the dirham isn't here.
pub const ROUTES: [(&str, Route); 7] = [
    ("USD", Route::Direct { pair: XMR_USD.0, key: XMR_USD.1 }),
    ("EUR", Route::Direct { pair: "XMREUR", key: "XXMRZEUR" }),
    ("GBP", Route::ViaUsd { pair: "GBPUSD", key: "ZGBPZUSD", per_dollar: false }),
    ("AUD", Route::ViaUsd { pair: "AUDUSD", key: "AUDUSD", per_dollar: false }),
    ("CAD", Route::ViaUsd { pair: "USDCAD", key: "ZUSDZCAD", per_dollar: true }),
    ("CHF", Route::ViaUsd { pair: "USDCHF", key: "USDCHF", per_dollar: true }),
    ("JPY", Route::ViaUsd { pair: "USDJPY", key: "ZUSDZJPY", per_dollar: true }),
];
const CURRENCIES: [&str; 7] = {
    let mut out = [""; 7];
    let mut i = 0;
    while i < ROUTES.len() {
        out[i] = ROUTES[i].0;
        i += 1;
    }
    out
};
/// The most daily candles `OHLC` returns.
const HISTORY_DAYS: i64 = 720;
/// One day, in OHLC's minutes.
const DAILY: u32 = 1440;

pub struct Kraken;

fn route(currency: &str) -> Result<Route> {
    ROUTES.iter().find(|r| r.0 == currency).map(|r| r.1).ok_or_else(|| FobError::Price(format!("Kraken has no XMR price in {currency}")))
}

/// The answer's `result`, or what Kraken said went wrong: busy for its rate
/// limits, not answering for anything else it refused.
fn result(body: &[u8]) -> Result<Value> {
    let bad = || FobError::PriceFailed(Failure::BadAnswer);
    let mut v: Value = serde_json::from_slice(body).map_err(|_| bad())?;
    if let Some(errors) = v.get("error").and_then(Value::as_array).filter(|e| !e.is_empty()) {
        let busy = errors.iter().filter_map(Value::as_str).any(|e| e.contains("Rate limit") || e.contains("Too many requests"));
        return Err(FobError::PriceFailed(if busy { Failure::Busy } else { Failure::NotAnswering }));
    }
    v.get_mut("result").map(Value::take).ok_or_else(bad)
}

/// `Ticker`'s last trade price (`c[0]`), in millionths.
pub(crate) fn parse_ticker(body: &[u8], key: &str) -> Result<u64> {
    result(body)?
        .get(key)
        .and_then(|t| t.get("c")?.get(0)?.as_str().and_then(micro))
        .filter(|p| *p > 0)
        .ok_or(FobError::PriceFailed(Failure::BadAnswer))
}

/// `OHLC`'s daily candles, `[time, open, high, low, close, vwap, volume,
/// count]` each, as (day, price): the day's volume-weighted average price,
/// nearer a moment picked at random in the day than its close, or the close
/// for a day with no trades.
pub(crate) fn parse_ohlc(body: &[u8], key: &str) -> Result<Vec<(i64, u64)>> {
    let result = result(body)?;
    let rows = result.get(key).and_then(Value::as_array).ok_or(FobError::PriceFailed(Failure::BadAnswer))?;
    Ok(rows
        .iter()
        .filter_map(|row| {
            let time = row.get(0)?.as_i64()?;
            let price = |i: usize| row.get(i)?.as_str().and_then(micro).filter(|p| *p > 0);
            Some((time.div_euclid(86_400), price(5).or_else(|| price(4))?))
        })
        .collect())
}

/// XMR's price in dollars (`xmr_usd`) through an exchange rate (`rate`, in
/// millionths), rounded half up; `None` at a rate of 0.
pub(crate) fn through(xmr_usd: u64, rate: u64, per_dollar: bool) -> Option<u64> {
    let (num, den) = if per_dollar { (xmr_usd as u128 * rate as u128, MICRO as u128) } else { (xmr_usd as u128 * MICRO as u128, rate as u128) };
    if den == 0 {
        return None;
    }
    u64::try_from((num + den / 2) / den).ok().filter(|p| *p > 0)
}

/// Daily XMR/USD prices through daily exchange rates, by day. A day the
/// exchange market has no candle for takes the last rate before it; a day
/// before any rate is left out.
pub(crate) fn join_days(xmr_usd: &[(i64, u64)], rates: &[(i64, u64)], per_dollar: bool) -> Vec<(i64, u64)> {
    let mut rates: Vec<(i64, u64)> = rates.to_vec();
    rates.sort_by_key(|r| r.0);
    xmr_usd
        .iter()
        .filter_map(|&(day, price)| {
            let i = rates.partition_point(|r| r.0 <= day);
            let rate = rates[..i].last()?.1;
            Some((day, through(price, rate, per_dollar)?))
        })
        .collect()
}

async fn ticker(pair: &str, key: &str, transport: Transport) -> Result<u64> {
    parse_ticker(&transport.get(&format!("{API}/Ticker?pair={pair}")).await?, key)
}

/// One request for the whole window Kraken keeps: `since` would tell
/// Kraken the oldest day wanted.
async fn ohlc(pair: &str, key: &str, transport: Transport) -> Result<Vec<(i64, u64)>> {
    parse_ohlc(&transport.get(&format!("{API}/OHLC?pair={pair}&interval={DAILY}")).await?, key)
}

impl Source for Kraken {
    fn name(&self) -> &'static str {
        "Kraken"
    }

    fn currencies(&self) -> &'static [&'static str] {
        &CURRENCIES
    }

    /// The web only. Not confirmed, since Tor isn't built: Kraken is
    /// expected to refuse Tor's exits, as its site does; look again then.
    fn answers_over(&self, transport: Transport) -> bool {
        transport == Transport::Web
    }

    async fn current(&self, currency: &str, transport: Transport) -> Result<u64> {
        match route(currency)? {
            Route::Direct { pair, key } => ticker(pair, key, transport).await,
            Route::ViaUsd { pair, key, per_dollar } => {
                let (xmr_usd, rate) = tokio::try_join!(ticker(XMR_USD.0, XMR_USD.1, transport), ticker(pair, key, transport))?;
                through(xmr_usd, rate, per_dollar).ok_or(FobError::PriceFailed(Failure::BadAnswer))
            }
        }
    }

    /// The whole window Kraken keeps, whatever `days` asks.
    async fn daily(&self, currency: &str, days: Range<i64>, transport: Transport) -> Result<Vec<(i64, u64)>> {
        let all = match route(currency)? {
            Route::Direct { pair, key } => ohlc(pair, key, transport).await?,
            Route::ViaUsd { pair, key, per_dollar } => {
                let (xmr_usd, rates) = tokio::try_join!(ohlc(XMR_USD.0, XMR_USD.1, transport), ohlc(pair, key, transport))?;
                join_days(&xmr_usd, &rates, per_dollar)
            }
        };
        Ok(all.into_iter().filter(|(day, _)| days.contains(day)).collect())
    }

    fn history_days(&self) -> i64 {
        HISTORY_DAYS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kraken_prices_xmr_in_seven_currencies() {
        assert_eq!(Kraken.currencies(), ["USD", "EUR", "GBP", "AUD", "CAD", "CHF", "JPY"]);
        assert_eq!(route("EUR").unwrap(), Route::Direct { pair: "XMREUR", key: "XXMRZEUR" });
        assert!(matches!(route("JPY").unwrap(), Route::ViaUsd { per_dollar: true, .. }));
        assert!(route("NZD").unwrap_err().to_string().contains("no XMR price in NZD"));
        assert!(route("AED").is_err(), "USD/AED never trades");
    }

    /// Kraken's answers on 2026-10-31, cut down.
    const XMR_USD_TICKER: &[u8] = br#"{"error":[],"result":{"XXMRZUSD":{"a":["593.80000000","1","1.000"],"b":["593.27000000","13","13.000"],"c":["593.73000000","0.02470432"],"v":["5106.77623027","8078.18059783"],"p":["594.41602778","592.35442506"],"t":[3599,6982],"l":["585.65000000","572.49000000"],"h":["601.05000000","601.05000000"],"o":"587.54000000"}}}"#;
    const AUD_USD_TICKER: &[u8] = br#"{"error":[],"result":{"AUDUSD":{"a":["0.70306","188","188.000"],"b":["0.70299","20","20.000"],"c":["0.70295","134.57654198"],"v":["718657.78395624","1464965.78094039"],"p":["0.70300","0.70302"],"t":[1863,3218],"l":["0.70251","0.70119"],"h":["0.70377","0.70386"],"o":"0.70305"}}}"#;
    const USD_JPY_TICKER: &[u8] = br#"{"error":[],"result":{"ZUSDZJPY":{"a":["159.730","51","51.000"],"b":["156.273","49","49.000"],"c":["159.731","5.85350000"],"v":["5754.48576341","6866.77938459"],"p":["159.911","159.643"],"t":[10,21],"l":["156.271","156.271"],"h":["159.992","159.992"],"o":"159.948"}}}"#;
    const USD_AED_TICKER: &[u8] = br#"{"error":[],"result":{"USDAED":{"a":["5000.00000","50","50.000"],"b":["0.00000","0","0.000"],"c":["0.00000","0.00000000"],"v":["0.00000000","0.00000000"],"p":["0.00000","0.00000"],"t":[0,0],"l":["0.00000","0.00000"],"h":["0.00000","0.00000"],"o":"0.00000"}}}"#;

    #[test]
    fn a_ticker_answer_gives_its_last_trade() {
        assert_eq!(parse_ticker(XMR_USD_TICKER, "XXMRZUSD").unwrap(), 593_730_000);
        assert_eq!(parse_ticker(AUD_USD_TICKER, "AUDUSD").unwrap(), 702_950);
        assert_eq!(parse_ticker(USD_JPY_TICKER, "ZUSDZJPY").unwrap(), 159_731_000);
        assert!(parse_ticker(XMR_USD_TICKER, "XXMRZEUR").is_err());
        // A market that never traded has no price.
        assert_eq!(parse_ticker(USD_AED_TICKER, "USDAED").unwrap_err().to_string(), "Answered without a price");
        let refused = br#"{"error":["EQuery:Unknown asset pair"]}"#;
        assert_eq!(parse_ticker(refused, "XXMRZUSD").unwrap_err().to_string(), "Not answering");
        let limited = br#"{"error":["EAPI:Rate limit exceeded"]}"#;
        assert_eq!(parse_ticker(limited, "XXMRZUSD").unwrap_err().to_string(), "Busy, try again soon");
        assert!(parse_ticker(b"<html>", "XXMRZUSD").is_err());
    }

    #[test]
    fn other_currencies_go_through_the_dollar() {
        let xmr_usd = parse_ticker(XMR_USD_TICKER, "XXMRZUSD").unwrap();
        // AUD/USD is dollars per Australian dollar: divide. $593.73 is A$844.62.
        let aud = through(xmr_usd, parse_ticker(AUD_USD_TICKER, "AUDUSD").unwrap(), false).unwrap();
        assert_eq!(aud, 844_626_218);
        // CoinGecko said A$844.02 that minute: within a few percent.
        assert!(aud.abs_diff(844_020_000) * 100 < 844_020_000 * 3);
        // USD/JPY is yen per dollar: multiply. $593.73 is ¥94,837.
        assert_eq!(through(xmr_usd, parse_ticker(USD_JPY_TICKER, "ZUSDZJPY").unwrap(), true), Some(94_837_086_630));
        assert_eq!(through(xmr_usd, 0, false), None);
        assert_eq!(through(xmr_usd, 0, true), None);
    }

    #[test]
    fn an_ohlc_answer_gives_each_day_s_average() {
        let body = br#"{"error":[],"result":{"XXMRZEUR":[
            [1731196800,"151.93","154.59","148.88","152.04","150.85","146.94429593",128],
            [1731283200,"150.18","151.58","143.88","145.88","0.00000","0",0],
            [1731369600,"bad"]
        ],"last":1793318400}}"#;
        assert_eq!(parse_ohlc(body, "XXMRZEUR").unwrap(), [(20_037, 150_850_000), (20_038, 145_880_000)]);
        // The never-traded market's whole history.
        let empty = br#"{"error":[],"result":{"USDAED":[[0,"0.000","0.000","0.000","0.000","0.000","0.00000000",0]],"last":0}}"#;
        assert_eq!(parse_ohlc(empty, "USDAED").unwrap(), []);
    }

    #[test]
    fn histories_through_the_dollar_are_joined_by_day() {
        let xmr = br#"{"error":[],"result":{"XXMRZUSD":[
            [1793232000,"597.92","649.40","582.03","601.94","608.41","17336.69890365",13911],
            [1793318400,"602.50","638.87","572.49","587.49","597.05","11512.57850975",10732],
            [1793404800,"587.54","601.05","585.65","593.54","594.41","5108.11007801",3614]
        ],"last":1793318400}}"#;
        let jpy = br#"{"error":[],"result":{"ZUSDZJPY":[
            [1793232000,"156.235","160.482","156.235","159.723","159.940","6763.66208734",33],
            [1793404800,"159.948","159.992","156.271","159.731","159.911","5754.48576341",10]
        ],"last":1793318400}}"#;
        let days = join_days(&parse_ohlc(xmr, "XXMRZUSD").unwrap(), &parse_ohlc(jpy, "ZUSDZJPY").unwrap(), true);
        // The middle day has no exchange candle: the day before's rate.
        assert_eq!(days, [(20_755, 97_309_095_400), (20_756, 95_492_177_000), (20_757, 95_052_697_510)]);
        // A day before any rate is left out.
        assert_eq!(join_days(&[(20_000, 1)], &[(20_001, 1_000_000)], true), []);
    }

    /// Real requests to Kraken, to check its answers still read.
    /// Run by hand: `cargo test -p fob-core kraken_answers -- --ignored`.
    #[tokio::test]
    #[ignore = "asks Kraken over the internet"]
    async fn kraken_answers_for_real() {
        for code in CURRENCIES {
            let now = Kraken.current(code, Transport::Web).await.unwrap();
            assert!(now > 1_000_000, "XMR at {now} millionths of {code}");
        }
        let today = (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() / 86_400) as i64;
        for code in ["EUR", "AUD"] {
            let days = Kraken.daily(code, today - HISTORY_DAYS + 1..today + 1, Transport::Web).await.unwrap();
            assert!(days.len() >= 700 && days.len() <= 720, "{} days of {code}", days.len());
            assert_eq!(days.last().unwrap().0, today);
        }
    }
}
