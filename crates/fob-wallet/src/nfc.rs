//! What Fob's two NFC protocols share: a code framed with its length and
//! checksum, and the command that selects an application.

use sha3::{Digest, Sha3_256};

use crate::error::{CodeError, CodeErrorResult as Result};

/// The first four bytes of a code's SHA3-256.
pub fn checksum(code: &[u8]) -> [u8; 4] {
    let hash = Sha3_256::digest(code);
    [hash[0], hash[1], hash[2], hash[3]]
}

/// `code` with its protocol version, length and checksum in front; a code over
/// `code_max` bytes is refused with `too_big`.
pub fn frame(code: &str, version: u8, code_max: usize, too_big: &str) -> Result<Vec<u8>> {
    let bytes = code.as_bytes();
    if bytes.len() > code_max {
        return Err(CodeError::Profile(too_big.into()));
    }
    let mut out = Vec::with_capacity(1 + 2 + 4 + bytes.len());
    out.push(version);
    out.extend_from_slice(&(bytes.len() as u16).to_be_bytes());
    out.extend_from_slice(&checksum(bytes));
    out.extend_from_slice(bytes);
    Ok(out)
}

/// The SELECT command for application `aid`.
pub fn select(aid: &[u8; 8]) -> Vec<u8> {
    let mut apdu = vec![0x00, 0xA4, 0x04, 0x00, aid.len() as u8];
    apdu.extend_from_slice(aid);
    apdu.push(0x00);
    apdu
}
