//! ORF receipts as Fob shows them: names, lines and money already formatted.

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
    }
}
