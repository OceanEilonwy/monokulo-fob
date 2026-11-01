//! QR codes as a grid of modules, so an app only has to draw squares (and,
//! in a payment code, the address's pattern as circles).

use qrcode::{EcLevel, QrCode, Version};

use crate::error::{FobError, Result};
use crate::pattern::{self, Pattern};

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
    /// The pattern of the address the code pays, to draw in the square: a
    /// payment code's only.
    pub dots: Option<Dots>,
}

/// An address's pattern as drawn in a payment code, in modules: an app
/// paints each dot as a circle and nothing else, so what a test reads is
/// what a phone shows.
#[derive(Clone, Debug, PartialEq)]
pub struct Dots {
    /// 0–7: an index into the apps' fixed palette, as [`Pattern::colour`].
    pub colour: u8,
    /// Each dot's radius.
    pub radius: f32,
    /// Each dot's centre, (x, y) from the code's top-left corner (its first
    /// module's, the quiet zone not counted).
    pub centres: Vec<(f32, f32)>,
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
/// at or below its version: high (about 30 % of codewords recoverable) up
/// to version 20; quartile (about 25 %) past it, Receive's longest and a
/// request's, keeping their modules larger. Both leave room for the
/// pattern in the middle.
const PAYMENT_LEVELS: [(EcLevel, i16); 2] = [(EcLevel::H, 20), (EcLevel::Q, 30)];

/// The smallest version a payment code is made at (docs/decisions.md has
/// why and the numbers).
const FEWEST: i16 = 11;

/// The cleared square's side, at most this share of the code's: a quarter
/// of the side is under 7 % of its modules, so about a tenth of the
/// codewords at most (a codeword's 8 modules straddle the square's edge),
/// well inside what high or quartile recover, and the pattern is still
/// about 40 dp across on Receive's card.
const CENTRE_SHARE: f32 = 0.25;

/// A payment code (Receive's, a request's) with its middle cleared for the
/// pattern of the address it pays, the dots placed: at high error
/// correction, or quartile for a long one, as [`PAYMENT_LEVELS`] says, and
/// at least version [`FEWEST`]. A code too long for either (none Fob
/// makes) is made as [`encode`] makes it, with no square and no pattern.
pub fn encode_payment(text: &str) -> Result<QrMatrix> {
    let Some((code, version)) = payment_code(text) else { return encode(text) };
    let centre = centre_for(version);
    let mut qr = matrix(&code, centre);
    qr.dots = Some(dots(&pattern::address_pattern(crate::uri::code_address(text)), qr.size, centre));
    Ok(qr)
}

/// `text` at the first of [`PAYMENT_LEVELS`] that holds it, at least at
/// version [`FEWEST`], and its version.
fn payment_code(text: &str) -> Option<(QrCode, i16)> {
    PAYMENT_LEVELS.iter().find_map(|&(level, most)| {
        let code = QrCode::with_error_correction_level(text.as_bytes(), level).ok()?;
        match code.version() {
            Version::Normal(v) if v < FEWEST => Some((QrCode::with_version(text.as_bytes(), Version::Normal(FEWEST), level).ok()?, FEWEST)),
            Version::Normal(v) if v <= most => Some((code, v)),
            _ => None,
        }
    })
}

/// The centres of a version's alignment patterns along either axis (each
/// pattern 5 modules across, its centre's and 2 either side): none in
/// version 1, then 6, the last at `size − 7` and the rest evenly spaced
/// between (ISO/IEC 18004 annex E's table, as a formula).
fn alignment_centres(version: i16) -> Vec<u32> {
    if version < 2 {
        return vec![];
    }
    let (v, count) = (version as u32, version as u32 / 7 + 2);
    let step = if v == 32 { 26 } else { (v * 4 + count * 2 + 1) / (count * 2 - 2) * 2 };
    let last = 4 * v + 17 - 7;
    let mut centres: Vec<u32> = (0..count - 1).map(|i| last - i * step).collect();
    centres.push(6);
    centres.reverse();
    centres
}

/// The cleared square for a version's code: the largest odd side within
/// [`CENTRE_SHARE`] of the code's that holds at most one alignment pattern,
/// whole, and misses the rest: never cutting one, and never taking more
/// than one from a reader that samples between them, as ZBar and zxing do
/// on a tilted code. (Along one axis, at most one; so at most one in all.)
fn centre_for(version: i16) -> u32 {
    let size = 4 * version as u32 + 17;
    let centres = alignment_centres(version);
    let most = (size as f32 * CENTRE_SHARE) as u32;
    (1..=most)
        .rev()
        .filter(|side| side % 2 == 1)
        .find(|side| {
            let (from, to) = ((size - side) / 2, (size + side) / 2 - 1);
            // Along one axis; the square is the same along the other.
            let missed = centres.iter().filter(|&&c| c + 2 < from || c - 2 > to).count();
            let held = centres.iter().filter(|&&c| c - 2 >= from && c + 2 <= to).count();
            missed + held == centres.len() && held <= 1
        })
        .unwrap_or(0)
}

/// How big the dots are: on a 5 × 5 grid filling the square inside its
/// one-module margin, each dot this share of its cell across.
const DOT: f32 = 0.84;

/// `pattern`'s dots in the `centre`-wide square in the middle of a code
/// `size` modules wide.
fn dots(pattern: &Pattern, size: u32, centre: u32) -> Dots {
    let grid = centre.saturating_sub(2) as f32;
    let cell = grid / 5.0;
    let start = (size as f32 - grid) / 2.0;
    let centres = pattern
        .dots()
        .iter()
        .enumerate()
        .filter(|(_, on)| **on)
        .map(|(i, _)| (start + ((i % 5) as f32 + 0.5) * cell, start + ((i / 5) as f32 + 0.5) * cell))
        .collect();
    Dots { colour: pattern.colour, radius: cell * DOT / 2.0, centres }
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
    QrMatrix { size, dark, centre, dots: None }
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
        read_grey(size, 4, |x, y| {
            let inside = x >= 0.0 && y >= 0.0 && x < size as f32 && y < size as f32;
            if inside && dark(x as u32, y as u32) { 0 } else { 255 }
        })
    }

    /// Read a code `size` modules wide, `px` pixels a module with a
    /// 4-module quiet zone, from `grey` at a point in modules from the
    /// code's corner (the quiet zone negative or past `size`), sampled at
    /// each pixel's centre.
    fn read_grey(size: u32, px: u32, grey: impl Fn(f32, f32) -> u8) -> (rqrr::MetaData, String) {
        let quiet = 4;
        let side = ((size + 2 * quiet) * px) as usize;
        let at = |p: usize| (p as f32 + 0.5) / px as f32 - quiet as f32;
        let mut image = rqrr::PreparedImage::prepare_from_greyscale(side, side, |x, y| grey(at(x), at(y)));
        let grids = image.detect_grids();
        assert_eq!(grids.len(), 1, "one code found");
        grids[0].decode().expect("the code reads")
    }

    /// The grey of each of the apps' pattern colours (Theme.kt's
    /// `patternColours`), as luminance (Rec. 601) sees it.
    const PATTERN_GREYS: [u8; 8] = [95, 89, 86, 92, 87, 88, 86, 68];

    /// A code as a phone draws it: its modules, and its dots as circles in
    /// their colour's grey.
    fn as_drawn(qr: &QrMatrix) -> impl Fn(f32, f32) -> u8 + '_ {
        let (from, to) = ((qr.size - qr.centre) as f32 / 2.0, (qr.size + qr.centre) as f32 / 2.0);
        move |x, y| {
            let square = (from..to).contains(&x) && (from..to).contains(&y);
            if square
                && let Some(d) = &qr.dots
                && d.centres.iter().any(|(cx, cy)| (x - cx).powi(2) + (y - cy).powi(2) <= d.radius.powi(2))
            {
                return PATTERN_GREYS[d.colour as usize];
            }
            let inside = x >= 0.0 && y >= 0.0 && x < qr.size as f32 && y < qr.size as f32;
            if inside && qr.dark[(y as u32 * qr.size + x as u32) as usize] { 26 } else { 255 }
        }
    }

    #[test]
    fn payment_codes_are_high_then_quartile_at_least_version_11_with_a_quarter_wide_middle_cleared() {
        let made: Vec<_> = payment_uris().iter().map(|uri| encode_payment(uri).unwrap()).collect();
        // A bare address alone would be version 10.
        assert_eq!(made.iter().map(version).collect::<Vec<_>>(), [11, 11, 12, 14, 17, 22, 29]);
        for qr in &made {
            let (n, c) = (qr.size, qr.centre);
            assert!(c % 2 == 1 && c >= 15 && c * 4 <= n, "{c} of {n}");
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
        assert_eq!((long.centre, long.dots), (0, None));
        assert_eq!(long.dark, encode(&"a".repeat(1500)).unwrap().dark);
        // Other codes (a wallet's, a profile's) clear nothing and have no dots.
        let wallet = encode(&payment_uris()[0]).unwrap();
        assert_eq!((wallet.centre, wallet.dots), (0, None));
    }

    #[test]
    fn alignment_patterns_are_where_the_standard_puts_them() {
        assert_eq!(alignment_centres(1), Vec::<u32>::new());
        assert_eq!(alignment_centres(2), [6, 18]);
        assert_eq!(alignment_centres(10), [6, 28, 50]);
        assert_eq!(alignment_centres(14), [6, 26, 46, 66]);
        assert_eq!(alignment_centres(25), [6, 32, 58, 84, 110]);
        assert_eq!(alignment_centres(29), [6, 30, 54, 78, 102, 126]);
        assert_eq!(alignment_centres(32), [6, 34, 60, 86, 112, 138]);
        assert_eq!(alignment_centres(36), [6, 24, 50, 76, 102, 128, 154]);
        assert_eq!(alignment_centres(40), [6, 30, 58, 86, 114, 142, 170]);
    }

    #[test]
    fn the_square_holds_only_the_middle_alignment_pattern_whole_and_misses_the_rest() {
        // v14 and v17's quarter (17 and 21) would cut a corner off four;
        // v29's (33) would hold four whole.
        assert_eq!((centre_for(14), centre_for(17), centre_for(29)), (15, 19, 19));
        for version in FEWEST..=30 {
            let (size, c) = (4 * version as u32 + 17, centre_for(version));
            let (from, to) = ((size - c) / 2, (size + c) / 2 - 1);
            assert!(c % 2 == 1 && c >= 15 && c as f32 <= size as f32 * CENTRE_SHARE, "v{version}: {c}");
            let inside: Vec<_> = alignment_centres(version).iter().map(|a| (a - 2..=a + 2).filter(|m| (from..=to).contains(m)).count()).collect();
            assert!(inside.iter().all(|n| [0, 5].contains(n)), "v{version}: a pattern cut, {inside:?}");
            assert!(inside.iter().filter(|n| **n == 5).count() <= 1, "v{version}: more than one held, {inside:?}");
        }
    }

    #[test]
    fn the_dots_are_the_pattern_of_the_address_the_code_pays() {
        let uri = &payment_uris()[2];
        let address = crate::uri::code_address(uri);
        let qr = encode_payment(uri).unwrap();
        let dots = qr.dots.clone().unwrap();
        let p = pattern::address_pattern(address);
        assert_eq!(dots.colour, p.colour);
        assert_eq!(dots.centres.len(), p.dots().iter().filter(|d| **d).count());
        // On a 5 × 5 grid inside the square's one-module margin, never past it.
        let (start, end) = ((qr.size - qr.centre) / 2 + 1, (qr.size + qr.centre) / 2 - 1);
        let cell = (qr.centre - 2) as f32 / 5.0;
        assert!((dots.radius - cell * 0.42).abs() < 1e-4);
        for (x, y) in &dots.centres {
            for v in [x, y] {
                assert!(v - dots.radius >= start as f32 && v + dots.radius <= end as f32, "{v} in {start}..{end}");
            }
        }
        // A bare address's code has the same dots where its square is the same size.
        assert_eq!(encode_payment(&format!("monero:{address}")).unwrap().dots.unwrap().colour, p.colour);
    }

    #[test]
    fn a_payment_code_reads_with_its_middle_cleared_or_every_module_there_wrong() {
        for uri in payment_uris() {
            let qr = encode_payment(&uri).unwrap();
            let n = qr.size;
            // The modules alone: the middle light.
            let (meta, text) = read(n, |x, y| qr.dark[(y * n + x) as usize]);
            assert_eq!(text, uri);
            assert_eq!(meta.ecc_level, if version(&qr) <= 20 { 2 } else { 3 }, "high, or quartile");
            // The worst a pattern could do: every module in the square the
            // opposite of the code's own.
            let full = payment_code(&uri).unwrap().0.to_colors();
            let start = (n - qr.centre) / 2;
            let inside = |v: u32| (start..start + qr.centre).contains(&v);
            let (_, text) = read(n, |x, y| (full[(y * n + x) as usize] == qrcode::Color::Dark) != (inside(x) && inside(y)));
            assert_eq!(text, uri, "version {}", version(&qr));
        }
    }

    /// Codes as a phone draws them, the real dots in their colours' greys,
    /// for addresses at every version a payment code can be (11 to 30), read
    /// at 4 pixels a module and some at 6. (ZBar and zxing-cpp were run over far more,
    /// once: docs/decisions.md.)
    #[test]
    fn payment_codes_read_with_their_dots_for_many_addresses_at_every_version() {
        const B58: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
        let mut seed = 7u64;
        let mut next = move || {
            seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
            (seed >> 33) as usize
        };
        let uri = |address: &str, note: usize| match note {
            0 => format!("monero:{address}"),
            n => format!("monero:{address}?tx_amount=0.1421&tx_description={}", "n".repeat(n)),
        };
        let probe = format!("5{}", "a".repeat(94));
        // The shortest note that makes a code of each version, one pass.
        let mut notes = std::collections::BTreeMap::new();
        let mut note = 0;
        while notes.len() < 20 {
            notes.entry(version(&encode_payment(&uri(&probe, note)).unwrap())).or_insert(note);
            note += 4;
        }
        assert_eq!(notes.keys().copied().collect::<Vec<_>>(), (FEWEST as u32..=30).collect::<Vec<_>>());
        for (&want, &note) in &notes {
            for i in 0..3 {
                let address: String = std::iter::once('5').chain((0..94).map(|_| B58[next() % 58] as char)).collect();
                let text = uri(&address, note);
                let qr = encode_payment(&text).unwrap();
                assert_eq!(version(&qr), want);
                for px in if i == 0 { &[4, 6][..] } else { &[4][..] } {
                    let (_, read) = read_grey(qr.size, *px, as_drawn(&qr));
                    assert_eq!(read, text, "version {want} at {px} px");
                }
            }
        }
    }
}
