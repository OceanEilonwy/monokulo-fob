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
/// read better than more correction. A payment or wallet code stays at
/// medium; a profile's photo makes one dense.
const DENSE: i16 = 20;

/// The QR code for `text` (a payment code, a wallet's, a profile's), at
/// medium error correction, or low for a code past version [`DENSE`] (a
/// profile with its photo), at most version 40. Nothing is drawn over a
/// code: the address's pattern sits beside it, never in it.
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
        // A short code keeps medium: version 6 holds 106 bytes at M (L: 5).
        assert_eq!(version(&"a".repeat(106)), 6);
        // Past version 20 at medium (666 bytes), low: 2000 bytes are version 33.
        assert_eq!(version(&"a".repeat(2000)), 33);
        // Version 40 at low holds 2953 bytes; more can't be a code.
        assert_eq!(version(&"a".repeat(2953)), 40);
        assert!(encode(&"a".repeat(2954)).is_err());
    }

    /// The codes Receive and a request make, from a bare address to the
    /// longest each can be.
    fn payment_uris() -> Vec<String> {
        use crate::uri::{payment_uri, receive};
        let address = "888tNkZrPN6JsEgekjMnABU4TBzc2Dt29EPAvkRxbANsAnjyPbb3iQ1YBRk1UXcdRsiKc9dhwMVgN5S9cQUiyoogDavup3H";
        let integrated = format!("4{}Ab9zQx7Wk2m", &address[1..]);
        let most = "18446744.073709551615";
        vec![
            format!("monero:{address}"),
            receive(address, Some("0.1421"), None).unwrap().uri,
            receive(address, Some("0.1421"), Some("Concert tickets")).unwrap().uri,
            payment_uri(address, Some(481_700_000_000), Some("Concert tickets"), Some("Henry Hollingworth")),
            receive(&integrated, Some(most), Some(&"a".repeat(crate::uri::NOTE_MAX_CHARS))).unwrap().uri,
            // Receive's longest: an integrated address, the largest amount, the note all emoji.
            receive(&integrated, Some(most), Some(&"😀".repeat(crate::uri::NOTE_MAX_CHARS))).unwrap().uri,
            // A request's longest: a name and a note all emoji.
            payment_uri(address, Some(u64::MAX), Some(&"😀".repeat(crate::uri::NOTE_MAX_CHARS)), Some(&"😀".repeat(40))),
        ]
    }

    /// Read `dark` back as a camera would a screen: 4 pixels a module, a
    /// 4-module quiet zone.
    fn read(size: u32, dark: impl Fn(u32, u32) -> bool) -> (rqrr::MetaData, String) {
        let (px, quiet) = (4, 4);
        let side = ((size + 2 * quiet) * px) as usize;
        let at = |p: usize| (p as f32 + 0.5) / px as f32 - quiet as f32;
        let mut image = rqrr::PreparedImage::prepare_from_greyscale(side, side, |x, y| {
            let (x, y) = (at(x), at(y));
            let inside = x >= 0.0 && y >= 0.0 && x < size as f32 && y < size as f32;
            if inside && dark(x as u32, y as u32) { 0 } else { 255 }
        });
        let grids = image.detect_grids();
        assert_eq!(grids.len(), 1, "one code found");
        grids[0].decode().expect("the code reads")
    }

    #[test]
    fn every_payment_code_reads_whole_at_medium_and_only_a_request_s_longest_at_low() {
        let made: Vec<_> = payment_uris().iter().map(|uri| encode(uri).unwrap()).collect();
        // A bare address is version 6; Receive's longest 18; a request's longest (886 bytes) is past DENSE.
        assert_eq!(made.iter().map(|qr| (qr.size - 17) / 4).collect::<Vec<_>>(), [6, 7, 9, 10, 12, 18, 21]);
        for (qr, uri) in made.iter().zip(payment_uris()) {
            let (meta, text) = read(qr.size, |x, y| qr.dark[(y * qr.size + x) as usize]);
            assert_eq!(text, uri);
            // rqrr's levels: 0 medium, 1 low.
            assert_eq!(meta.ecc_level, if (qr.size - 17) / 4 <= DENSE as u32 { 0 } else { 1 });
        }
    }
}
