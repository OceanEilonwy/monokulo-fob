//! QR codes as a grid of modules, so an app only has to draw squares.

use qrcode::{EcLevel, QrCode};

use crate::error::{FobError, Result};

pub struct QrMatrix {
    /// Modules per side, without a quiet zone.
    pub size: u32,
    /// Row by row, `true` for a dark module.
    pub dark: Vec<bool>,
}

/// The QR code for `text`, at medium error correction. The text goes in as it
/// is: Monero addresses are case-sensitive, so it can't be upper-cased into the
/// denser alphanumeric mode.
pub fn encode(text: &str) -> Result<QrMatrix> {
    let code = QrCode::with_error_correction_level(text.as_bytes(), EcLevel::M).map_err(|_| FobError::BadCode)?;
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
}
