//! CoinGecko's public API (<https://docs.coingecko.com/v3.0.1/reference/introduction>):
//! no account, no key. `simple/price` for the price now, the coin's daily
//! `market_chart` for the history, which reaches back 365 days for the
//! keyless API (more is refused). It prices XMR directly in all 45 of
//! Fob's currencies. Without a key it allows only a few requests a minute
//! and answers HTTP 429 past that, which Fob calls busy.

use std::ops::Range;

use serde_json::Value;

use super::{Failure, Source, Transport, currencies, micro_number};
use crate::error::{FobError, Result};

const API: &str = "https://api.coingecko.com/api/v3";
/// The most days back the keyless API gives.
const HISTORY_DAYS: i64 = 365;

pub struct CoinGecko;

/// The answer as JSON, or busy when its body says it's rate limited (as
/// it does alongside a 429).
fn json(body: &[u8]) -> Result<Value> {
    let v: Value = serde_json::from_slice(body).map_err(|_| FobError::PriceFailed(Failure::BadAnswer))?;
    let code = v.get("status").or_else(|| v.get("error")?.get("status")).and_then(|s| s.get("error_code")).and_then(Value::as_u64);
    match code {
        Some(429) => Err(FobError::PriceFailed(Failure::Busy)),
        Some(_) => Err(FobError::PriceFailed(Failure::NotAnswering)),
        None => Ok(v),
    }
}

/// `simple/price`'s `{"monero":{"aud":844.02}}`, in millionths.
pub(crate) fn parse_simple(body: &[u8], currency: &str) -> Result<u64> {
    json(body)?
        .get("monero")
        .and_then(|m| m.get(currency.to_ascii_lowercase()))
        .and_then(micro_number)
        .filter(|p| *p > 0)
        .ok_or(FobError::PriceFailed(Failure::BadAnswer))
}

/// `market_chart`'s `prices`, `[milliseconds, price]` each, one at the
/// start of each UTC day and the last at the moment asked, as (day,
/// price): the price as the day began, the first one in a day.
pub(crate) fn parse_chart(body: &[u8]) -> Result<Vec<(i64, u64)>> {
    let v = json(body)?;
    let rows = v.get("prices").and_then(Value::as_array).ok_or(FobError::PriceFailed(Failure::BadAnswer))?;
    let mut days: Vec<(i64, u64)> = rows
        .iter()
        .filter_map(|row| {
            let ms = row.get(0)?.as_i64()?;
            Some((ms.div_euclid(86_400_000), micro_number(row.get(1)?).filter(|p| *p > 0)?))
        })
        .collect();
    // Sorted, so the first in a day is kept.
    days.sort_by_key(|d| d.0);
    days.dedup_by_key(|d| d.0);
    Ok(days)
}

impl Source for CoinGecko {
    fn name(&self) -> &'static str {
        "CoinGecko"
    }

    fn currencies(&self) -> &'static [&'static str] {
        &currencies::CODES
    }

    /// The web and Tor. Tor isn't confirmed, since it isn't built:
    /// CoinGecko is expected to answer Tor's exits; look again then.
    fn answers_over(&self, _: Transport) -> bool {
        true
    }

    async fn current(&self, currency: &str, transport: Transport) -> Result<u64> {
        let code = currency.to_ascii_lowercase();
        parse_simple(&transport.get(&format!("{API}/simple/price?ids=monero&vs_currencies={code}")).await?, currency)
    }

    /// The whole window CoinGecko gives, whatever `days` asks.
    async fn daily(&self, currency: &str, days: Range<i64>, transport: Transport) -> Result<Vec<(i64, u64)>> {
        let code = currency.to_ascii_lowercase();
        let all = parse_chart(&transport.get(&format!("{API}/coins/monero/market_chart?vs_currency={code}&days={HISTORY_DAYS}&interval=daily")).await?)?;
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
    fn coingecko_prices_all_45() {
        assert_eq!(CoinGecko.currencies().len(), 45);
        assert!(CoinGecko.currencies().contains(&"NZD"));
    }

    #[test]
    fn a_simple_price_answer_gives_the_price() {
        // CoinGecko's answers on 2026-10-31.
        assert_eq!(parse_simple(br#"{"monero":{"aud":844.02}}"#, "AUD").unwrap(), 844_020_000);
        assert_eq!(parse_simple(br#"{"monero":{"jpy":93477,"bhd":223.69,"vnd":15306509}}"#, "JPY").unwrap(), 93_477_000_000);
        assert_eq!(parse_simple(br#"{"monero":{"jpy":93477,"bhd":223.69,"vnd":15306509}}"#, "VND").unwrap(), 15_306_509_000_000);
        // A currency it doesn't have.
        assert_eq!(parse_simple(br#"{"monero":{}}"#, "XYZ").unwrap_err().to_string(), "Answered without a price");
        let limited = br#"{"status":{"error_code":429,"error_message":"You've exceeded the Rate Limit. Please visit https://www.coingecko.com/en/api/pricing to subscribe to our API plans for higher rate limits."}}"#;
        assert_eq!(parse_simple(limited, "AUD").unwrap_err().to_string(), "Busy, try again soon");
        assert!(parse_simple(b"<html>", "AUD").is_err());
    }

    #[test]
    fn a_market_chart_answer_gives_each_day_s_first_price() {
        let body = br#"{"prices":[[1793318400000,858.4169718300233],[1793404800000,835.4879017046717],[1793451930000,843.8638856187766]],
            "market_caps":[[1793318400000,15834552311.9]],"total_volumes":[[1793318400000,88551234.1]]}"#;
        assert_eq!(parse_chart(body).unwrap(), [(20_756, 858_416_972), (20_757, 835_487_902)]);
        // More days than the keyless API gives.
        let refused = br#"{"error":{"status":{"timestamp":"2026-10-31T12:59:45.645+00:00","error_code":10012,"error_message":"Your request exceeds the allowed time range. Public API users are limited to querying historical data within the past 365 days."}}}"#;
        assert_eq!(parse_chart(refused).unwrap_err().to_string(), "Not answering");
        assert!(parse_chart(br#"{"prices":"none"}"#).is_err());
    }

    /// Real requests to CoinGecko, to check its answers still read.
    /// Run by hand: `cargo test -p fob-core coingecko_answers -- --ignored`.
    #[tokio::test]
    #[ignore = "asks CoinGecko over the internet"]
    async fn coingecko_answers_for_real() {
        let now = CoinGecko.current("NZD", Transport::Web).await.unwrap();
        assert!(now > 1_000_000, "XMR at {now} millionths of a New Zealand dollar");
        let today = (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() / 86_400) as i64;
        let days = CoinGecko.daily("JPY", today - HISTORY_DAYS + 1..today + 1, Transport::Web).await.unwrap();
        assert!(days.len() >= 360 && days.len() <= 365, "{} days", days.len());
        assert_eq!(days.last().unwrap().0, today);
    }
}
