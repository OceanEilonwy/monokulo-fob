//! The wallet file: the wallet's state as JSON, sealed with ChaCha20-Poly1305
//! under a 32-byte key the app keeps (on Android, behind the Keystore).

use std::path::Path;

use chacha20poly1305::{
    ChaCha20Poly1305, Key, Nonce,
    aead::{Aead, KeyInit},
};
use rand_core::{OsRng, RngCore};
use serde::{Serialize, de::DeserializeOwned};
use zeroize::Zeroizing;

use crate::error::{FobError, Result};

const MAGIC: &[u8; 4] = b"FOB1";

fn cipher(key: &[u8; 32]) -> ChaCha20Poly1305 {
    ChaCha20Poly1305::new(Key::from_slice(key))
}

/// Writes `value` to `path`, replacing it only once the new file is complete.
pub fn save<T: Serialize>(path: &Path, key: &[u8; 32], value: &T) -> Result<()> {
    let plain = Zeroizing::new(serde_json::to_vec(value).map_err(|e| FobError::Storage(e.to_string()))?);
    let mut nonce = [0u8; 12];
    OsRng.fill_bytes(&mut nonce);
    let sealed = cipher(key).encrypt(Nonce::from_slice(&nonce), plain.as_slice()).map_err(|_| FobError::Storage("sealing".into()))?;
    let mut file = Vec::with_capacity(MAGIC.len() + nonce.len() + sealed.len());
    file.extend_from_slice(MAGIC);
    file.extend_from_slice(&nonce);
    file.extend_from_slice(&sealed);
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, &file).map_err(|e| FobError::Storage(e.to_string()))?;
    std::fs::rename(&tmp, path).map_err(|e| FobError::Storage(e.to_string()))
}

pub fn load<T: DeserializeOwned>(path: &Path, key: &[u8; 32]) -> Result<T> {
    let file = std::fs::read(path).map_err(|e| FobError::Storage(e.to_string()))?;
    if file.len() < MAGIC.len() + 12 || &file[..4] != MAGIC {
        return Err(FobError::Storage("not a Fob wallet file".into()));
    }
    let (nonce, sealed) = file[4..].split_at(12);
    let plain = Zeroizing::new(cipher(key).decrypt(Nonce::from_slice(nonce), sealed).map_err(|_| FobError::WrongKey)?);
    serde_json::from_slice(&plain).map_err(|e| FobError::Storage(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_saved_value_loads_back_only_with_its_key() {
        let dir = std::env::temp_dir().join(format!("fob-store-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("wallet.fob");
        let key = [7u8; 32];
        save(&path, &key, &vec!["hello", "fob"]).unwrap();
        assert_eq!(load::<Vec<String>>(&path, &key).unwrap(), ["hello", "fob"]);
        assert!(matches!(load::<Vec<String>>(&path, &[8u8; 32]), Err(FobError::WrongKey)));
        assert!(!std::fs::read(&path).unwrap().windows(5).any(|w| w == b"hello"), "stored in the clear");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
