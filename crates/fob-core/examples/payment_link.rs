//! A stagenet payment link carrying an ORF receipt, as a Monokular till makes
//! one, for trying the app: `cargo run -p fob-core --example payment_link`.

use fob_core::{Network, keys::Keys, uri};

const RECEIPT: &str = r##"{
  "orf_version": "0.1.0",
  "receipt": { "receipt_id": "#1042", "issue_timestamp": "2026-10-31T09:12:00Z", "receipt_type": "sale" },
  "merchant": { "legal_name": "Corner Café", "country_code": "US" },
  "line_items": [
    { "description": "Flat white", "quantity": 2, "unit_price": 4.5, "line_total": 9.0 },
    { "description": "Almond croissant", "quantity": 1, "unit_price": 4.5, "line_total": 4.5 },
    { "description": "House beans, 250 g", "quantity": 1, "unit_price": 11.0, "line_total": 11.0 }
  ],
  "tax": { "tax_lines": [ { "rate": 0.1, "tax_amount": 2.23 } ] },
  "total": 24.5,
  "currency": "USD"
}"##;

fn main() {
    let shop = Keys::generate(Network::Stagenet).keys;
    let receipt: orf_codec::Receipt = serde_json::from_str(RECEIPT).unwrap();
    let uri = orf_codec::query::append(&uri::payment_uri(&shop.address(1), Some(142_100_000_000)), &receipt).unwrap();
    println!("{uri}");
}
