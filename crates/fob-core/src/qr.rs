//! QR codes as a grid of modules, so an app only has to draw squares.

use qrcode::{EcLevel, QrCode, Version};

use crate::error::{FobError, Result};

pub struct QrMatrix {
    /// Modules per side, without a quiet zone.
    pub size: u32,
    /// Row by row, `true` for a dark module.
    pub dark: Vec<bool>,
}

/// Past this version at medium error correction, a code is made at low
/// instead: a screen isn't scratched or dirty, and fewer, larger modules
/// read better than more correction. Every payment code stays at medium
/// (they're version 18 or below); a profile's photo makes it dense.
const DENSE: i16 = 20;

/// The QR code for `text`, at medium error correction, or low for a code
/// past version [`DENSE`] (a profile with its photo), at most version 40.
/// The text goes in as it is: Monero addresses are case-sensitive, so it
/// can't be upper-cased into the denser alphanumeric mode.
pub fn encode(text: &str) -> Result<QrMatrix> {
    let at = |level| QrCode::with_error_correction_level(text.as_bytes(), level).ok();
    let code = match at(EcLevel::M) {
        Some(code) if matches!(code.version(), Version::Normal(v) if v <= DENSE) => code,
        medium => at(EcLevel::L).or(medium).ok_or(FobError::BadCode)?,
    };
    let size = code.width() as u32;
    let dark = code.to_colors().into_iter().map(|c| c == qrcode::Color::Dark).collect();
    Ok(QrMatrix { size, dark })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_payment_uri_fits_and_is_square() {
        let uri = format!("monero:{}?tx_amount=0.1421", "5".repeat(95));
        let qr = encode(&uri).unwrap();
        assert!(qr.size >= 21);
        assert_eq!(qr.dark.len(), (qr.size * qr.size) as usize);
        assert!(qr.dark[0], "finder patterns start dark");
    }

    #[test]
    fn a_long_code_is_made_at_low_correction_up_to_version_40() {
        let version = |text: &str| (encode(text).unwrap().size - 17) / 4;
        // A payment code keeps medium: version 6 holds 106 bytes at M (L: 5).
        assert_eq!(version(&"a".repeat(106)), 6);
        // Past version 20 at medium (666 bytes), low: 2000 bytes are version 33.
        assert_eq!(version(&"a".repeat(2000)), 33);
        // Version 40 at low holds 2953 bytes; more can't be a code.
        assert_eq!(version(&"a".repeat(2953)), 40);
        assert!(encode(&"a".repeat(2954)).is_err());
    }
}
