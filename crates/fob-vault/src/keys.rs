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

use crate::error::{Result, VaultError};
use fob_monero::error::MoneroError;
use fob_monero::network::Network;

/// Which kind of seed phrase a wallet was made from, so Backup shows the
/// same words.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SeedKind {
    /// 16 words with the wallet's birthday inside.
    Polyseed,
    /// Monero's original 25 words.
    Classic,
}

impl SeedKind {
    /// How many words its phrase has.
    pub fn words(self) -> usize {
        match self {
            SeedKind::Polyseed => 16,
            SeedKind::Classic => 25,
        }
    }
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
    let bytes = Zeroizing::new(hex::decode(hex_key.trim()).map_err(|_| VaultError::BadKey)?);
    if bytes.len() != 32 {
        return Err(VaultError::BadKey);
    }
    Scalar::read(&mut bytes.as_slice()).map(Zeroizing::new).map_err(|_| VaultError::BadKey)
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
                let seed = Polyseed::from_string(PolyLanguage::English, words.clone()).map_err(|_| VaultError::BadSeed)?;
                Ok(Restored {
                    birthday: Some(seed.birthday()),
                    keys: Keys::from_spend(network, reduce(&seed.key()), Some((SeedKind::Polyseed, seed.to_string()))),
                })
            }
            25 => {
                let seed = Seed::from_string(SeedLanguage::English, words.clone()).map_err(|_| VaultError::BadSeed)?;
                Ok(Restored {
                    birthday: None,
                    keys: Keys::from_spend(network, reduce(&seed.entropy()), Some((SeedKind::Classic, seed.to_string()))),
                })
            }
            _ => Err(VaultError::BadSeed),
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
            return Err(VaultError::BadKey);
        }
        Ok(keys)
    }

    /// A view-only wallet: the primary address and its private view key.
    pub fn view_only(network: Network, address: &str, view_hex: &str) -> Result<Keys> {
        let address = MoneroAddress::from_str(network.monero(), address.trim()).map_err(|_| VaultError::Monero(MoneroError::BadAddress(network)))?;
        if address.is_subaddress() {
            return Err(VaultError::Monero(MoneroError::BadAddress(network)));
        }
        let view = parse_scalar(view_hex)?;
        if public(&view) != address.view() {
            return Err(VaultError::BadKey);
        }
        Ok(Keys { network, spend: None, view, spend_pub: address.spend(), seed: None })
    }

    pub fn can_spend(&self) -> bool {
        self.spend.is_some()
    }

    pub fn spend(&self) -> Result<&Zeroizing<Scalar>> {
        self.spend.as_ref().ok_or(VaultError::ViewOnly)
    }

    /// The view key's bytes, for the prefilter.
    pub fn view_bytes(&self) -> Zeroizing<[u8; 32]> {
        Zeroizing::new(<[u8; 32]>::from(*self.view))
    }

    pub fn view_pair(&self) -> ViewPair {
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

    /// How many words [`seed_words`](Keys::seed_words) has, without
    /// building the phrase: `None` for a view-only wallet.
    pub fn seed_word_count(&self) -> Option<usize> {
        match &self.seed {
            Some((kind, _)) => Some(kind.words()),
            None => self.spend.as_ref().map(|_| SeedKind::Classic.words()),
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

    /// The key image an output with this one-time `key` and `key_offset`
    /// shows when spent; `None` for a view-only wallet.
    pub fn key_image(&self, key: Point, key_offset: Scalar) -> Option<[u8; 32]> {
        let spend = self.spend.as_ref()?;
        let secret = Zeroizing::new((**spend).into() + key_offset.into());
        let image = Point::from(*secret * Point::biased_hash(key.compress().to_bytes()).into());
        Some(image.compress().to_bytes())
    }
}

/// What the wallet file stores for the keys.
#[derive(Serialize, Deserialize)]
pub struct StoredKeys {
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
    /// Keeps these keys in `connection`'s keys table.
    pub(crate) fn write(&self, connection: &rusqlite::Connection) -> std::result::Result<(), rusqlite::Error> {
        let kind = self.seed.as_ref().map(|(kind, _)| match kind {
            SeedKind::Polyseed => "polyseed",
            SeedKind::Classic => "classic",
        });
        connection.execute(
            "INSERT OR REPLACE INTO keys (id, network, spend, view, spend_pub, seed_kind, seed_words) VALUES (1, ?, ?, ?, ?, ?, ?)",
            rusqlite::params![self.network.name(), self.spend, self.view, self.spend_pub, kind, self.seed.as_ref().map(|(_, words)| words)],
        )?;
        Ok(())
    }

    /// The keys kept in `connection`'s keys table.
    pub(crate) fn read(connection: &rusqlite::Connection) -> std::result::Result<StoredKeys, rusqlite::Error> {
        connection.query_row("SELECT network, spend, view, spend_pub, seed_kind, seed_words FROM keys WHERE id = 1", [], |row| {
            let unknown = |i: usize, what: &str| rusqlite::Error::FromSqlConversionFailure(i, rusqlite::types::Type::Text, format!("unknown {what}").into());
            let network = match row.get::<_, String>(0)?.as_str() {
                "mainnet" => Network::Mainnet,
                "stagenet" => Network::Stagenet,
                _ => return Err(unknown(0, "network")),
            };
            let kind: Option<String> = row.get(4)?;
            let words: Option<String> = row.get(5)?;
            let seed = match (kind.as_deref(), words) {
                (Some("polyseed"), Some(words)) => Some((SeedKind::Polyseed, words)),
                (Some("classic"), Some(words)) => Some((SeedKind::Classic, words)),
                (None, None) => None,
                _ => return Err(unknown(4, "seed")),
            };
            Ok(StoredKeys { network, spend: row.get(1)?, view: row.get(2)?, spend_pub: row.get(3)?, seed })
        })
    }

    /// Whether `other` holds the same keys.
    pub fn same_as(&self, other: &StoredKeys) -> bool {
        self.network == other.network && self.spend == other.spend && self.view == other.view && self.spend_pub == other.spend_pub && self.seed == other.seed
    }

    pub fn into_keys(self) -> Result<Keys> {
        let stored = Zeroizing::new(self);
        let view = parse_scalar(&stored.view)?;
        let spend = stored.spend.as_deref().map(parse_scalar).transpose()?;
        let spend_pub_bytes: [u8; 32] =
            hex::decode(&stored.spend_pub).ok().and_then(|b| b.try_into().ok()).ok_or(VaultError::BadKey)?;
        let spend_pub = monero_wallet::ed25519::CompressedPoint::from(spend_pub_bytes).decompress().ok_or(VaultError::BadKey)?;
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
    fn the_word_count_is_the_phrase_s_without_building_it() {
        let count = |keys: &Keys| (keys.seed_word_count(), keys.seed_words().map(|(_, w)| w.split_whitespace().count()));
        let made = Keys::generate(Network::Stagenet).keys;
        assert_eq!(count(&made), (Some(16), Some(16)));
        let classic = Keys::from_words(Network::Mainnet, CLASSIC).unwrap().keys;
        assert_eq!(count(&classic), (Some(25), Some(25)));
        // From a spend key: the 25 words that encode it.
        let spend = Keys::from_spend_key(Network::Mainnet, &classic.spend_key_hex().unwrap(), None).unwrap();
        assert_eq!(count(&spend), (Some(25), Some(25)));
        let watching = Keys::view_only(Network::Mainnet, &classic.primary_address(), &classic.view_key_hex()).unwrap();
        assert_eq!(count(&watching), (None, None));
    }

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
        assert!(matches!(Keys::from_words(Network::Mainnet, "not a seed"), Err(VaultError::BadSeed)));
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
    fn keys_survive_the_wallet_file() {
        let keys = Keys::generate(Network::Stagenet).keys;
        let stored = serde_json::to_string(&StoredKeys::from(&keys)).unwrap();
        let back = serde_json::from_str::<StoredKeys>(&stored).unwrap().into_keys().unwrap();
        assert_eq!(back.primary_address(), keys.primary_address());
        assert_eq!(back.seed_words().unwrap().1, keys.seed_words().unwrap().1);
    }

    #[test]
    fn keys_are_monero_s_in_full() {
        let keys = Keys::from_words(Network::Mainnet, CLASSIC).unwrap().keys;
        // Monero's own test vector for these words.
        let (spend, view) = (keys.spend_key_hex().unwrap(), keys.view_key_hex());
        assert_eq!(*spend, "c0af65c0dd837e666b9d0dfed62745f4df35aed7ea619b2798a709f0fe545403");
        assert_eq!(*view, "513ba91c538a5a9069e0094de90e927c0cd147fa10428ce3ac1afd49f63e3b01");
        // The public keys are the ones monero-oxide puts in the address,
        // and the secret keys times the base point.
        let address = MoneroAddress::from_str(Network::Mainnet.monero(), &keys.primary_address()).unwrap();
        assert_eq!(keys.public_view_key_hex(), hex::encode(address.view().compress().to_bytes()));
        assert_eq!(keys.public_spend_key_hex(), hex::encode(address.spend().compress().to_bytes()));
        let times_base = |hex_key: &str| hex::encode(public(&parse_scalar(hex_key).unwrap()).compress().to_bytes());
        assert_eq!(keys.public_view_key_hex(), times_base(&view));
        assert_eq!(keys.public_spend_key_hex(), times_base(&spend));
        for key in [spend.to_string(), view.to_string(), keys.public_view_key_hex(), keys.public_spend_key_hex()] {
            assert_eq!(key.len(), 64);
        }
        assert_eq!(keys.primary_address().len(), 95, "the whole address, not cut");
    }

    #[test]
    fn a_view_only_wallet_keeps_no_spend_key_even_through_its_file() {
        let full = Keys::from_words(Network::Mainnet, CLASSIC).unwrap().keys;
        let watch = Keys::view_only(Network::Mainnet, &full.primary_address(), &full.view_key_hex()).unwrap();
        assert!(watch.spend_key_hex().is_none());
        assert_eq!(watch.view_key_hex(), full.view_key_hex());
        // It still knows both public keys, from the address it was given.
        assert_eq!(watch.public_view_key_hex(), full.public_view_key_hex());
        assert_eq!(watch.public_spend_key_hex(), full.public_spend_key_hex());
        // And its file keeps no spend key or seed, so none comes back.
        let stored = serde_json::to_value(StoredKeys::from(&watch)).unwrap();
        assert!(stored["spend"].is_null() && stored["seed"].is_null());
        let back = serde_json::from_value::<StoredKeys>(stored).unwrap().into_keys().unwrap();
        assert!(back.spend_key_hex().is_none());
        assert_eq!(back.view_key_hex(), full.view_key_hex());
        assert_eq!(back.primary_address(), full.primary_address());
    }
}
