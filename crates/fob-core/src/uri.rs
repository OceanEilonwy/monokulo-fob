//! What a QR code can hold: a payment request (`monero:`), maybe with an ORF
//! receipt, or a whole wallet (`monero_wallet:`, or bare seed words).

use monero_wallet::address::MoneroAddress;
use percent_encoding::{NON_ALPHANUMERIC, percent_decode_str, utf8_percent_encode};
use zeroize::Zeroizing;

use crate::amount;

pub use orf_codec::Receipt;
use crate::error::{FobError, Result};
use crate::network::Network;

/// A request to pay, read from a payment code.
#[derive(Clone, Debug, PartialEq)]
pub struct PaymentRequest {
    pub address: String,
    pub amount: Option<u64>,
    pub recipient_name: Option<String>,
    pub description: Option<String>,
    /// The ORF receipt the code carried, if any.
    pub receipt: Option<orf_codec::Receipt>,
}

/// A wallet read from a wallet code.
pub enum WalletCode {
    /// Seed words, from a `seed` parameter or a code that is only words.
    Words { words: Zeroizing<String>, height: Option<u64> },
    /// A spend key, with its view key when the code had one.
    SpendKey { address: Option<String>, spend: Zeroizing<String>, view: Option<Zeroizing<String>>, height: Option<u64> },
    /// An address and view key only: a wallet that watches.
    ViewOnly { address: String, view: Zeroizing<String>, height: Option<u64> },
}

fn split_scheme<'a>(text: &'a str, schemes: &[&str]) -> Option<&'a str> {
    let (scheme, rest) = text.split_once(':')?;
    schemes.iter().any(|s| scheme.eq_ignore_ascii_case(s)).then_some(rest)
}

fn decode(value: &str) -> String {
    percent_decode_str(&value.replace('+', " ")).decode_utf8_lossy().into_owned()
}

/// `path?key=value&…` as the path and decoded pairs.
fn split_query(rest: &str) -> (&str, Vec<(String, String)>) {
    let (path, query) = rest.split_once('?').unwrap_or((rest, ""));
    let pairs = query
        .split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap_or((p, ""));
            (k.to_ascii_lowercase(), decode(v))
        })
        .collect();
    (path.trim_start_matches("//"), pairs)
}

fn get<'a>(pairs: &'a [(String, String)], keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|k| pairs.iter().find(|(pk, v)| pk == k && !v.is_empty()).map(|(_, v)| v.as_str()))
}

/// The address's network, for a clear error when a code is for the other one.
fn check_address(network: Network, address: &str) -> Result<()> {
    if MoneroAddress::from_str(network.monero(), address).is_ok() {
        return Ok(());
    }
    let other = match network {
        Network::Mainnet => Network::Stagenet,
        Network::Stagenet => Network::Mainnet,
    };
    if MoneroAddress::from_str(other.monero(), address).is_ok() {
        return Err(FobError::WrongNetwork { expected: network.name(), found: other.name() });
    }
    Err(FobError::BadAddress(network.name()))
}

/// A scanned or pasted payment code: `monero:<address>?tx_amount=…&orf=…`,
/// or just an address.
pub fn parse_payment(network: Network, text: &str) -> Result<PaymentRequest> {
    let text = text.trim();
    let rest = split_scheme(text, &["monero"]).unwrap_or(text);
    let (address, pairs) = split_query(rest);
    check_address(network, address)?;
    let amount = get(&pairs, &["tx_amount", "amount"]).map(amount::parse).transpose()?;
    // A damaged receipt doesn't stop the payment: the code still pays.
    let receipt = orf_codec::query::from_uri(text).ok().flatten();
    Ok(PaymentRequest {
        address: address.to_owned(),
        amount,
        recipient_name: get(&pairs, &["recipient_name"]).map(str::to_owned),
        description: get(&pairs, &["tx_description", "description"]).map(str::to_owned),
        receipt,
    })
}

/// A payment code for receiving: an address and, optionally, an amount.
pub fn payment_uri(address: &str, amount: Option<u64>) -> String {
    match amount {
        Some(a) if a > 0 => format!("monero:{address}?tx_amount={}", amount::format(a, 0)),
        _ => format!("monero:{address}"),
    }
}

/// A wallet code: `monero_wallet:` with keys or seed (the form the Monero GUI
/// and Cake Wallet show), or a code holding only seed words.
pub fn parse_wallet_code(network: Network, text: &str) -> Result<WalletCode> {
    let text = text.trim();
    let Some(rest) = split_scheme(text, &["monero_wallet", "monero-wallet", "monerowallet"]) else {
        let words = text.split_whitespace().count();
        if words == 16 || words == 25 {
            return Ok(WalletCode::Words { words: Zeroizing::new(text.to_owned()), height: None });
        }
        return Err(FobError::BadCode);
    };
    let (address, pairs) = split_query(rest);
    let pairs = Zeroizing::new(pairs);
    let address = (!address.is_empty()).then(|| address.to_owned());
    if let Some(a) = &address {
        check_address(network, a)?;
    }
    let height = get(&pairs, &["height", "restore_height", "restoreheight"]).map(|h| h.parse().map_err(|_| FobError::BadCode)).transpose()?;
    let secret = |keys: &[&str]| get(&pairs, keys).map(|s| Zeroizing::new(s.to_owned()));
    if let Some(words) = secret(&["seed", "mnemonic_seed", "mnemonic"]) {
        return Ok(WalletCode::Words { words, height });
    }
    let view = secret(&["view_key", "viewkey"]);
    if let Some(spend) = secret(&["spend_key", "spendkey"]) {
        return Ok(WalletCode::SpendKey { address, spend, view, height });
    }
    match (address, view) {
        (Some(address), Some(view)) => Ok(WalletCode::ViewOnly { address, view, height }),
        _ => Err(FobError::BadCode),
    }
}

/// What a scanned code is, so the app can send it to the right screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CodeKind {
    Payment,
    Wallet,
    Unknown,
}

pub fn classify(network: Network, text: &str) -> CodeKind {
    if parse_wallet_code(network, text).is_ok() {
        CodeKind::Wallet
    } else if parse_payment(network, text).is_ok() {
        CodeKind::Payment
    } else {
        CodeKind::Unknown
    }
}

/// A wallet code other wallets can restore from.
pub fn wallet_code(address: &str, spend_hex: Option<&str>, view_hex: &str, height: u64) -> Zeroizing<String> {
    let mut code = format!("monero_wallet:{address}?");
    if let Some(spend) = spend_hex {
        code.push_str(&format!("spend_key={}&", utf8_percent_encode(spend, NON_ALPHANUMERIC)));
    }
    code.push_str(&format!("view_key={view_hex}&height={height}"));
    Zeroizing::new(code)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::Keys;

    fn wallet() -> Keys {
        Keys::generate(Network::Stagenet).keys
    }

    const RECEIPT: &str = r##"{
      "orf_version": "0.1.0",
      "receipt": { "receipt_id": "#1042", "issue_timestamp": "2026-10-31T09:12:00Z", "receipt_type": "sale" },
      "merchant": { "legal_name": "Corner Café", "country_code": "US" },
      "line_items": [
        { "description": "Flat white", "quantity": 2, "unit_price": 4.5, "line_total": 9.0 },
        { "description": "Almond croissant", "quantity": 1, "unit_price": 4.5, "line_total": 4.5 },
        { "description": "House beans, 250 g", "quantity": 1, "unit_price": 11.0, "line_total": 11.0 }
      ],
      "total": 24.5,
      "currency": "USD"
    }"##;

    #[test]
    fn reads_a_payment_code_with_a_receipt_like_monokular_makes() {
        let address = wallet().address(4);
        let receipt: orf_codec::Receipt = serde_json::from_str(RECEIPT).unwrap();
        let uri = orf_codec::query::append(&format!("monero:{address}?tx_amount=0.1421"), &receipt).unwrap();
        let request = parse_payment(Network::Stagenet, &uri).unwrap();
        assert_eq!(request.address, address);
        assert_eq!(request.amount, Some(142_100_000_000));
        assert_eq!(request.receipt.unwrap().merchant.legal_name, "Corner Café");
    }

    #[test]
    fn reads_a_bare_address_and_other_wallets_parameters() {
        let address = wallet().primary_address();
        let plain = parse_payment(Network::Stagenet, &format!("  {address}\n")).unwrap();
        assert_eq!((plain.amount, plain.receipt), (None, None));
        let named =
            parse_payment(Network::Stagenet, &format!("MONERO:{address}?recipient_name=Corner%20Caf%C3%A9&tx_description=Two+coffees"))
                .unwrap();
        assert_eq!(named.recipient_name.as_deref(), Some("Corner Café"));
        assert_eq!(named.description.as_deref(), Some("Two coffees"));
    }

    #[test]
    fn a_damaged_receipt_still_pays() {
        let address = wallet().primary_address();
        let request = parse_payment(Network::Stagenet, &format!("monero:{address}?tx_amount=1&orf=12")).unwrap();
        assert_eq!(request.amount, Some(amount::ATOMIC_PER_XMR));
        assert!(request.receipt.is_none());
    }

    #[test]
    fn a_mainnet_code_on_a_stagenet_wallet_says_which_network() {
        let mainnet = Keys::generate(Network::Mainnet).keys.primary_address();
        let err = parse_payment(Network::Stagenet, &format!("monero:{mainnet}")).unwrap_err();
        assert!(matches!(err, FobError::WrongNetwork { expected: "stagenet", found: "mainnet" }));
        assert!(matches!(parse_payment(Network::Stagenet, "monero:hello"), Err(FobError::BadAddress(_))));
    }

    #[test]
    fn receive_codes_round_trip() {
        let address = wallet().address(2);
        let uri = payment_uri(&address, Some(500_000_000_000));
        assert_eq!(uri, format!("monero:{address}?tx_amount=0.5"));
        assert_eq!(parse_payment(Network::Stagenet, &uri).unwrap().amount, Some(500_000_000_000));
        assert_eq!(payment_uri(&address, None), format!("monero:{address}"));
    }

    #[test]
    fn wallet_codes_round_trip_with_keys() {
        let keys = wallet();
        let code = wallet_code(&keys.primary_address(), keys.spend_key_hex().as_deref().map(|s| s.as_str()), &keys.view_key_hex(), 2_100_000);
        let WalletCode::SpendKey { address, spend, view, height } = parse_wallet_code(Network::Stagenet, &code).unwrap() else {
            panic!("not a spend-key code")
        };
        assert_eq!(address.as_deref(), Some(keys.primary_address().as_str()));
        assert_eq!(*spend, *keys.spend_key_hex().unwrap());
        assert_eq!(view.as_deref().map(|v| v.as_str()), Some(keys.view_key_hex().as_str()));
        assert_eq!(height, Some(2_100_000));
    }

    #[test]
    fn wallet_codes_with_a_seed_or_only_a_view_key() {
        let keys = wallet();
        let (_, words) = keys.seed_words().unwrap();
        let seeded = format!("monero-wallet:{}?seed={}&restore_height=12", keys.primary_address(), words.replace(' ', "%20"));
        let WalletCode::Words { words: read, height } = parse_wallet_code(Network::Stagenet, &seeded).unwrap() else { panic!() };
        assert_eq!((read.as_str(), height), (words.as_str(), Some(12)));

        let watch = format!("monero_wallet:{}?view_key={}", keys.primary_address(), keys.view_key_hex().as_str());
        assert!(matches!(parse_wallet_code(Network::Stagenet, &watch).unwrap(), WalletCode::ViewOnly { height: None, .. }));

        assert!(matches!(parse_wallet_code(Network::Stagenet, &words).unwrap(), WalletCode::Words { .. }));
        assert!(parse_wallet_code(Network::Stagenet, "monero_wallet:").is_err());
        assert!(parse_wallet_code(Network::Stagenet, "hello there").is_err());
    }

    #[test]
    fn codes_are_told_apart() {
        let keys = wallet();
        assert_eq!(classify(Network::Stagenet, &payment_uri(&keys.address(1), None)), CodeKind::Payment);
        assert_eq!(classify(Network::Stagenet, &keys.seed_words().unwrap().1), CodeKind::Wallet);
        assert_eq!(classify(Network::Stagenet, "https://example.com"), CodeKind::Unknown);
    }
}
