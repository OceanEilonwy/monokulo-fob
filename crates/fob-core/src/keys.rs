//! A wallet's keys, and the seed phrases that hold them.

use curve25519_dalek::constants::ED25519_BASEPOINT_TABLE;
use monero_seed::{Language as SeedLanguage, Seed};
use monero_wallet::{
    ViewPair,
    address::{MoneroAddress, SubaddressIndex},
    ed25519::{Point, Scalar},
};
use polyseed::{Language as PolyLanguage, Polyseed};
use rand_core::OsRng;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::error::{FobError, Result};
use crate::network::Network;

/// Which kind of seed phrase a wallet was made from, so Backup shows the
/// same words.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SeedKind {
    /// 16 words with the wallet's birthday inside.
    Polyseed,
    /// Monero's original 25 words.
    Classic,
}

/// The secrets a wallet holds. A view-only wallet has no spend key.
#[derive(Clone)]
pub struct Keys {
    pub network: Network,
    spend: Option<Zeroizing<Scalar>>,
    view: Zeroizing<Scalar>,
    spend_pub: Point,
    /// The seed phrase, kept so Backup can show it again.
    seed: Option<(SeedKind, Zeroizing<String>)>,
}

/// What restoring from words found: the keys and, for a Polyseed, when the
/// wallet was made.
pub struct Restored {
    pub keys: Keys,
    pub birthday: Option<u64>,
}

fn reduce(bytes: &[u8; 32]) -> Scalar {
    Scalar::from(curve25519_dalek::Scalar::from_bytes_mod_order(*bytes))
}

fn public(secret: &Scalar) -> Point {
    Point::from(&(*secret).into() * ED25519_BASEPOINT_TABLE)
}

fn scalar_bytes(s: &Scalar) -> Zeroizing<[u8; 32]> {
    Zeroizing::new(<[u8; 32]>::from(*s))
}

/// A 32-byte hex key as written in wallet codes; refuses non-canonical ones.
fn parse_scalar(hex_key: &str) -> Result<Zeroizing<Scalar>> {
    let bytes = Zeroizing::new(hex::decode(hex_key.trim()).map_err(|_| FobError::BadKey)?);
    if bytes.len() != 32 {
        return Err(FobError::BadKey);
    }
    Scalar::read(&mut bytes.as_slice()).map(Zeroizing::new).map_err(|_| FobError::BadKey)
}

impl Keys {
    fn from_spend(network: Network, spend: Scalar, seed: Option<(SeedKind, Zeroizing<String>)>) -> Keys {
        let view = Scalar::hash(*scalar_bytes(&spend));
        Keys { network, spend_pub: public(&spend), spend: Some(Zeroizing::new(spend)), view: Zeroizing::new(view), seed }
    }

    /// A brand new wallet, with a 16-word Polyseed. Returns its birthday too.
    pub fn generate(network: Network) -> Restored {
        let seed = Polyseed::new(&mut OsRng, PolyLanguage::English);
        let spend = reduce(&seed.key());
        Restored {
            birthday: Some(seed.birthday()),
            keys: Keys::from_spend(network, spend, Some((SeedKind::Polyseed, seed.to_string()))),
        }
    }

    /// Words someone typed or scanned: a 16-word Polyseed or a 25-word seed,
    /// in English.
    pub fn from_words(network: Network, words: &str) -> Result<Restored> {
        let words = Zeroizing::new(words.split_whitespace().map(str::to_lowercase).collect::<Vec<_>>().join(" "));
        match words.split(' ').count() {
            16 => {
                let seed = Polyseed::from_string(PolyLanguage::English, words.clone()).map_err(|_| FobError::BadSeed)?;
                Ok(Restored {
                    birthday: Some(seed.birthday()),
                    keys: Keys::from_spend(network, reduce(&seed.key()), Some((SeedKind::Polyseed, seed.to_string()))),
                })
            }
            25 => {
                let seed = Seed::from_string(SeedLanguage::English, words.clone()).map_err(|_| FobError::BadSeed)?;
                Ok(Restored {
                    birthday: None,
                    keys: Keys::from_spend(network, reduce(&seed.entropy()), Some((SeedKind::Classic, seed.to_string()))),
                })
            }
            _ => Err(FobError::BadSeed),
        }
    }

    /// A private spend key in hex, as in a wallet code. The view key follows
    /// from it; if the code gave one too, it must match.
    pub fn from_spend_key(network: Network, spend_hex: &str, view_hex: Option<&str>) -> Result<Keys> {
        let spend = parse_scalar(spend_hex)?;
        let keys = Keys::from_spend(network, *spend, None);
        if let Some(view_hex) = view_hex
            && *parse_scalar(view_hex)? != *keys.view
        {
            return Err(FobError::BadKey);
        }
        Ok(keys)
    }

    /// A view-only wallet: the primary address and its private view key.
    pub fn view_only(network: Network, address: &str, view_hex: &str) -> Result<Keys> {
        let address = MoneroAddress::from_str(network.monero(), address.trim()).map_err(|_| FobError::BadAddress(network.name()))?;
        if address.is_subaddress() {
            return Err(FobError::BadAddress(network.name()));
        }
        let view = parse_scalar(view_hex)?;
        if public(&view) != address.view() {
            return Err(FobError::BadKey);
        }
        Ok(Keys { network, spend: None, view, spend_pub: address.spend(), seed: None })
    }

    pub fn can_spend(&self) -> bool {
        self.spend.is_some()
    }

    pub(crate) fn spend(&self) -> Result<&Zeroizing<Scalar>> {
        self.spend.as_ref().ok_or(FobError::ViewOnly)
    }

    pub(crate) fn view_pair(&self) -> ViewPair {
        ViewPair::new(self.spend_pub, self.view.clone()).expect("a wallet's own keys make a view pair")
    }

    pub fn primary_address(&self) -> String {
        self.view_pair().legacy_address(self.network.monero()).to_string()
    }

    /// Subaddress `index` of account 0; index 0 is the primary address.
    pub fn address(&self, index: u32) -> String {
        match SubaddressIndex::new(0, index) {
            Some(sub) => self.view_pair().subaddress(self.network.monero(), sub).to_string(),
            None => self.primary_address(),
        }
    }

    /// The seed phrase to show in Backup. A wallet restored from a spend key
    /// still has 25 words: they encode the same key.
    pub fn seed_words(&self) -> Option<(SeedKind, Zeroizing<String>)> {
        if let Some(seed) = &self.seed {
            return Some(seed.clone());
        }
        let spend = self.spend.as_ref()?;
        Seed::from_entropy(SeedLanguage::English, scalar_bytes(spend)).map(|s| (SeedKind::Classic, s.to_string()))
    }

    pub fn spend_key_hex(&self) -> Option<Zeroizing<String>> {
        self.spend.as_ref().map(|s| Zeroizing::new(hex::encode(*scalar_bytes(s))))
    }

    pub fn view_key_hex(&self) -> Zeroizing<String> {
        Zeroizing::new(hex::encode(*scalar_bytes(&self.view)))
    }

    /// The public spend key, as the wallet's address carries it.
    pub fn public_spend_key_hex(&self) -> String {
        hex::encode(self.spend_pub.compress().to_bytes())
    }

    /// The public view key, as the wallet's address carries it.
    pub fn public_view_key_hex(&self) -> String {
        hex::encode(public(&self.view).compress().to_bytes())
    }

    /// Keys' page (board KeysDetail): the secret keys first, each saying
    /// what it lets someone do, then the public ones. The restore height
    /// shows its `date` when there is one ("1690400 · 2 Sep 2026"), and
    /// copies as the number alone. A view-only wallet has no secret spend
    /// key row.
    pub fn page(&self, restore_height: u64, date: Option<String>) -> Vec<KeyGroup> {
        let row = |title: &str, badge: KeyBadge, line: Option<&str>, value: Zeroizing<String>| KeyRow {
            title: title.to_owned(),
            badge,
            badge_text: badge.text().to_owned(),
            line: line.map(str::to_owned),
            clip: value.clone(),
            value,
            copy: format!("Copy the {}", title.to_lowercase()),
        };
        let restore = match date {
            Some(date) => format!("{restore_height} · {date}"),
            None => restore_height.to_string(),
        };
        let mut height = row("Restore height", KeyBadge::Public, Some("Where a restore starts looking."), Zeroizing::new(restore));
        height.clip = Zeroizing::new(restore_height.to_string());
        let mut secret = Vec::new();
        if let Some(spend) = self.spend_key_hex() {
            secret.push(row("Secret spend key", KeyBadge::SpendsMoney, Some("With the view key, restores this wallet anywhere."), spend));
        }
        secret.push(row("Secret view key", KeyBadge::SeesPayments, Some("Lets a watch-only wallet see what arrives."), self.view_key_hex()));
        let public = vec![
            row("Main address", KeyBadge::Public, Some("Your first address; Fob gives out new ones each time."), Zeroizing::new(self.primary_address())),
            height,
            row("Public view key", KeyBadge::Public, None, Zeroizing::new(self.public_view_key_hex())),
            row("Public spend key", KeyBadge::Public, None, Zeroizing::new(self.public_spend_key_hex())),
        ];
        vec![KeyGroup { title: "Secret: keep to yourself".into(), keys: secret }, KeyGroup { title: "Public".into(), keys: public }]
    }

    /// The key image an output with this one-time `key` and `key_offset`
    /// shows when spent; `None` for a view-only wallet.
    pub(crate) fn key_image(&self, key: Point, key_offset: Scalar) -> Option<[u8; 32]> {
        let spend = self.spend.as_ref()?;
        let secret = Zeroizing::new((**spend).into() + key_offset.into());
        let image = Point::from(*secret * Point::biased_hash(key.compress().to_bytes()).into());
        Some(image.compress().to_bytes())
    }
}

/// What a key's badge says it lets someone do, which also sets its colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyBadge {
    SpendsMoney,
    SeesPayments,
    Public,
}

impl KeyBadge {
    fn text(self) -> &'static str {
        match self {
            KeyBadge::SpendsMoney => "Spends money",
            KeyBadge::SeesPayments => "Sees payments",
            KeyBadge::Public => "Public",
        }
    }
}

/// One key on Keys' page, ready to draw.
#[derive(Clone, Debug, PartialEq)]
pub struct KeyRow {
    /// "Secret spend key", "Main address"
    pub title: String,
    pub badge: KeyBadge,
    /// "Spends money", "Sees payments", "Public"
    pub badge_text: String,
    /// What it's for; none for the public keys.
    pub line: Option<String>,
    /// In full: 64 hex digits, the whole address, "1690400 · 2 Sep 2026".
    pub value: Zeroizing<String>,
    /// What Copy puts on the clipboard: the value, but the restore height
    /// without its date, as other wallets take it.
    pub clip: Zeroizing<String>,
    /// The Copy button's label: "Copy the secret spend key".
    pub copy: String,
}

/// A card of keys under its heading: "Secret: keep to yourself", "Public".
#[derive(Clone, Debug, PartialEq)]
pub struct KeyGroup {
    pub title: String,
    pub keys: Vec<KeyRow>,
}

/// Where a wallet's seed phrase stands, for Backup and keys' Seed phrase
/// row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backed {
    /// Made here and said to be written down.
    Up,
    /// Made here and not written down yet: Home reminds.
    Pending,
    /// Restored: the words were already somewhere, so nothing is said.
    Restored,
}

/// What a row of Backup and keys opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HubItem {
    SeedPhrase,
    WalletCode,
    Keys,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HubRow {
    pub item: HubItem,
    pub title: String,
    pub line: String,
}

/// Backup and keys (board KeysHub), every word ready to draw, and the line
/// for its row in Settings.
#[derive(Clone, Debug, PartialEq)]
pub struct Hub {
    pub warning: String,
    /// Under "Back up": Seed phrase (not on a view-only wallet), Wallet code.
    pub backup: Vec<HubRow>,
    /// Under "For other wallets": Keys.
    pub other: Vec<HubRow>,
    pub footer: String,
    /// Settings' Backup and keys row: "Seed phrase, wallet code, keys".
    pub settings_line: String,
}

/// Backup and keys for a wallet whose seed phrase has `words` words (none
/// for a view-only wallet), written down or not as `backed` says, on a
/// phone with a screen lock set or not (`phone_lock`: without one, Backup
/// and keys opens straight away, so the warning doesn't promise a lock).
pub fn hub(words: Option<usize>, backed: Backed, phone_lock: bool) -> Hub {
    let row = |item, title: &str, line: String| HubRow { item, title: title.to_owned(), line };
    let mut backup = Vec::new();
    if let Some(n) = words {
        let state = match backed {
            Backed::Up => " · backed up",
            Backed::Pending => " · not written down yet",
            Backed::Restored => "",
        };
        backup.push(row(HubItem::SeedPhrase, "Seed phrase", format!("{n} words that restore everything{state}")));
    }
    backup.push(row(HubItem::WalletCode, "Wallet code", "A QR code another phone can scan".into()));
    let other = vec![row(HubItem::Keys, "Keys", "View and spend keys, address, restore height".into())];
    let spends = words.is_some();
    let who = if spends { "Anyone who sees these can take or watch your money." } else { "Anyone who sees these can watch your money." };
    let hides = if phone_lock {
        "Fob hides them from screenshots and asks for your phone's lock first."
    } else {
        "Fob hides them from screenshots."
    };
    Hub {
        warning: format!("{who} {hides}"),
        backup,
        other,
        footer: if spends {
            "The seed phrase is all you need. The code and keys are for moving this wallet to another app."
        } else {
            "This wallet only watches. The code and keys are for moving it to another app."
        }
        .into(),
        settings_line: if spends { "Seed phrase, wallet code, keys" } else { "Wallet code, keys" }.into(),
    }
}

/// What the wallet file stores for the keys.
#[derive(Serialize, Deserialize)]
pub(crate) struct StoredKeys {
    network: Network,
    spend: Option<String>,
    view: String,
    spend_pub: String,
    seed: Option<(SeedKind, String)>,
}

impl From<&Keys> for StoredKeys {
    fn from(keys: &Keys) -> Self {
        StoredKeys {
            network: keys.network,
            spend: keys.spend_key_hex().map(|s| s.to_string()),
            view: keys.view_key_hex().to_string(),
            spend_pub: keys.public_spend_key_hex(),
            seed: keys.seed.as_ref().map(|(kind, words)| (*kind, words.to_string())),
        }
    }
}

impl StoredKeys {
    pub(crate) fn into_keys(self) -> Result<Keys> {
        let stored = Zeroizing::new(self);
        let view = parse_scalar(&stored.view)?;
        let spend = stored.spend.as_deref().map(parse_scalar).transpose()?;
        let spend_pub_bytes: [u8; 32] =
            hex::decode(&stored.spend_pub).ok().and_then(|b| b.try_into().ok()).ok_or(FobError::BadKey)?;
        let spend_pub = monero_wallet::ed25519::CompressedPoint::from(spend_pub_bytes).decompress().ok_or(FobError::BadKey)?;
        Ok(Keys {
            network: stored.network,
            spend,
            view,
            spend_pub,
            seed: stored.seed.as_ref().map(|(kind, words)| (*kind, Zeroizing::new(words.clone()))),
        })
    }
}

impl zeroize::Zeroize for StoredKeys {
    fn zeroize(&mut self) {
        self.spend.zeroize();
        self.view.zeroize();
        if let Some((_, words)) = &mut self.seed {
            words.zeroize();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLASSIC: &str = "washing thirsty occur lectures tuesday fainted toxic adapt abnormal memoir nylon mostly building \
                           shrugged online ember northern ruby woes dauntless boil family illness inroads northern";

    #[test]
    fn a_classic_seed_gives_monero_s_keys() {
        let keys = Keys::from_words(Network::Mainnet, CLASSIC).unwrap().keys;
        assert_eq!(*keys.spend_key_hex().unwrap(), "c0af65c0dd837e666b9d0dfed62745f4df35aed7ea619b2798a709f0fe545403");
        assert_eq!(*keys.view_key_hex(), "513ba91c538a5a9069e0094de90e927c0cd147fa10428ce3ac1afd49f63e3b01");
        assert_eq!(keys.seed_words().unwrap().0, SeedKind::Classic);
    }

    #[test]
    fn seed_words_are_read_whatever_the_spacing_and_case() {
        let messy = format!("  {}\n", CLASSIC.to_uppercase().replace(' ', "   "));
        let keys = Keys::from_words(Network::Mainnet, &messy).unwrap().keys;
        assert_eq!(*keys.view_key_hex(), "513ba91c538a5a9069e0094de90e927c0cd147fa10428ce3ac1afd49f63e3b01");
    }

    #[test]
    fn a_new_wallet_restores_from_its_own_polyseed() {
        let made = Keys::generate(Network::Stagenet);
        let (kind, words) = made.keys.seed_words().unwrap();
        assert_eq!(kind, SeedKind::Polyseed);
        assert_eq!(words.split(' ').count(), 16);
        let back = Keys::from_words(Network::Stagenet, &words).unwrap();
        assert_eq!(back.keys.primary_address(), made.keys.primary_address());
        assert_eq!(back.birthday, made.birthday);
        assert!(made.keys.primary_address().starts_with('5'), "stagenet addresses start with 5");
    }

    #[test]
    fn wrong_words_are_refused() {
        assert!(matches!(Keys::from_words(Network::Mainnet, "not a seed"), Err(FobError::BadSeed)));
        let swapped = CLASSIC.replacen("washing", "thirsty", 1);
        assert!(Keys::from_words(Network::Mainnet, &swapped).is_err());
    }

    #[test]
    fn a_spend_key_restores_the_same_wallet_and_checks_the_view_key() {
        let seeded = Keys::from_words(Network::Mainnet, CLASSIC).unwrap().keys;
        let spend = seeded.spend_key_hex().unwrap();
        let keys = Keys::from_spend_key(Network::Mainnet, &spend, Some(&seeded.view_key_hex())).unwrap();
        assert_eq!(keys.primary_address(), seeded.primary_address());
        assert_eq!(keys.seed_words().unwrap().1, seeded.seed_words().unwrap().1);
        let wrong_view = "00".repeat(32);
        assert!(Keys::from_spend_key(Network::Mainnet, &spend, Some(&wrong_view)).is_err());
    }

    #[test]
    fn a_view_only_wallet_sees_the_same_address_but_cannot_spend() {
        let full = Keys::from_words(Network::Mainnet, CLASSIC).unwrap().keys;
        let watch = Keys::view_only(Network::Mainnet, &full.primary_address(), &full.view_key_hex()).unwrap();
        assert_eq!(watch.primary_address(), full.primary_address());
        assert_eq!(watch.address(3), full.address(3));
        assert!(!watch.can_spend());
        assert!(watch.seed_words().is_none());
        let other = Keys::generate(Network::Mainnet).keys;
        assert!(Keys::view_only(Network::Mainnet, &full.primary_address(), &other.view_key_hex()).is_err());
    }

    #[test]
    fn subaddresses_differ_from_the_primary_address() {
        let keys = Keys::generate(Network::Stagenet).keys;
        assert_eq!(keys.address(0), keys.primary_address());
        assert_ne!(keys.address(1), keys.primary_address());
        assert_ne!(keys.address(1), keys.address(2));
    }

    #[test]
    fn keys_page_shows_monero_s_keys_in_full() {
        let keys = Keys::from_words(Network::Mainnet, CLASSIC).unwrap().keys;
        let page = keys.page(1_690_400, Some("2 Sep 2026".into()));
        let titles: Vec<Vec<&str>> = page.iter().map(|g| g.keys.iter().map(|k| k.title.as_str()).collect()).collect();
        assert_eq!(page[0].title, "Secret: keep to yourself");
        assert_eq!(page[1].title, "Public");
        assert_eq!(
            titles,
            [vec!["Secret spend key", "Secret view key"], vec!["Main address", "Restore height", "Public view key", "Public spend key"]]
        );
        let value = |g: usize, k: usize| page[g].keys[k].value.to_string();
        // Monero's own test vector for these words.
        assert_eq!(value(0, 0), "c0af65c0dd837e666b9d0dfed62745f4df35aed7ea619b2798a709f0fe545403");
        assert_eq!(value(0, 1), "513ba91c538a5a9069e0094de90e927c0cd147fa10428ce3ac1afd49f63e3b01");
        // The public keys are the ones monero-oxide puts in the address,
        // and the secret keys times the base point.
        let address = MoneroAddress::from_str(Network::Mainnet.monero(), &value(1, 0)).unwrap();
        assert_eq!(value(1, 0), keys.primary_address());
        assert_eq!(value(1, 2), hex::encode(address.view().compress().to_bytes()));
        assert_eq!(value(1, 3), hex::encode(address.spend().compress().to_bytes()));
        let times_base = |hex_key: &str| hex::encode(public(&parse_scalar(hex_key).unwrap()).compress().to_bytes());
        assert_eq!(value(1, 2), times_base(&value(0, 1)));
        assert_eq!(value(1, 3), times_base(&value(0, 0)));
        assert_eq!(value(1, 1), "1690400 · 2 Sep 2026");
        assert_eq!(*page[1].keys[1].clip, "1690400", "copied as other wallets take it");
        for key in [value(0, 0), value(0, 1), value(1, 2), value(1, 3)] {
            assert_eq!(key.len(), 64);
        }
        for (g, k) in [(0, 0), (0, 1), (1, 0), (1, 2), (1, 3)] {
            assert_eq!(page[g].keys[k].clip, page[g].keys[k].value);
        }
        assert_eq!(*keys.page(1_690_400, None)[1].keys[1].value, "1690400");
        assert_eq!(value(1, 0).len(), 95, "the whole address, not cut");
    }

    #[test]
    fn keys_page_words() {
        let keys = Keys::from_words(Network::Mainnet, CLASSIC).unwrap().keys;
        let page = keys.page(0, None);
        let spend = &page[0].keys[0];
        assert_eq!((spend.badge, spend.badge_text.as_str()), (KeyBadge::SpendsMoney, "Spends money"));
        assert_eq!(spend.line.as_deref(), Some("With the view key, restores this wallet anywhere."));
        assert_eq!(spend.copy, "Copy the secret spend key");
        let view = &page[0].keys[1];
        assert_eq!((view.badge, view.badge_text.as_str()), (KeyBadge::SeesPayments, "Sees payments"));
        assert_eq!(view.line.as_deref(), Some("Lets a watch-only wallet see what arrives."));
        let main = &page[1].keys[0];
        assert_eq!((main.badge, main.badge_text.as_str()), (KeyBadge::Public, "Public"));
        assert_eq!(main.line.as_deref(), Some("Your first address; Fob gives out new ones each time."));
        assert_eq!(main.copy, "Copy the main address");
        assert_eq!(page[1].keys[1].line.as_deref(), Some("Where a restore starts looking."));
        assert_eq!(page[1].keys[2].line, None);
        assert_eq!(page[1].keys[3].copy, "Copy the public spend key");
    }

    #[test]
    fn a_view_only_wallet_s_page_has_no_secret_spend_key() {
        let full = Keys::from_words(Network::Mainnet, CLASSIC).unwrap().keys;
        let watch = Keys::view_only(Network::Mainnet, &full.primary_address(), &full.view_key_hex()).unwrap();
        let page = watch.page(3_000_000, None);
        assert_eq!(page[0].keys.len(), 1);
        assert_eq!(page[0].keys[0].title, "Secret view key");
        assert_eq!(page[0].keys[0].value, full.view_key_hex());
        // It still knows both public keys, from the address it was given.
        let full_page = full.page(3_000_000, None);
        assert_eq!(page[1], full_page[1]);
        // And its file keeps no spend key or seed, so none comes back.
        let stored = serde_json::to_value(StoredKeys::from(&watch)).unwrap();
        assert!(stored["spend"].is_null() && stored["seed"].is_null());
        let back = serde_json::from_value::<StoredKeys>(stored).unwrap().into_keys().unwrap();
        assert_eq!(back.page(3_000_000, None), page);
    }

    #[test]
    fn the_hub_says_how_many_words_and_whether_they_are_written_down() {
        let made = hub(Some(16), Backed::Up, true);
        assert_eq!(made.backup.iter().map(|r| r.item).collect::<Vec<_>>(), [HubItem::SeedPhrase, HubItem::WalletCode]);
        assert_eq!(made.backup[0].title, "Seed phrase");
        assert_eq!(made.backup[0].line, "16 words that restore everything · backed up");
        assert_eq!(made.backup[1].line, "A QR code another phone can scan");
        assert_eq!(made.other[0].item, HubItem::Keys);
        assert_eq!(made.other[0].line, "View and spend keys, address, restore height");
        assert_eq!(made.footer, "The seed phrase is all you need. The code and keys are for moving this wallet to another app.");
        assert_eq!(made.settings_line, "Seed phrase, wallet code, keys");
        assert_eq!(hub(Some(16), Backed::Pending, true).backup[0].line, "16 words that restore everything · not written down yet");
        assert_eq!(hub(Some(25), Backed::Restored, true).backup[0].line, "25 words that restore everything");
    }

    #[test]
    fn the_hub_s_warning_promises_the_phone_s_lock_only_when_one_is_set() {
        // Any lock (PIN, pattern, fingerprint, face) is asked for, so the
        // words name none in particular.
        assert_eq!(
            hub(Some(16), Backed::Up, true).warning,
            "Anyone who sees these can take or watch your money. Fob hides them from screenshots and asks for your phone's lock first."
        );
        assert_eq!(
            hub(None, Backed::Restored, true).warning,
            "Anyone who sees these can watch your money. Fob hides them from screenshots and asks for your phone's lock first."
        );
        // No lock set: Backup and keys opens straight away, so nothing is promised.
        assert_eq!(
            hub(Some(16), Backed::Up, false).warning,
            "Anyone who sees these can take or watch your money. Fob hides them from screenshots."
        );
        assert_eq!(hub(None, Backed::Restored, false).warning, "Anyone who sees these can watch your money. Fob hides them from screenshots.");
    }

    #[test]
    fn a_view_only_wallet_s_hub_has_no_seed_phrase() {
        let watch = hub(None, Backed::Restored, true);
        assert_eq!(watch.backup.iter().map(|r| r.item).collect::<Vec<_>>(), [HubItem::WalletCode]);
        assert_eq!(watch.other.len(), 1);
        assert!(watch.warning.starts_with("Anyone who sees these can watch your money."));
        assert_eq!(watch.footer, "This wallet only watches. The code and keys are for moving it to another app.");
        assert_eq!(watch.settings_line, "Wallet code, keys");
    }

    #[test]
    fn keys_survive_the_wallet_file() {
        let keys = Keys::generate(Network::Stagenet).keys;
        let stored = serde_json::to_string(&StoredKeys::from(&keys)).unwrap();
        let back = serde_json::from_str::<StoredKeys>(&stored).unwrap().into_keys().unwrap();
        assert_eq!(back.primary_address(), keys.primary_address());
        assert_eq!(back.seed_words().unwrap().1, keys.seed_words().unwrap().1);
    }
}
