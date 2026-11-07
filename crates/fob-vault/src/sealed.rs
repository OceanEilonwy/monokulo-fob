//! Secrets kept sealed while they're held in memory, opened only for the moment
//! they're used.
//!
//! A loaded wallet needs its view key all the time, to scan, but its spend key
//! (and the seed phrase, the same secret) only to sign, work out a key image or
//! show a backup. Those are kept sealed with ChaCha20-Poly1305 under a key made
//! at random for this process and kept on locked pages ([`Locked`]), and opened
//! into a wiped buffer each time.
//!
//! This is obfuscation, not a boundary: the process holds the key that opens
//! them. What it stops is the raw scalar sitting next to the view key for the
//! whole session, to be found by scanning a memory image, a swapped page or a
//! leaked buffer for something that looks like a key.

use std::sync::OnceLock;

use chacha20poly1305::{
    ChaCha20Poly1305, KeyInit, Nonce,
    aead::{Aead, AeadCore},
};
use rand_core::{OsRng, RngCore};
use zeroize::Zeroizing;

use crate::{
    error::{Result, VaultError},
    locked::Locked,
};

/// The key every [`Sealed`] in this process is sealed under.
fn session_key() -> &'static Locked<[u8; 32]> {
    static KEY: OnceLock<Locked<[u8; 32]>> = OnceLock::new();
    KEY.get_or_init(|| {
        let mut key = Locked::new([0u8; 32]);
        OsRng.fill_bytes(&mut key[..]);
        key
    })
}

fn cipher() -> ChaCha20Poly1305 {
    ChaCha20Poly1305::new(session_key()[..].into())
}

/// Bytes sealed under this process's session key.
#[derive(Clone)]
pub struct Sealed {
    nonce: [u8; 12],
    bytes: Vec<u8>,
}

impl Sealed {
    /// Seals `plain`; wiping it is the caller's.
    pub fn seal(plain: &[u8]) -> Sealed {
        let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng);
        let bytes = cipher().encrypt(&nonce, plain).expect("sealing in memory doesn't fail");
        Sealed { nonce: nonce.into(), bytes }
    }

    /// The bytes it holds, in a buffer wiped when it goes.
    pub fn open(&self) -> Result<Zeroizing<Vec<u8>>> {
        cipher().decrypt(Nonce::from_slice(&self.nonce), self.bytes.as_slice()).map(Zeroizing::new).map_err(|_| VaultError::BadKey)
    }
}

impl std::fmt::Debug for Sealed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Sealed(..)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_opens_to_what_was_sealed() {
        let sealed = Sealed::seal(b"a spend key's 32 bytes, say.....");
        assert_eq!(sealed.open().unwrap().as_slice(), b"a spend key's 32 bytes, say.....");
        assert_eq!(sealed.clone().open().unwrap().as_slice(), sealed.open().unwrap().as_slice());
    }

    #[test]
    fn the_plain_bytes_are_nowhere_in_it() {
        let secret = [0x5au8; 32];
        let sealed = Sealed::seal(&secret);
        assert!(!sealed.bytes.windows(4).any(|w| w == [0x5a; 4]));
        assert_eq!(sealed.bytes.len(), 32 + 16, "the bytes and the tag");
        assert_eq!(format!("{sealed:?}"), "Sealed(..)");
    }

    #[test]
    fn each_seal_is_different() {
        let (a, b) = (Sealed::seal(&[1; 32]), Sealed::seal(&[1; 32]));
        assert_ne!(a.nonce, b.nonce);
        assert_ne!(a.bytes, b.bytes);
    }

    #[test]
    fn a_changed_seal_is_refused() {
        let mut sealed = Sealed::seal(&[1; 32]);
        sealed.bytes[0] ^= 1;
        assert!(matches!(sealed.open(), Err(VaultError::BadKey)));
    }
}
