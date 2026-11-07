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

/// What checking a receipt against Fob's prices found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Check {
    /// The total and what the payment is worth agree; Confirm needs nothing.
    Matches,
    /// They disagree: the warning to show and have acknowledged before paying,
    /// "The receipt says $5.00, but 5 XMR is worth $812.40 now. …".
    Mismatch(String),
    /// Fob has no price in the receipt's currency, so the total can't be
    /// checked: the softer notice to show, still acknowledged before paying,
    /// since a tampered code is exactly what would pick a currency Fob can't
    /// check.
    Unchecked(String),
}

/// Confirm's notice when Fob can't check the receipt's total.
const UNCHECKED: &str = "Fob can't check this receipt's total against its own price. Make sure the XMR amount is right before paying.";

/// Checks the receipt for paying `atomic` (written `amount_text`) against
/// `prices`, Fob's own XMR prices as (currency, price) pairs, using the one
/// in the receipt's currency. With none in that currency (another currency
/// than the user's, or no price yet) the receipt is unchecked, never taken
/// as matching.
pub fn check(view: &ReceiptView, atomic: u64, amount_text: &str, prices: &[(fiat::Currency, u64)]) -> Check {
    let compared = prices.iter().find_map(|(currency, price)| Some((disagrees(view, atomic, *price, currency)?, currency, *price)));
    match compared {
        Some((false, _, _)) => Check::Matches,
        Some((true, currency, price)) => Check::Mismatch(format!(
            "The receipt says {}, but {amount_text} XMR is worth {} now. Pay only if the XMR amount is right.",
            view.total,
            fiat::value(atomic, price, currency)
        )),
        None => Check::Unchecked(UNCHECKED.into()),
    }
}

fn minor_digits(currency: &str) -> usize {
    match currency {
        "JPY" | "KRW" | "VND" | "CLP" | "ISK" | "UGX" | "XAF" | "XOF" => 0,
        "BHD" | "KWD" | "OMR" | "JOD" | "TND" | "LYD" | "IQD" => 3,
        _ => 2,
    }
}

/// `amount` in `currency`, the way people write it: "$24.50", "€9.00",
/// "CHF 12.00".
pub fn money(amount: f64, currency: &str) -> String {
    let digits = minor_digits(currency);
    let number = format!("{:.*}", digits, amount.abs());
    let sign = if amount < 0.0 { "−" } else { "" };
    match currency {
        "USD" | "CAD" | "AUD" | "NZD" => format!("{sign}${number}"),
        "EUR" => format!("{sign}€{number}"),
        "GBP" => format!("{sign}£{number}"),
        "JPY" => format!("{sign}¥{number}"),
        _ => format!("{sign}{currency} {number}"),
    }
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
        assert_eq!(money(9.0, "EUR"), "€9.00");
        assert_eq!(money(1200.0, "JPY"), "¥1200");
        assert_eq!(money(1.5, "KWD"), "KWD 1.500");
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
            lines: vec![],
            item_count: 1,
            tax: None,
            total: money(total, currency),
            currency: currency.into(),
            total_value: total,
        }
    }

    #[test]
    fn a_receipt_is_checked_against_fob_s_own_price() {
        let usd = fiat::currency("USD");
        // $160.00 per XMR.
        let price = 160 * fiat::MICRO;
        let xmr = fob_monero::amount::ATOMIC_PER_XMR;
        // 0.05 XMR is $8.00: a receipt for $8.00, or a little off, agrees.
        assert_eq!(disagrees(&view(8.0, "USD"), xmr / 20, price, &usd), Some(false));
        assert_eq!(disagrees(&view(7.80, "USD"), xmr / 20, price, &usd), Some(false));
        // A $5.00 receipt on a code asking 5 XMR ($800) is the attack.
        assert_eq!(disagrees(&view(5.0, "USD"), 5 * xmr, price, &usd), Some(true));
        // Not a total at all.
        assert_eq!(disagrees(&view(-1.0, "USD"), xmr, price, &usd), Some(true));
        assert_eq!(disagrees(&view(f64::NAN, "USD"), xmr, price, &usd), Some(true));
        // Another currency can't be compared at a USD price.
        assert_eq!(disagrees(&view(5.0, "EUR"), 5 * xmr, price, &usd), None);
    }

    #[test]
    fn a_matching_receipt_needs_nothing() {
        let usd = fiat::currency("USD");
        let prices = [(usd, 160 * fiat::MICRO)];
        let xmr = fob_monero::amount::ATOMIC_PER_XMR;
        assert_eq!(check(&view(8.0, "USD"), xmr / 20, "0.05", &prices), Check::Matches);
    }

    #[test]
    fn a_receipt_far_from_the_price_is_a_mismatch() {
        let usd = fiat::currency("USD");
        let prices = [(usd, 160 * fiat::MICRO)];
        let xmr = fob_monero::amount::ATOMIC_PER_XMR;
        let Check::Mismatch(warning) = check(&view(5.0, "USD"), 5 * xmr, "5", &prices) else { panic!("not a mismatch") };
        assert!(warning.starts_with("The receipt says $5.00, but 5 XMR is worth $800.00 now."), "{warning}");
    }

    #[test]
    fn a_receipt_fob_has_no_price_for_is_unchecked() {
        let usd = fiat::currency("USD");
        let eur = fiat::currency("EUR");
        let xmr = fob_monero::amount::ATOMIC_PER_XMR;
        // "€5.00" over 5 XMR, with only a USD price: not a match, and not
        // passed over either.
        let usd_only = [(usd.clone(), 160 * fiat::MICRO)];
        assert!(matches!(check(&view(5.0, "EUR"), 5 * xmr, "5", &usd_only), Check::Unchecked(_)));
        // No price at all, offline or on a fresh install.
        assert!(matches!(check(&view(8.0, "USD"), xmr / 20, "0.05", &[]), Check::Unchecked(_)));
        // With a EUR price as well, the same receipt is checked in euros.
        let both = [(usd, 160 * fiat::MICRO), (eur, 150 * fiat::MICRO)];
        let Check::Mismatch(warning) = check(&view(5.0, "EUR"), 5 * xmr, "5", &both) else { panic!("not a mismatch") };
        assert!(warning.starts_with("The receipt says €5.00, but 5 XMR is worth €750.00 now."), "{warning}");
        assert_eq!(check(&view(7.5, "EUR"), xmr / 20, "0.05", &both), Check::Matches);
    }
}
