//! A wallet's keys, and the seed phrases that hold them: `fob-keys`' own,
//! with what the wallet file stores of them.

pub use fob_keys::keys::*;

use serde::{Deserialize, Serialize};

use crate::error::{Result, VaultError};
use fob_monero::network::Network;

impl From<fob_keys::KeyError> for VaultError {
    fn from(e: fob_keys::KeyError) -> Self {
        match e {
            fob_keys::KeyError::BadSeed => VaultError::BadSeed,
            fob_keys::KeyError::BadKey => VaultError::BadKey,
            fob_keys::KeyError::ViewOnly => VaultError::ViewOnly,
            fob_keys::KeyError::Monero(e) => VaultError::Monero(e),
        }
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
    #[serde(default)]
    device: Option<Device>,
}

impl From<&Keys> for StoredKeys {
    fn from(keys: &Keys) -> Self {
        StoredKeys {
            network: keys.network,
            spend: keys.spend_key_hex().map(|s| s.to_string()),
            view: keys.view_key_hex().to_string(),
            spend_pub: keys.public_spend_key_hex(),
            // Only a seeded wallet has its words kept; one from a spend key shows
            // the 25 words that encode it, made again each time.
            seed: keys.kept_seed_words().map(|(kind, words)| (kind, words.to_string())),
            device: keys.device(),
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
        let device = self.device.map(|d| match d {
            Device::Trezor => "trezor",
        });
        connection.execute(
            "INSERT OR REPLACE INTO keys (id, network, spend, view, spend_pub, seed_kind, seed_words, device) VALUES (1, ?, ?, ?, ?, ?, ?, ?)",
            rusqlite::params![self.network.name(), self.spend, self.view, self.spend_pub, kind, self.seed.as_ref().map(|(_, words)| words), device],
        )?;
        Ok(())
    }

    /// The keys kept in `connection`'s keys table.
    pub(crate) fn read(connection: &rusqlite::Connection) -> std::result::Result<StoredKeys, rusqlite::Error> {
        connection.query_row("SELECT network, spend, view, spend_pub, seed_kind, seed_words, device FROM keys WHERE id = 1", [], |row| {
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
            let device = match row.get::<_, Option<String>>(6)?.as_deref() {
                None => None,
                Some("trezor") => Some(Device::Trezor),
                Some(_) => return Err(unknown(6, "device")),
            };
            Ok(StoredKeys { network, spend: row.get(1)?, view: row.get(2)?, spend_pub: row.get(3)?, seed, device })
        })
    }

    /// Whether `other` holds the same keys.
    pub fn same_as(&self, other: &StoredKeys) -> bool {
        self.network == other.network && self.spend == other.spend && self.view == other.view && self.spend_pub == other.spend_pub && self.seed == other.seed && self.device == other.device
    }

    pub fn into_keys(self) -> Result<Keys> {
        let seed = self.seed.as_ref().map(|(kind, words)| (*kind, words.as_str()));
        Ok(Keys::from_parts(self.network, &self.view, self.spend.as_deref(), &self.spend_pub, seed, self.device)?)
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

// Every StoredKeys is wiped when it goes, including the short-lived ones
// made to write the keys or compare them with another wallet's.
impl Drop for StoredKeys {
    fn drop(&mut self) {
        zeroize::Zeroize::zeroize(self);
    }
}

impl zeroize::ZeroizeOnDrop for StoredKeys {}

#[cfg(test)]
mod tests {
    use super::*;

    const CLASSIC: &str = "washing thirsty occur lectures tuesday fainted toxic adapt abnormal memoir nylon mostly building \
                           shrugged online ember northern ruby woes dauntless boil family illness inroads northern";

    fn through_the_file(keys: &Keys) -> Keys {
        serde_json::from_str::<StoredKeys>(&serde_json::to_string(&StoredKeys::from(keys)).unwrap()).unwrap().into_keys().unwrap()
    }

    #[test]
    fn keys_survive_the_wallet_file() {
        let keys = Keys::generate(Network::Stagenet).keys;
        let back = through_the_file(&keys);
        assert_eq!(back.primary_address(), keys.primary_address());
        assert_eq!(back.seed_words().unwrap().1, keys.seed_words().unwrap().1);
    }

    #[test]
    fn the_spend_key_and_words_come_back_through_the_file() {
        let keys = Keys::from_words(Network::Mainnet, CLASSIC).unwrap().keys;
        let back = through_the_file(&keys);
        assert_eq!(*back.spend().unwrap(), *keys.spend().unwrap());
        assert_eq!(back.seed_words().unwrap().1, keys.seed_words().unwrap().1);
        // A wallet from a spend key keeps no words, as before.
        let bare = Keys::from_spend_key(Network::Mainnet, &keys.spend_key_hex().unwrap(), None).unwrap();
        assert!(serde_json::to_value(StoredKeys::from(&bare)).unwrap()["seed"].is_null());
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

    #[test]
    fn a_trezor_wallet_keeps_its_device_through_the_file() {
        let full = Keys::from_words(Network::Mainnet, CLASSIC).unwrap().keys;
        let view: [u8; 32] = hex::decode(&*full.view_key_hex()).unwrap().try_into().unwrap();
        let trezor = Keys::trezor(Network::Mainnet, &full.primary_address(), &view).unwrap();
        let back = through_the_file(&trezor);
        assert_eq!(back.device(), Some(Device::Trezor));
        assert!(back.can_spend());
    }

    #[test]
    fn a_bad_key_says_so_in_the_vault_s_words() {
        assert!(matches!(Keys::from_words(Network::Mainnet, "not a seed").map_err(VaultError::from), Err(VaultError::BadSeed)));
        assert!(matches!(Keys::from_spend_key(Network::Mainnet, "zz", None).map_err(VaultError::from), Err(VaultError::BadKey)));
    }
}
