//! QR codes as a grid of modules, so an app only has to draw squares.

use qrcode::{EcLevel, QrCode, Version};

use crate::error::{FobError, Result};

pub struct QrMatrix {
    /// Modules per side, without a quiet zone.
    pub size: u32,
    /// Row by row, `true` for a dark module.
    pub dark: Vec<bool>,
    /// Modules per side of the light square cleared in the middle for the
    /// address's pattern, 0 for none. Odd, like `size`, so it sits exactly
    /// in the middle; its outer ring of modules is a light margin and the
    /// pattern goes in the `centre - 2` inside it.
    pub centre: u32,
}

/// Past this version at medium error correction, a code is made at low
/// instead: a screen isn't scratched or dirty, and fewer, larger modules
/// read better than more correction. A wallet's code stays at medium; a
/// profile's photo makes one dense. Payment codes are made by
/// [`encode_payment`], whose correction pays for the pattern in the middle.
const DENSE: i16 = 20;

/// The QR code for `text` (a wallet's, a profile's), at medium error
/// correction, or low for a code past version [`DENSE`] (a profile with its
/// photo), at most version 40.
/// The text goes in as it is: Monero addresses are case-sensitive, so it
/// can't be upper-cased into the denser alphanumeric mode.
pub fn encode(text: &str) -> Result<QrMatrix> {
    let at = |level| QrCode::with_error_correction_level(text.as_bytes(), level).ok();
    let code = match at(EcLevel::M) {
        Some(code) if matches!(code.version(), Version::Normal(v) if v <= DENSE) => code,
        medium => at(EcLevel::L).or(medium).ok_or(FobError::BadCode)?,
    };
    Ok(matrix(&code, 0))
}

/// A payment code's error correction, most first, each while the code stays
/// at or below its version: high (about 30 % of codewords recoverable) for
/// every code Receive makes, its longest note in emoji and all (521 bytes,
/// version 25); quartile (about 25 %) for a request's longest, a name and a
/// note in emoji (886 bytes, version 29). A usual one, an amount and a
/// short note, is version 12 at high. Both leave room for the pattern in
/// the middle.
const PAYMENT_LEVELS: [(EcLevel, i16); 2] = [(EcLevel::H, 25), (EcLevel::Q, 30)];

/// The cleared square's side, at most this share of the code's: a quarter
/// of the side is under 7 % of its modules, so about a tenth of the
/// codewords at most (a codeword's 8 modules straddle the square's edge),
/// well inside what high or quartile recover, and the pattern is still
/// about 40 dp across on Receive's card.
const CENTRE_SHARE: f32 = 0.25;

/// A payment code (Receive's, a request's) with its middle cleared for the
/// address's pattern: at high error correction, or quartile for a long
/// one, as [`PAYMENT_LEVELS`] says. A code too long for either (none Fob
/// makes) is made as [`encode`] makes it, with no square.
pub fn encode_payment(text: &str) -> Result<QrMatrix> {
    let Some(code) = payment_code(text) else { return encode(text) };
    let centre = centre_for(code.width() as u32);
    Ok(matrix(&code, centre))
}

/// `text` at the first of [`PAYMENT_LEVELS`] that holds it.
fn payment_code(text: &str) -> Option<QrCode> {
    PAYMENT_LEVELS.iter().find_map(|&(level, most)| {
        QrCode::with_error_correction_level(text.as_bytes(), level)
            .ok()
            .filter(|code| matches!(code.version(), Version::Normal(v) if v <= most))
    })
}

/// The cleared square for a code `size` modules wide: the largest odd side
/// within [`CENTRE_SHARE`] of it.
fn centre_for(size: u32) -> u32 {
    let side = (size as f32 * CENTRE_SHARE) as u32;
    if side % 2 == 0 { side - 1 } else { side }
}

/// The code's modules, with a light square `centre` wide in the middle.
fn matrix(code: &QrCode, centre: u32) -> QrMatrix {
    let size = code.width() as u32;
    let start = (size - centre) / 2;
    let cleared = start..start + centre;
    let dark = code
        .to_colors()
        .into_iter()
        .enumerate()
        .map(|(i, c)| {
            let (x, y) = (i as u32 % size, i as u32 / size);
            c == qrcode::Color::Dark && !(cleared.contains(&x) && cleared.contains(&y))
        })
        .collect();
    QrMatrix { size, dark, centre }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_payment_uri_fits_and_is_square() {
        let uri = format!("monero:{}?tx_amount=0.1421", "5".repeat(95));
        let qr = encode_payment(&uri).unwrap();
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

    fn version(qr: &QrMatrix) -> u32 {
        (qr.size - 17) / 4
    }

    /// Read `dark` back as a camera would a screen: 4 pixels a module, a
    /// 4-module quiet zone.
    fn read(size: u32, dark: impl Fn(u32, u32) -> bool) -> (rqrr::MetaData, String) {
        let (px, quiet) = (4, 4);
        let side = ((size + 2 * quiet) * px) as usize;
        let mut image = rqrr::PreparedImage::prepare_from_greyscale(side, side, |x, y| {
            let (x, y) = ((x as u32 / px).wrapping_sub(quiet), (y as u32 / px).wrapping_sub(quiet));
            if x < size && y < size && dark(x, y) { 0 } else { 255 }
        });
        let grids = image.detect_grids();
        assert_eq!(grids.len(), 1, "one code found");
        grids[0].decode().expect("the code reads")
    }

    #[test]
    fn payment_codes_are_high_or_quartile_with_a_quarter_wide_middle_cleared() {
        let made: Vec<_> = payment_uris().iter().map(|uri| encode_payment(uri).unwrap()).collect();
        assert_eq!(made.iter().map(version).collect::<Vec<_>>(), [10, 10, 12, 14, 17, 25, 29]);
        for qr in &made {
            let (n, c) = (qr.size, qr.centre);
            assert!(c % 2 == 1 && c >= 13 && c * 4 <= n, "{c} of {n}");
            assert!((c * c) as f32 / (n * n) as f32 <= 0.07, "under 7 % of the modules");
            let start = (n - c) / 2;
            assert_eq!(start * 2 + c, n, "exactly in the middle");
            for y in start..start + c {
                for x in start..start + c {
                    assert!(!qr.dark[(y * n + x) as usize], "cleared at {x}, {y}");
                }
            }
            // Just outside, the code's own modules: some dark.
            assert!((start..start + c).any(|x| qr.dark[((start - 1) * n + x) as usize]));
        }
        // A code past both levels is made as any other, with nothing cleared.
        let long = encode_payment(&"a".repeat(1500)).unwrap();
        assert_eq!(long.centre, 0);
        assert_eq!(long.dark, encode(&"a".repeat(1500)).unwrap().dark);
        // Other codes (a wallet's, a profile's) clear nothing.
        assert_eq!(encode(&payment_uris()[0]).unwrap().centre, 0);
    }

    #[test]
    fn a_payment_code_reads_with_its_middle_cleared_or_every_module_there_wrong() {
        for uri in payment_uris() {
            let qr = encode_payment(&uri).unwrap();
            let n = qr.size;
            // As drawn: the middle light, the pattern's dots read as light.
            let (meta, text) = read(n, |x, y| qr.dark[(y * n + x) as usize]);
            assert_eq!(text, uri);
            assert_eq!(meta.ecc_level, if version(&qr) <= 25 { 2 } else { 3 }, "high, or quartile");
            // The worst a pattern could do: every module in the square the
            // opposite of the code's own.
            let full = payment_code(&uri).unwrap().to_colors();
            let start = (n - qr.centre) / 2;
            let inside = |v: u32| (start..start + qr.centre).contains(&v);
            let (_, text) = read(n, |x, y| (full[(y * n + x) as usize] == qrcode::Color::Dark) != (inside(x) && inside(y)));
            assert_eq!(text, uri, "version {}", version(&qr));
        }
    }
}
