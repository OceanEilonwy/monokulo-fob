//! The arithmetic the host side of Trezor's Monero protocol needs: Keccak
//! hashes, the keys that open what the device seals, and checks that what
//! it hands back is what it claims.

use chacha20poly1305::{
    ChaCha20Poly1305, Key, Nonce,
    aead::{Aead, KeyInit},
};
use curve25519_dalek::{
    EdwardsPoint, Scalar,
    constants::ED25519_BASEPOINT_POINT,
    edwards::CompressedEdwardsY,
    traits::{IsIdentity, VartimeMultiscalarMul},
};
use monero_wallet::ed25519::Point;
use sha3::{Digest, Keccak256};

use super::proto::varint;

pub fn keccak(data: &[u8]) -> [u8; 32] {
    Keccak256::digest(data).into()
}

/// Keccak twice over: `H(H(data))`, as Trezor's keys are derived.
pub fn keccak_twice(data: &[u8]) -> [u8; 32] {
    keccak(&keccak(data))
}

/// A hash reduced to a scalar, as Monero's `hash_to_scalar`.
pub fn hash_to_scalar(data: &[u8]) -> Scalar {
    Scalar::from_bytes_mod_order(keccak(data))
}

/// The key (or, with `iv`, the nonce) that opens the sealed signature of
/// input `index`: `H(H(master || "sig-key" padded to 12 || varint(index)
/// padded to 4))`, as the device derives it.
pub fn sealing_key(master: &[u8], index: usize, iv: bool) -> [u8; 32] {
    let separator: &[u8] = if iv { b"sig-iv" } else { b"sig-key" };
    let index = varint(index as u64);
    let mut data = Vec::with_capacity(48);
    data.extend_from_slice(master);
    data.extend_from_slice(separator);
    data.resize(32 + 12, 0);
    data.extend_from_slice(&index);
    data.resize(32 + 12 + index.len().max(4), 0);
    keccak_twice(&data)
}

/// Opens `sealed` (ciphertext then its 16-byte tag) under `key` and `nonce`.
pub fn open(key: &[u8], nonce: &[u8], sealed: &[u8]) -> Result<Vec<u8>, String> {
    if key.len() != 32 || nonce.len() != 12 {
        return Err("the device's encryption key has the wrong size".into());
    }
    ChaCha20Poly1305::new(Key::from_slice(key))
        .decrypt(Nonce::from_slice(nonce), sealed)
        .map_err(|_| "what the device sealed doesn't open: the data was changed on the way".to_string())
}

/// A point with any torsion component removed, as Monero's
/// `pubkey_clear_torsion`: the device refuses points off the prime-order
/// subgroup.
pub fn clear_torsion(point: EdwardsPoint) -> EdwardsPoint {
    point.mul_by_cofactor() * Scalar::from(8u8).invert()
}

/// Keccak of an output's details, as both sides hash them to agree on what
/// the key image sync covers.
pub fn transfer_hash(out_key: &[u8; 32], tx_pub_key: &[u8; 32], additional: Option<&[u8; 32]>, index_in_tx: u64) -> [u8; 32] {
    let mut hasher = Keccak256::new();
    hasher.update(out_key);
    hasher.update(tx_pub_key);
    if let Some(additional) = additional {
        hasher.update(additional);
    }
    hasher.update(varint(index_in_tx));
    hasher.finalize().into()
}

/// Checks the one-member ring signature `(c, r)` the device gives with a
/// key image, proving the image belongs to `out_key`: as Monero's
/// `check_ring_signature` with the image's own bytes as the message.
pub fn key_image_checks_out(image: &[u8; 32], c: &[u8; 32], r: &[u8; 32], out_key: &[u8; 32]) -> bool {
    let Some(image_point) = CompressedEdwardsY(*image).decompress() else { return false };
    if image_point.is_identity() || !image_point.is_torsion_free() {
        return false;
    }
    let Some(public) = CompressedEdwardsY(*out_key).decompress() else { return false };
    let (Some(c), Some(r)) = (Scalar::from_canonical_bytes(*c).into(), Scalar::from_canonical_bytes(*r).into()) else {
        return false;
    };
    let c: Scalar = c;
    let r: Scalar = r;
    let hashed = Point::biased_hash(*out_key).into();
    // L = c·P + r·G, R = r·Hp(P) + c·I
    let l = EdwardsPoint::vartime_multiscalar_mul([c, r], [public, ED25519_BASEPOINT_POINT]);
    let right = EdwardsPoint::vartime_multiscalar_mul([r, c], [hashed, image_point]);
    let mut data = Vec::with_capacity(96);
    data.extend_from_slice(image);
    data.extend_from_slice(l.compress().as_bytes());
    data.extend_from_slice(right.compress().as_bytes());
    hash_to_scalar(&data) == c
}

/// Makes a one-member ring signature as the device does, for tests of the
/// check above and of the key image sync.
#[cfg(test)]
pub fn sign_key_image(image: &[u8; 32], secret: &Scalar, out_key: &[u8; 32]) -> ([u8; 32], [u8; 32]) {
    let k = Scalar::from_bytes_mod_order(keccak(secret.as_bytes()));
    let l = ED25519_BASEPOINT_POINT * k;
    let hashed: EdwardsPoint = Point::biased_hash(*out_key).into();
    let right = hashed * k;
    let mut data = Vec::with_capacity(96);
    data.extend_from_slice(image);
    data.extend_from_slice(l.compress().as_bytes());
    data.extend_from_slice(right.compress().as_bytes());
    let c = hash_to_scalar(&data);
    let r = k - c * secret;
    (c.to_bytes(), r.to_bytes())
}

/// Seals as the device does, for tests.
#[cfg(test)]
pub fn seal(key: &[u8; 32], nonce: &[u8; 12], plain: &[u8]) -> Vec<u8> {
    ChaCha20Poly1305::new(Key::from_slice(key)).encrypt(Nonce::from_slice(nonce), plain).expect("sealing can't fail")
}

#[cfg(test)]
mod tests {
    use super::*;
    use curve25519_dalek::constants::ED25519_BASEPOINT_TABLE;
    use rand_core::{OsRng, RngCore};

    #[test]
    fn sealing_keys_pad_the_separator_and_index_as_the_device_does() {
        let master = [9u8; 32];
        let mut expected = Vec::new();
        expected.extend_from_slice(&master);
        expected.extend_from_slice(b"sig-key\0\0\0\0\0");
        expected.extend_from_slice(&[3, 0, 0, 0]);
        assert_eq!(sealing_key(&master, 3, false), keccak_twice(&expected));
        let mut expected = Vec::new();
        expected.extend_from_slice(&master);
        expected.extend_from_slice(b"sig-iv\0\0\0\0\0\0");
        expected.extend_from_slice(&[200, 1, 0, 0]);
        assert_eq!(sealing_key(&master, 200, true), keccak_twice(&expected));
        assert_ne!(sealing_key(&master, 0, true), sealing_key(&master, 0, false));
    }

    #[test]
    fn opens_what_was_sealed_and_nothing_else() {
        let key = [1u8; 32];
        let nonce = [2u8; 12];
        let sealed = seal(&key, &nonce, b"secret");
        assert_eq!(open(&key, &nonce, &sealed).unwrap(), b"secret");
        let mut tampered = sealed.clone();
        tampered[0] ^= 1;
        assert!(open(&key, &nonce, &tampered).is_err());
        assert!(open(&[0; 31], &nonce, &sealed).is_err());
    }

    #[test]
    fn torsion_is_cleared_and_clean_points_stay() {
        let p = &Scalar::from(12345u64) * ED25519_BASEPOINT_TABLE;
        assert_eq!(clear_torsion(p), p);
        let torsion = curve25519_dalek::constants::EIGHT_TORSION[1];
        assert_eq!(clear_torsion(p + torsion), p);
    }

    #[test]
    fn a_key_image_signature_checks_out_only_for_its_output() {
        let mut bytes = [0u8; 32];
        OsRng.fill_bytes(&mut bytes);
        let secret = Scalar::from_bytes_mod_order(bytes);
        let out_key = (&secret * ED25519_BASEPOINT_TABLE).compress().to_bytes();
        let hashed: EdwardsPoint = Point::biased_hash(out_key).into();
        let image = (hashed * secret).compress().to_bytes();
        let (c, r) = sign_key_image(&image, &secret, &out_key);
        assert!(key_image_checks_out(&image, &c, &r, &out_key));
        let other = (&Scalar::from(5u64) * ED25519_BASEPOINT_TABLE).compress().to_bytes();
        assert!(!key_image_checks_out(&image, &c, &r, &other));
        let mut wrong_image = image;
        wrong_image[0] ^= 1;
        assert!(!key_image_checks_out(&wrong_image, &c, &r, &out_key));
        assert!(!key_image_checks_out(&[0; 32], &c, &r, &out_key), "an image that isn't a point");
    }

    #[test]
    fn transfer_hash_takes_the_index_as_a_varint() {
        let a = transfer_hash(&[1; 32], &[2; 32], None, 300);
        let mut data = vec![1u8; 32];
        data.extend_from_slice(&[2; 32]);
        data.extend_from_slice(&[0xac, 0x02]);
        assert_eq!(a, keccak(&data));
        assert_ne!(a, transfer_hash(&[1; 32], &[2; 32], Some(&[3; 32]), 300));
    }
}
