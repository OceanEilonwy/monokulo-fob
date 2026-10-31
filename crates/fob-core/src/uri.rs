//! What a QR code can hold: a payment request (`monero:`), maybe with an ORF
//! receipt, or a whole wallet (`monero_wallet:`, or bare seed words).

use monero_wallet::address::MoneroAddress;
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, percent_decode_str, utf8_percent_encode};
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
        // Shown as typed by whoever made the code: cleaned and cut.
        recipient_name: get(&pairs, &["recipient_name"]).and_then(crate::contacts::clean_name),
        description: get(&pairs, &["tx_description", "description"]).map(str::to_owned),
        receipt,
    })
}

/// What a value in a code is encoded with: everything but letters, digits
/// and `-._~` (RFC 3986's unreserved), so a space is `%20` and a `+` is
/// `%2B`, which every reader decodes the same.
const VALUE: &AsciiSet = &NON_ALPHANUMERIC.remove(b'-').remove(b'.').remove(b'_').remove(b'~');

/// A payment code for receiving: an address and, optionally, an amount and
/// a note for the payer (`tx_description`).
pub fn payment_uri(address: &str, amount: Option<u64>, note: Option<&str>) -> String {
    let mut params = vec![];
    if let Some(a) = amount.filter(|a| *a > 0) {
        params.push(format!("tx_amount={}", amount::format(a)));
    }
    if let Some(note) = note.map(str::trim).filter(|n| !n.is_empty()) {
        // Cut here too, whatever the app let through: the code must encode.
        params.push(format!("tx_description={}", utf8_percent_encode(&fit_note(note).0, VALUE)));
    }
    if params.is_empty() { format!("monero:{address}") } else { format!("monero:{address}?{}", params.join("&")) }
}

/// The most characters a note for the payer may have.
pub const NOTE_MAX_CHARS: usize = 120;
/// The most bytes a note may take in the code once percent-encoded: 120
/// characters that each need encoding. Bounds the code whatever the script
/// (an emoji is 12 bytes encoded), so the code stays at QR version 18 or
/// below at medium error correction, which reads at Receive's 220 dp.
pub const NOTE_MAX_ENCODED: usize = 360;

/// A note cut to what a payment code holds: at most [`NOTE_MAX_CHARS`]
/// characters and [`NOTE_MAX_ENCODED`] bytes encoded, at a character's end.
/// Returns the note and whether it was cut.
pub fn fit_note(note: &str) -> (String, bool) {
    let mut fitted = String::new();
    let mut encoded = 0;
    for (i, c) in note.chars().enumerate() {
        let mut buf = [0u8; 4];
        let size = utf8_percent_encode(c.encode_utf8(&mut buf), VALUE).map(str::len).sum::<usize>();
        if i >= NOTE_MAX_CHARS || encoded + size > NOTE_MAX_ENCODED {
            return (fitted, true);
        }
        encoded += size;
        fitted.push(c);
    }
    (fitted, false)
}

/// A request to be paid, as Receive shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Receive {
    /// The payment code.
    pub uri: String,
    /// The big line on the expanded code: "0.5 XMR", when there's an amount.
    pub headline: Option<String>,
    /// The small line under it: the note, when there is one.
    pub detail: Option<String>,
}

/// Receive's payment code and the words around it. `amount` and `note` as
/// typed; a blank one is left out.
pub fn receive(address: &str, amount: Option<&str>, note: Option<&str>) -> Result<Receive> {
    let amount = amount.map(str::trim).filter(|a| !a.is_empty()).map(amount::parse).transpose()?.filter(|a| *a > 0);
    let note = note.map(str::trim).filter(|n| !n.is_empty()).map(|n| fit_note(n).0);
    Ok(Receive {
        uri: payment_uri(address, amount, note.as_deref()),
        headline: amount.map(|a| format!("{} XMR", amount::format(a))),
        detail: note,
    })
}

/// A piece of an address as Receive shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddressGroup {
    pub text: String,
    /// The first, middle and last groups stand out: the ones people check.
    pub highlighted: bool,
}

/// An address in groups of five characters (19 for a 95-character address;
/// an integrated one splits the same way, its last group shorter), with the
/// first, the middle (index count ÷ 2) and the last highlighted.
pub fn address_groups(address: &str) -> Vec<AddressGroup> {
    let chars: Vec<char> = address.trim().chars().collect();
    let count = chars.len().div_ceil(5);
    chars
        .chunks(5)
        .enumerate()
        .map(|(i, group)| AddressGroup { text: group.iter().collect(), highlighted: i == 0 || i == count / 2 || i + 1 == count })
        .collect()
}

/// An address in a field, its first and last five: "84Bv9 … 99fQ2". Short
/// text is kept as it is.
pub fn short(address: &str) -> String {
    let chars: Vec<char> = address.trim().chars().collect();
    if chars.len() <= 12 {
        return chars.into_iter().collect();
    }
    let (head, tail): (String, String) = (chars[..5].iter().collect(), chars[chars.len() - 5..].iter().collect());
    format!("{head} … {tail}")
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
    fn a_code_s_name_is_decoded_trimmed_cleaned_and_cut() {
        let address = wallet().primary_address();
        let name = |value: &str| parse_payment(Network::Stagenet, &format!("monero:{address}?recipient_name={value}")).unwrap().recipient_name;
        assert_eq!(name("%20%20Sam%0A%07%20").as_deref(), Some("Sam"));
        assert_eq!(name("Sam+Smith").as_deref(), Some("Sam Smith"));
        assert_eq!(name("%0A%09"), None);
        assert_eq!(name(&"x".repeat(60)).map(|n| n.chars().count()), Some(40));
        // A direction override can't turn the name around on screen.
        assert_eq!(name("%E2%80%AEevil").as_deref(), Some("evil"));
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
        let uri = payment_uri(&address, Some(500_000_000_000), None);
        assert_eq!(uri, format!("monero:{address}?tx_amount=0.5"));
        assert_eq!(parse_payment(Network::Stagenet, &uri).unwrap().amount, Some(500_000_000_000));
        assert_eq!(payment_uri(&address, None, None), format!("monero:{address}"));
        assert_eq!(payment_uri(&address, Some(0), Some("  ")), format!("monero:{address}"));
    }

    #[test]
    fn a_note_goes_in_as_tx_description_and_comes_back_whole() {
        let address = wallet().address(3);
        let note = "Two coffees + a croissant, 50% off & café ☕ #12/b?=~";
        let uri = payment_uri(&address, Some(142_100_000_000), Some(note));
        assert_eq!(
            uri,
            format!(
                "monero:{address}?tx_amount=0.1421&tx_description=Two%20coffees%20%2B%20a%20croissant%2C%2050%25%20off%20%26%20caf%C3%A9%20%E2%98%95%20%2312%2Fb%3F%3D~"
            )
        );
        let read = parse_payment(Network::Stagenet, &uri).unwrap();
        assert_eq!((read.amount, read.description.as_deref()), (Some(142_100_000_000), Some(note)));
        let only_note = payment_uri(&address, None, Some(" Rent "));
        assert_eq!(only_note, format!("monero:{address}?tx_description=Rent"));
        assert_eq!(parse_payment(Network::Stagenet, &only_note).unwrap().description.as_deref(), Some("Rent"));
    }

    #[test]
    fn receive_writes_the_code_and_its_words() {
        let address = wallet().address(1);
        let r = receive(&address, Some(" 0.50 "), Some("Lunch")).unwrap();
        assert_eq!(r.uri, format!("monero:{address}?tx_amount=0.5&tx_description=Lunch"));
        assert_eq!((r.headline.as_deref(), r.detail.as_deref()), (Some("0.5 XMR"), Some("Lunch")));
        let bare = receive(&address, Some(""), None).unwrap();
        assert_eq!((bare.uri, bare.headline, bare.detail), (format!("monero:{address}"), None, None));
        assert!(receive(&address, Some("lots"), None).is_err());
    }

    #[test]
    fn a_note_is_cut_to_what_the_code_holds_and_the_code_always_encodes() {
        let (plain, cut) = fit_note(&"a".repeat(NOTE_MAX_CHARS));
        assert_eq!((plain.chars().count(), cut), (NOTE_MAX_CHARS, false));
        let (long, cut) = fit_note(&"a".repeat(NOTE_MAX_CHARS + 5));
        assert_eq!((long.chars().count(), cut), (NOTE_MAX_CHARS, true));
        // An emoji is 4 bytes, 12 encoded: 30 of them fill the note.
        let (emoji, cut) = fit_note(&"☕😀".repeat(40));
        assert!(cut);
        // 9 + 12 bytes a pair: 17 pairs are 357, and a ☕ more would be 366.
        assert_eq!(emoji.chars().count(), 34);
        assert!(emoji.ends_with('😀'), "cut at a character's end");

        // The longest code Fob can make: an integrated address (106), the
        // longest amount, and a note at either limit. It encodes at version
        // 18 or below, so it reads on Receive's card.
        let integrated = "5".repeat(106);
        let amount = "18446744.073709551615";
        let worst = [
            "%".repeat(NOTE_MAX_CHARS + 50),
            "😀".repeat(NOTE_MAX_CHARS),
            "é".repeat(NOTE_MAX_CHARS),
            "a".repeat(NOTE_MAX_CHARS * 2),
        ];
        for note in worst {
            let code = receive(&integrated, Some(amount), Some(&note)).unwrap();
            let encoded = code.uri.split("tx_description=").nth(1).unwrap();
            assert!(encoded.len() <= NOTE_MAX_ENCODED);
            assert_eq!(code.detail.as_deref(), Some(fit_note(&note).0.as_str()));
            let qr = crate::qr::encode(&code.uri).unwrap();
            assert!(qr.size <= 17 + 4 * 18, "version {} for {}", (qr.size - 17) / 4, code.uri.len());
        }
        // And whatever reaches `payment_uri` directly is cut too.
        assert!(payment_uri(&integrated, None, Some(&"x".repeat(500))).len() < 300);
    }

    #[test]
    fn addresses_show_in_groups_of_five_ends_and_middle_marked() {
        let address = wallet().address(1);
        assert_eq!(address.len(), 95);
        let groups = address_groups(&address);
        assert_eq!(groups.len(), 19);
        assert_eq!(groups.iter().map(|g| g.text.as_str()).collect::<String>(), address);
        let marked: Vec<usize> = groups.iter().enumerate().filter(|(_, g)| g.highlighted).map(|(i, _)| i).collect();
        assert_eq!(marked, [0, 9, 18]);
        // An integrated address: 106 characters, the last group one long.
        let integrated = "4".repeat(106);
        let groups = address_groups(&integrated);
        assert_eq!((groups.len(), groups[21].text.as_str()), (22, "4"));
        let marked: Vec<usize> = groups.iter().enumerate().filter(|(_, g)| g.highlighted).map(|(i, _)| i).collect();
        assert_eq!(marked, [0, 11, 21]);
    }

    #[test]
    fn an_address_in_a_field_shows_its_first_and_last_five() {
        let address = wallet().address(1);
        assert_eq!(short(&address), format!("{} … {}", &address[..5], &address[90..]));
        assert_eq!(short(" 84Bv9qqqqqqqqqqqqq99fQ2 "), "84Bv9 … 99fQ2");
        assert_eq!(short("86Ux"), "86Ux");
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
        assert_eq!(classify(Network::Stagenet, &payment_uri(&keys.address(1), None, None)), CodeKind::Payment);
        assert_eq!(classify(Network::Stagenet, &keys.seed_words().unwrap().1), CodeKind::Wallet);
        assert_eq!(classify(Network::Stagenet, "https://example.com"), CodeKind::Unknown);
    }
}
