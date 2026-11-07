//! The wallet file key on a device with no secure hardware to keep it in: a
//! random key, sealed in `vault.json` under a key derived from a passphrase.
//!
//! A desktop keyring opens for anything running as the same user once it's
//! unlocked, which is too little for spend keys. Here the file key exists only
//! in memory, after the passphrase is typed, and changing the passphrase only
//! seals the same key again, so no wallet file has to be touched.

use std::path::{Path, PathBuf};

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::{
    ChaCha20Poly1305, KeyInit, Nonce,
    aead::{Aead, Payload},
};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::error::{Result, VaultError};

/// The shortest passphrase taken.
pub const MIN_LENGTH: usize = 8;

const FILE: &str = "vault.json";
const VERSION: u32 = 1;
/// Bound to the seal, so a sealed key can't be passed off as something else.
const CONTEXT: &[u8] = b"fob vault v1";

/// What deriving the key costs, kept in the file so it can rise later. About
/// half a second on a laptop, and 128 MiB for anyone guessing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Cost {
    memory_kib: u32,
    time: u32,
    lanes: u32,
}

const COST: Cost = Cost { memory_kib: 128 * 1024, time: 3, lanes: 1 };

#[derive(Serialize, Deserialize)]
struct Sealed {
    version: u32,
    cost: Cost,
    salt: String,
    nonce: String,
    key: String,
}

/// Whether this directory has a passphrase set up.
pub fn exists(dir: &Path) -> bool {
    path(dir).exists()
}

fn path(dir: &Path) -> PathBuf {
    dir.join(FILE)
}

/// Makes a new file key, sealed under `passphrase`, and returns it.
pub fn create(dir: &Path, passphrase: &str) -> Result<Zeroizing<[u8; 32]>> {
    create_at(dir, passphrase, COST)
}

fn create_at(dir: &Path, passphrase: &str, cost: Cost) -> Result<Zeroizing<[u8; 32]>> {
    if exists(dir) {
        return Err(VaultError::PassphraseExists);
    }
    check(passphrase)?;
    let mut key = Zeroizing::new([0u8; 32]);
    OsRng.fill_bytes(key.as_mut());
    write(dir, &seal(passphrase, &key, cost)?)?;
    Ok(key)
}

/// The file key, if `passphrase` is the right one.
pub fn unlock(dir: &Path, passphrase: &str) -> Result<Zeroizing<[u8; 32]>> {
    let text = std::fs::read(path(dir)).map_err(|e| VaultError::Storage(e.to_string()))?;
    let sealed: Sealed = serde_json::from_slice(&text).map_err(|_| VaultError::NotAWalletFile)?;
    if sealed.version != VERSION {
        return Err(VaultError::NotAWalletFile);
    }
    open(passphrase, &sealed)
}

/// Seals the same file key under `new`; wallet files don't change.
pub fn change(dir: &Path, old: &str, new: &str) -> Result<()> {
    check(new)?;
    let key = unlock(dir, old)?;
    write(dir, &seal(new, &key, COST)?)
}

fn check(passphrase: &str) -> Result<()> {
    if passphrase.chars().count() < MIN_LENGTH {
        return Err(VaultError::PassphraseTooShort { min: MIN_LENGTH });
    }
    Ok(())
}

fn derive(passphrase: &str, salt: &[u8], cost: Cost) -> Result<Zeroizing<[u8; 32]>> {
    let params = Params::new(cost.memory_kib, cost.time, cost.lanes, Some(32)).map_err(|e| VaultError::Storage(e.to_string()))?;
    let mut derived = Zeroizing::new([0u8; 32]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(passphrase.as_bytes(), salt, derived.as_mut())
        .map_err(|e| VaultError::Storage(e.to_string()))?;
    Ok(derived)
}

fn seal(passphrase: &str, key: &[u8; 32], cost: Cost) -> Result<Sealed> {
    let (mut salt, mut nonce) = ([0u8; 16], [0u8; 12]);
    OsRng.fill_bytes(&mut salt);
    OsRng.fill_bytes(&mut nonce);
    let derived = derive(passphrase, &salt, cost)?;
    let sealed = ChaCha20Poly1305::new(derived.as_ref().into())
        .encrypt(Nonce::from_slice(&nonce), Payload { msg: key, aad: CONTEXT })
        .map_err(|_| VaultError::Sealing)?;
    Ok(Sealed { version: VERSION, cost, salt: hex::encode(salt), nonce: hex::encode(nonce), key: hex::encode(sealed) })
}

fn open(passphrase: &str, sealed: &Sealed) -> Result<Zeroizing<[u8; 32]>> {
    let bad = |_| VaultError::NotAWalletFile;
    let (salt, nonce, key) = (hex::decode(&sealed.salt).map_err(bad)?, hex::decode(&sealed.nonce).map_err(bad)?, hex::decode(&sealed.key).map_err(bad)?);
    if nonce.len() != 12 {
        return Err(VaultError::NotAWalletFile);
    }
    let derived = derive(passphrase, &salt, sealed.cost)?;
    let opened = Zeroizing::new(
        ChaCha20Poly1305::new(derived.as_ref().into())
            .decrypt(Nonce::from_slice(&nonce), Payload { msg: &key, aad: CONTEXT })
            .map_err(|_| VaultError::WrongPassphrase)?,
    );
    let mut out = Zeroizing::new([0u8; 32]);
    if opened.len() != 32 {
        return Err(VaultError::NotAWalletFile);
    }
    out.copy_from_slice(&opened);
    Ok(out)
}

/// Written whole or not at all, readable only by this user.
fn write(dir: &Path, sealed: &Sealed) -> Result<()> {
    let text = serde_json::to_vec_pretty(sealed).map_err(|e| VaultError::Storage(e.to_string()))?;
    crate::files::write_private(&path(dir), &text).map_err(|e| VaultError::Storage(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cheap, so tests don't wait: the file says what it costs.
    const CHEAP: Cost = Cost { memory_kib: 64, time: 1, lanes: 1 };

    fn dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("fob-vault-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_right_passphrase_gives_the_key_and_a_wrong_one_is_told_apart() {
        let dir = dir("unlock");
        assert!(!exists(&dir));
        let key = create_at(&dir, "correct horse", CHEAP).unwrap();
        assert!(exists(&dir));
        assert_eq!(*unlock(&dir, "correct horse").unwrap(), *key);
        assert_eq!(unlock(&dir, "wrong horse!").unwrap_err(), VaultError::WrongPassphrase);
    }

    #[test]
    fn the_key_is_never_in_the_file_and_each_vault_has_its_own() {
        let (one, two) = (dir("own-1"), dir("own-2"));
        let (a, b) = (create_at(&one, "correct horse", CHEAP).unwrap(), create_at(&two, "correct horse", CHEAP).unwrap());
        assert_ne!(*a, *b);
        let text = std::fs::read_to_string(one.join(FILE)).unwrap();
        assert!(!text.contains(&hex::encode(*a)));
    }

    #[test]
    fn changing_the_passphrase_keeps_the_key() {
        let dir = dir("change");
        let key = create_at(&dir, "correct horse", CHEAP).unwrap();
        assert_eq!(change(&dir, "not the one!", "battery staple").unwrap_err(), VaultError::WrongPassphrase);
        assert_eq!(*unlock(&dir, "correct horse").unwrap(), *key, "a refused change changes nothing");
        assert_eq!(change(&dir, "correct horse", "short").unwrap_err(), VaultError::PassphraseTooShort { min: MIN_LENGTH });
        // Sealed at the real cost from here on, which this reads back.
        change(&dir, "correct horse", "battery staple").unwrap();
        assert_eq!(*unlock(&dir, "battery staple").unwrap(), *key);
        assert_eq!(unlock(&dir, "correct horse").unwrap_err(), VaultError::WrongPassphrase);
    }

    #[test]
    fn a_short_passphrase_and_a_second_vault_are_refused() {
        let dir = dir("refuse");
        assert_eq!(create_at(&dir, "short", CHEAP).unwrap_err(), VaultError::PassphraseTooShort { min: MIN_LENGTH });
        assert!(!exists(&dir));
        create_at(&dir, "correct horse", CHEAP).unwrap();
        assert_eq!(create_at(&dir, "correct horse", CHEAP).unwrap_err(), VaultError::PassphraseExists, "it would lose every wallet");
    }

    #[test]
    fn a_tampered_or_foreign_file_is_refused() {
        let dir = dir("tamper");
        create_at(&dir, "correct horse", CHEAP).unwrap();
        let file = dir.join(FILE);
        let mut sealed: serde_json::Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        let key = sealed["key"].as_str().unwrap().to_owned();
        let flipped = key.strip_prefix('0').map_or_else(|| format!("0{}", &key[1..]), |rest| format!("1{rest}"));
        sealed["key"] = flipped.into();
        std::fs::write(&file, serde_json::to_vec(&sealed).unwrap()).unwrap();
        assert_eq!(unlock(&dir, "correct horse").unwrap_err(), VaultError::WrongPassphrase);
        std::fs::write(&file, b"not json").unwrap();
        assert_eq!(unlock(&dir, "correct horse").unwrap_err(), VaultError::NotAWalletFile);
    }

    #[cfg(unix)]
    #[test]
    fn only_this_user_can_read_the_file() {
        use std::os::unix::fs::PermissionsExt;
        let dir = dir("mode");
        create_at(&dir, "correct horse", CHEAP).unwrap();
        assert_eq!(std::fs::metadata(dir.join(FILE)).unwrap().permissions().mode() & 0o777, 0o600);
    }
}
