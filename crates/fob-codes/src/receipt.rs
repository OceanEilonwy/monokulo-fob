//! ORF receipts as Fob shows them: names, lines and money already formatted.

use fob_monero::fiat;
use orf_codec::Receipt;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReceiptLine {
    /// "2 × Flat white", or just the description for one.
    pub label: String,
    pub amount: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReceiptView {
    pub merchant: String,
    /// The receipt's own time, as the shop wrote it (RFC 3339).
    pub issued: String,
    pub lines: Vec<ReceiptLine>,
    pub item_count: u32,
    pub tax: Option<String>,
    pub total: String,
    pub currency: String,
    /// The total as a number, to check against what the payment is worth.
    #[serde(default)]
    pub total_value: f64,
}

/// How far apart, in percent, the receipt's total and Fob's own valuation of
/// the payment can be before Confirm warns: room for the shop's price being a
/// few minutes older than Fob's, not for a different amount.
const AGREE_PERCENT: u128 = 5;

/// Whether the receipt's total disagrees with what `atomic` XMR is worth at
/// Fob's `price` in `currency`: `None` when they can't be compared, the
/// receipt being in another currency.
///
/// Both the receipt and the amount come from the same code, so a tampered
/// code can pair a small total with a large amount; Fob's own price is the
/// only thing here the code didn't say.
pub fn disagrees(view: &ReceiptView, atomic: u64, price: u64, currency: &fiat::Currency) -> Option<bool> {
    if !view.currency.eq_ignore_ascii_case(&currency.code) {
        return None;
    }
    let worth = fiat::to_minor(atomic, price, currency) as u128;
    let scaled = (view.total_value * 10f64.powi(currency.decimals as i32)).round();
    // A negative, absurd or missing total can't agree with a real payment.
    if !(scaled.is_finite() && scaled > 0.0 && scaled < u64::MAX as f64) {
        return Some(worth > 0);
    }
    let said = scaled as u128;
    Some(worth.abs_diff(said) * 100 > AGREE_PERCENT * worth.max(said))
}

/// A receipt as Confirm shows it, set against Fob's prices.
#[derive(Clone, Debug, PartialEq)]
pub struct Checked {
    /// The receipt's currency when it isn't the wallet's ("USD"), for the
    /// receipt's heading; `None` in the wallet's own.
    pub other_currency: Option<String>,
    /// Lines, tax and total as written beside the wallet's money: "US$4.50"
    /// where a bare "$" could be another dollar.
    pub lines: Vec<ReceiptLine>,
    pub tax: Option<String>,
    pub total: String,
    /// In another currency, the total in the wallet's at Fob's prices, its
    /// last row: ("US$8.25 in AUD", "A$12.62").
    pub converted: Option<(String, String)>,
    /// The total and what the XMR is worth are more than 5% apart: the red
    /// line under the receipt.
    pub mismatch: Option<String>,
    /// Fob has no price in the receipt's currency: "No USD price right now".
    pub no_price: Option<String>,
}

/// Whether a receipt in `receipt_currency` is in another currency than the
/// wallet's (`None`: XMR only), so money beside it is written
/// [`distinct`](fiat::Currency::distinct)ly.
pub fn other_currency(receipt_currency: &str, wallet: Option<&str>) -> bool {
    wallet.is_some_and(|w| !w.eq_ignore_ascii_case(receipt_currency))
}

/// How a receipt writes `code`: as Fob knows it, or by its code with the
/// usual digits for one it doesn't.
fn receipt_currency(code: &str) -> fiat::Currency {
    let mut currency = fiat::currency(code);
    if fiat::currencies::find(code).is_none() {
        currency.decimals = minor_digits(code);
    }
    currency
}

/// `text`, written by [`money`] in `currency`, written distinctly instead:
/// "$4.50" as "US$4.50". Anything else comes back as it was.
fn distinct_text(text: &str, currency: &fiat::Currency) -> String {
    let distinct = currency.clone().distinct();
    let (Some(symbol), Some(d)) = (currency.symbol, distinct.symbol.filter(|d| Some(*d) != currency.symbol)) else { return text.to_owned() };
    let (sign, rest) = text.strip_prefix('−').map_or(("", text), |rest| ("−", rest));
    match rest.strip_prefix(symbol) {
        Some(digits) => format!("{sign}{d}{digits}"),
        None => text.to_owned(),
    }
}

/// Sets the receipt for paying `atomic` against Fob's prices. `wallet` is
/// the currency amounts are also shown in (`None`: XMR only, when Fob asks
/// for no prices and checks nothing), at `wallet_price` when it has one;
/// `receipt_price` is Fob's price in the receipt's currency, when another.
///
/// With no price in the receipt's currency the total can't be checked, and
/// a muted line says so: never taken as a match, never a warning.
pub fn check(view: &ReceiptView, atomic: u64, wallet: Option<&fiat::Currency>, wallet_price: Option<u64>, receipt_price: Option<u64>) -> Checked {
    let plain = receipt_currency(&view.currency);
    let other = other_currency(&view.currency, wallet.map(|w| w.code.as_str()));
    let written = |text: &str| if other { distinct_text(text, &plain) } else { text.to_owned() };
    let mut checked = Checked {
        other_currency: other.then(|| view.currency.to_ascii_uppercase()),
        lines: view.lines.iter().map(|l| ReceiptLine { label: l.label.clone(), amount: written(&l.amount) }).collect(),
        tax: view.tax.as_deref().map(written),
        total: written(&view.total),
        converted: None,
        mismatch: None,
        no_price: None,
    };
    let Some(wallet) = wallet else { return checked };
    let receipt_price = if other { receipt_price } else { wallet_price };
    let Some(receipt_price) = receipt_price.filter(|p| *p > 0) else {
        checked.no_price = Some(format!("No {} price right now", view.currency.to_ascii_uppercase()));
        return checked;
    };
    let receipt_in = if other { plain.clone().distinct() } else { plain.clone() };
    let wallet_in = if other { wallet.clone().distinct() } else { wallet.clone() };
    let real_total = view.total_value.is_finite() && view.total_value > 0.0;
    // The total in the wallet's currency, through XMR at Fob's two prices.
    let in_wallet = match wallet_price {
        Some(wp) if other && real_total => {
            let minor = (view.total_value * wp as f64 / receipt_price as f64 * 10f64.powi(wallet.decimals as i32)).round();
            (minor.is_finite() && minor < u64::MAX as f64).then(|| fiat::format(minor as u64, &wallet_in))
        }
        _ => None,
    };
    if let Some(value) = &in_wallet {
        checked.converted = Some((format!("{} in {}", checked.total, wallet.code), value.clone()));
    }
    if disagrees(view, atomic, receipt_price, &plain) == Some(true) {
        let (paying, expected) = match (wallet_price, &in_wallet) {
            (Some(wp), Some(value)) => (fiat::value(atomic, wp, &wallet_in), value.clone()),
            _ => (fiat::value(atomic, receipt_price, &receipt_in), checked.total.clone()),
        };
        checked.mismatch = Some(if real_total {
            format!("You're paying {paying}, not {expected}. Don't send unless the till shows the same XMR.")
        } else {
            "This receipt's total isn't a real amount. Don't send unless the till shows the same XMR.".to_owned()
        });
    }
    checked
}

fn minor_digits(currency: &str) -> u32 {
    match currency {
        "JPY" | "KRW" | "VND" | "CLP" | "ISK" | "UGX" | "XAF" | "XOF" => 0,
        "BHD" | "KWD" | "OMR" | "JOD" | "TND" | "LYD" | "IQD" => 3,
        _ => 2,
    }
}

/// `amount` in `currency`, written as Fob writes money: its own sign first
/// ("$24.50", "A$9.00", "€9.00"), else the code after ("12.00 CHF").
pub fn money(amount: f64, currency: &str) -> String {
    let currency = receipt_currency(currency);
    let minor = (amount.abs() * 10f64.powi(currency.decimals as i32)).round();
    let minor = if minor.is_finite() && minor < u64::MAX as f64 { minor as u64 } else { 0 };
    let sign = if amount < 0.0 { "−" } else { "" };
    format!("{sign}{}", fiat::format(minor, &currency))
}

fn quantity(q: f64) -> String {
    if q.fract() == 0.0 { format!("{}", q as i64) } else { format!("{q}") }
}

impl From<&Receipt> for ReceiptView {
    fn from(r: &Receipt) -> Self {
        let lines = r
            .line_items
            .iter()
            .map(|item| ReceiptLine {
                label: if item.quantity == 1.0 {
                    item.description.clone()
                } else {
                    format!("{} × {}", quantity(item.quantity), item.description)
                },
                amount: money(item.line_total, &r.currency),
            })
            .collect();
        let item_count = r.line_items.iter().map(|i| if i.quantity.fract() == 0.0 { i.quantity.max(1.0) as u32 } else { 1 }).sum();
        let tax = r
            .tax
            .as_ref()
            .and_then(|t| t.tax_lines.as_ref())
            .map(|lines| lines.iter().map(|l| l.tax_amount).sum::<f64>())
            .filter(|t| *t != 0.0)
            .map(|t| money(t, &r.currency));
        ReceiptView {
            merchant: r.merchant.display_name.clone().unwrap_or_else(|| r.merchant.legal_name.clone()),
            issued: r.receipt.issue_timestamp.clone(),
            lines,
            item_count,
            tax,
            total: money(r.total, &r.currency),
            currency: r.currency.clone(),
            total_value: r.total,
        }
    }
}

/// A receipt as ORF JSON, to keep with the payment it came with.
pub fn to_json(receipt: &Receipt) -> Option<String> {
    serde_json::to_string(receipt).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_is_written_the_usual_way() {
        assert_eq!(money(24.5, "USD"), "$24.50");
        assert_eq!(money(8.25, "AUD"), "A$8.25");
        assert_eq!(money(9.0, "EUR"), "€9.00");
        assert_eq!(money(1200.0, "JPY"), "¥1,200");
        assert_eq!(money(1.5, "KWD"), "1.500 KWD");
        assert_eq!(money(2.0, "OMR"), "2.000 OMR");
        assert_eq!(money(-3.0, "GBP"), "−£3.00");
    }

    #[test]
    fn a_cafe_receipt_reads_like_the_design() {
        let receipt: Receipt = serde_json::from_str(
            r##"{
              "orf_version": "0.1.0",
              "receipt": { "receipt_id": "#1042", "issue_timestamp": "2026-10-31T09:12:00Z", "receipt_type": "sale" },
              "merchant": { "legal_name": "Corner Café Ltd", "display_name": "Corner Café", "country_code": "US" },
              "line_items": [
                { "description": "Flat white", "quantity": 2, "unit_price": 4.5, "line_total": 9.0 },
                { "description": "Almond croissant", "quantity": 1, "unit_price": 4.5, "line_total": 4.5 }
              ],
              "tax": { "tax_lines": [ { "rate": 0.1, "tax_amount": 1.23 } ] },
              "total": 13.5,
              "currency": "USD"
            }"##,
        )
        .unwrap();
        let view = ReceiptView::from(&receipt);
        assert_eq!(view.merchant, "Corner Café");
        assert_eq!(view.lines[0], ReceiptLine { label: "2 × Flat white".into(), amount: "$9.00".into() });
        assert_eq!(view.lines[1].label, "Almond croissant");
        assert_eq!(view.item_count, 3);
        assert_eq!(view.tax.as_deref(), Some("$1.23"));
        assert_eq!(view.total, "$13.50");
        assert_eq!(view.total_value, 13.5);
    }

    fn view(total: f64, currency: &str) -> ReceiptView {
        ReceiptView {
            merchant: "Corner Café".into(),
            issued: String::new(),
            lines: vec![ReceiptLine { label: "Flat white".into(), amount: money(total, currency) }],
            item_count: 1,
            tax: None,
            total: money(total, currency),
            currency: currency.into(),
            total_value: total,
        }
    }

    const XMR: u64 = fob_monero::amount::ATOMIC_PER_XMR;

    #[test]
    fn a_receipt_is_checked_against_fob_s_own_price() {
        let usd = fiat::currency("USD");
        // $160.00 per XMR.
        let price = 160 * fiat::MICRO;
        // 0.05 XMR is $8.00: a receipt for $8.00, or a little off, agrees.
        assert_eq!(disagrees(&view(8.0, "USD"), XMR / 20, price, &usd), Some(false));
        assert_eq!(disagrees(&view(7.80, "USD"), XMR / 20, price, &usd), Some(false));
        // A $5.00 receipt on a code asking 5 XMR ($800) is the attack.
        assert_eq!(disagrees(&view(5.0, "USD"), 5 * XMR, price, &usd), Some(true));
        // Not a total at all.
        assert_eq!(disagrees(&view(-1.0, "USD"), XMR, price, &usd), Some(true));
        assert_eq!(disagrees(&view(f64::NAN, "USD"), XMR, price, &usd), Some(true));
        // Another currency can't be compared at a USD price.
        assert_eq!(disagrees(&view(5.0, "EUR"), 5 * XMR, price, &usd), None);
    }

    #[test]
    fn a_matching_receipt_in_the_wallet_s_currency_adds_nothing() {
        let usd = fiat::currency("USD");
        let checked = check(&view(8.0, "USD"), XMR / 20, Some(&usd), Some(160 * fiat::MICRO), None);
        assert_eq!(checked.other_currency, None);
        assert_eq!(checked.total, "$8.00");
        assert_eq!((checked.converted, checked.mismatch, checked.no_price), (None, None, None));
    }

    #[test]
    fn a_receipt_far_from_the_price_says_so_in_one_line() {
        let usd = fiat::currency("USD");
        let checked = check(&view(5.0, "USD"), 5 * XMR, Some(&usd), Some(160 * fiat::MICRO), None);
        assert_eq!(checked.mismatch.as_deref(), Some("You're paying $800.00, not $5.00. Don't send unless the till shows the same XMR."));
        let nan = check(&view(f64::NAN, "USD"), 5 * XMR, Some(&usd), Some(160 * fiat::MICRO), None);
        assert!(nan.mismatch.unwrap().starts_with("This receipt's total isn't a real amount."));
    }

    #[test]
    fn a_receipt_in_another_currency_is_converted_and_names_its_dollar() {
        let aud = fiat::currency("AUD");
        // A$240.00 and US$160.00 per XMR: 0.05 XMR is US$8.00, A$12.00.
        let (wallet_price, usd_price) = (240 * fiat::MICRO, 160 * fiat::MICRO);
        let checked = check(&view(8.0, "USD"), XMR / 20, Some(&aud), Some(wallet_price), Some(usd_price));
        assert_eq!(checked.other_currency.as_deref(), Some("USD"));
        assert_eq!(checked.lines[0].amount, "US$8.00");
        assert_eq!(checked.total, "US$8.00");
        assert_eq!(checked.converted, Some(("US$8.00 in AUD".into(), "A$12.00".into())));
        assert_eq!((checked.mismatch, checked.no_price), (None, None));
        // The attack, across currencies: US$8.00 over 5 XMR.
        let attack = check(&view(8.0, "USD"), 5 * XMR, Some(&aud), Some(wallet_price), Some(usd_price));
        assert_eq!(attack.mismatch.as_deref(), Some("You're paying A$1,200.00, not A$12.00. Don't send unless the till shows the same XMR."));
    }

    #[test]
    fn a_usd_wallet_names_its_own_dollar_beside_an_aud_receipt() {
        let usd = fiat::currency("USD");
        let checked = check(&view(12.0, "AUD"), XMR / 20, Some(&usd), Some(160 * fiat::MICRO), Some(240 * fiat::MICRO));
        assert_eq!(checked.total, "A$12.00");
        assert_eq!(checked.converted, Some(("A$12.00 in USD".into(), "US$8.00".into())));
    }

    #[test]
    fn without_a_price_in_the_receipt_s_currency_it_says_so_quietly() {
        let aud = fiat::currency("AUD");
        let checked = check(&view(8.0, "USD"), 5 * XMR, Some(&aud), Some(240 * fiat::MICRO), None);
        assert_eq!(checked.no_price.as_deref(), Some("No USD price right now"));
        assert_eq!((checked.converted, checked.mismatch), (None, None));
        // Its own currency with no price yet, offline or on a fresh install.
        let offline = check(&view(8.0, "AUD"), XMR / 20, Some(&aud), None, None);
        assert_eq!(offline.no_price.as_deref(), Some("No AUD price right now"));
    }

    #[test]
    fn xmr_only_checks_nothing() {
        let checked = check(&view(8.0, "USD"), 5 * XMR, None, None, None);
        assert_eq!(checked.total, "$8.00");
        assert_eq!((checked.other_currency, checked.converted, checked.mismatch, checked.no_price), (None, None, None, None));
    }

    #[test]
    fn other_signs_stay_as_they_are_beside_another_currency() {
        let usd = fiat::currency("USD");
        let checked = check(&view(1200.0, "JPY"), XMR / 20, Some(&usd), Some(160 * fiat::MICRO), None);
        assert_eq!(checked.total, "¥1,200");
        assert_eq!(checked.other_currency.as_deref(), Some("JPY"));
    }
}
