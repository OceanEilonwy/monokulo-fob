//! Kraken's public REST API (<https://docs.kraken.com/api/>): no account,
//! no key. `Ticker` for the price now, `OHLC` at a daily interval for the
//! history, which reaches back 720 days and no further.

use std::ops::Range;

use serde_json::Value;

use super::{Source, Transport, micro};
use crate::error::{FobError, Result};

const API: &str = "https://api.kraken.com/0/public";

/// Kraken's XMR markets in money, as its `AssetPairs` listed them on
/// 2026-10-31: (currency, the pair to ask for, the key it answers under).
/// The rest of its XMR pairs are against coins (USDC, USDT, XBT).
pub const PAIRS: [(&str, &str, &str); 2] = [("USD", "XMRUSD", "XXMRZUSD"), ("EUR", "XMREUR", "XXMRZEUR")];
const CURRENCIES: [&str; 2] = [PAIRS[0].0, PAIRS[1].0];
/// The most daily candles `OHLC` returns.
const HISTORY_DAYS: i64 = 720;
/// One day, in OHLC's minutes.
const DAILY: u32 = 1440;

pub struct Kraken;

fn pair(currency: &str) -> Result<(&'static str, &'static str)> {
    PAIRS
        .iter()
        .find(|p| p.0 == currency)
        .map(|p| (p.1, p.2))
        .ok_or_else(|| FobError::Price(format!("Kraken has no XMR price in {currency}")))
}

/// The answer's `result`, or what Kraken said went wrong.
fn result(body: &[u8]) -> Result<Value> {
    let bad = || FobError::Price("Kraken's answer couldn't be read".into());
    let mut v: Value = serde_json::from_slice(body).map_err(|_| bad())?;
    if let Some(errors) = v.get("error").and_then(Value::as_array).filter(|e| !e.is_empty()) {
        let said: Vec<&str> = errors.iter().filter_map(Value::as_str).collect();
        return Err(FobError::Price(format!("Kraken said: {}", said.join(", "))));
    }
    v.get_mut("result").map(Value::take).ok_or_else(bad)
}

/// `Ticker`'s last trade price (`c[0]`), in millionths.
pub(crate) fn parse_ticker(body: &[u8], key: &str) -> Result<u64> {
    result(body)?
        .get(key)
        .and_then(|t| t.get("c")?.get(0)?.as_str().and_then(micro))
        .filter(|p| *p > 0)
        .ok_or_else(|| FobError::Price("Kraken's answer had no price".into()))
}

/// `OHLC`'s daily candles, `[time, open, high, low, close, vwap, volume,
/// count]` each, as (day, price): the day's volume-weighted average price,
/// nearer a moment picked at random in the day than its close, or the close
/// for a day with no trades.
pub(crate) fn parse_ohlc(body: &[u8], key: &str) -> Result<Vec<(i64, u64)>> {
    let result = result(body)?;
    let rows = result.get(key).and_then(Value::as_array).ok_or_else(|| FobError::Price("Kraken's answer had no prices".into()))?;
    Ok(rows
        .iter()
        .filter_map(|row| {
            let time = row.get(0)?.as_i64()?;
            let price = |i: usize| row.get(i)?.as_str().and_then(micro).filter(|p| *p > 0);
            Some((time.div_euclid(86_400), price(5).or_else(|| price(4))?))
        })
        .collect())
}

impl Source for Kraken {
    fn name(&self) -> &'static str {
        "Kraken"
    }

    fn currencies(&self) -> &'static [&'static str] {
        &CURRENCIES
    }

    async fn current(&self, currency: &str, transport: Transport) -> Result<u64> {
        let (pair, key) = pair(currency)?;
        parse_ticker(&transport.get(&format!("{API}/Ticker?pair={pair}")).await?, key)
    }

    /// One request for the whole window Kraken keeps, whatever `days`
    /// asks: `since` would tell Kraken the oldest day wanted.
    async fn daily(&self, currency: &str, days: Range<i64>, transport: Transport) -> Result<Vec<(i64, u64)>> {
        let (pair, key) = pair(currency)?;
        let all = parse_ohlc(&transport.get(&format!("{API}/OHLC?pair={pair}&interval={DAILY}")).await?, key)?;
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
    fn kraken_prices_xmr_in_dollars_and_euros() {
        assert_eq!(Kraken.currencies(), ["USD", "EUR"]);
        assert_eq!(pair("EUR").unwrap(), ("XMREUR", "XXMRZEUR"));
        assert!(pair("GBP").unwrap_err().to_string().contains("no XMR price in GBP"));
    }

    #[test]
    fn a_ticker_answer_gives_its_last_trade() {
        let body = br#"{"error":[],"result":{"XXMRZUSD":{"a":["593.91000000","1","1.000"],"b":["593.89000000","1","1.000"],"c":["593.91000000","0.40710000"],"o":"587.54000000"}}}"#;
        assert_eq!(parse_ticker(body, "XXMRZUSD").unwrap(), 593_910_000);
        assert!(parse_ticker(body, "XXMRZEUR").is_err());
        let refused = br#"{"error":["EQuery:Unknown asset pair"]}"#;
        assert_eq!(parse_ticker(refused, "XXMRZUSD").unwrap_err().to_string(), "Kraken said: EQuery:Unknown asset pair");
        assert!(parse_ticker(b"<html>", "XXMRZUSD").is_err());
    }

    #[test]
    fn an_ohlc_answer_gives_each_day_s_average() {
        let body = br#"{"error":[],"result":{"XXMRZEUR":[
            [1731196800,"151.93","154.59","148.88","152.04","150.85","146.94429593",128],
            [1731283200,"150.18","151.58","143.88","145.88","0.00000","0",0],
            [1731369600,"bad"]
        ],"last":1793318400}}"#;
        assert_eq!(parse_ohlc(body, "XXMRZEUR").unwrap(), [(20_037, 150_850_000), (20_038, 145_880_000)]);
    }

    /// One real request each to Kraken, to check its answers still read.
    /// Run by hand: `cargo test -p fob-core kraken_answers -- --ignored`.
    #[tokio::test]
    #[ignore = "asks Kraken over the internet"]
    async fn kraken_answers_for_real() {
        let now = Kraken.current("USD", Transport::Web).await.unwrap();
        assert!(now > 1_000_000, "XMR at {now} millionths of a dollar");
        let today = (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() / 86_400) as i64;
        let days = Kraken.daily("EUR", today - HISTORY_DAYS + 1..today + 1, Transport::Web).await.unwrap();
        assert!(days.len() >= 700 && days.len() <= 720, "{} days", days.len());
        assert_eq!(days.last().unwrap().0, today);
    }
}
