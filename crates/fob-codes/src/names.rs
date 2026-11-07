//! What may be a name, a note or a photo, wherever one comes from: typed here
//! or read from someone else's code.

use crate::{error::CodeError, error::Result, uri};

/// The most characters a contact's name has (a name from a payment code
/// is cut to it).
pub const NAME_MAX: usize = 40;

/// The most characters a contact's note has.
pub const NOTE_MAX: usize = 80;

/// The most bytes a contact's photo takes: a 128 × 128 WebP, its quality
/// stepped down (then made smaller) by the app until it fits. Photos only
/// travel by touching phones, never in a code, so this isn't held down by
/// what a QR code can carry.
pub const PHOTO_MAX: usize = 8192;

/// The most pixels a side of a photo has: the app makes them 96 at most,
/// and every avatar is drawn smaller than this.
pub const PHOTO_SIDE_MAX: u32 = 128;

/// Characters that draw nothing: zero-width spaces and joiners, the marks
/// that turn text round, line and paragraph separators, invisible
/// operators, fillers, variation selectors and tags (Unicode's default
/// ignorables, with the Arabic letter mark).
fn invisible(c: char) -> bool {
    matches!(
        c,
        '\u{ad}'
            | '\u{34f}'
            | '\u{61c}'
            | '\u{115f}'
            | '\u{1160}'
            | '\u{17b4}'
            | '\u{17b5}'
            | '\u{180b}'..='\u{180f}'
            | '\u{200b}'..='\u{200f}'
            | '\u{2028}'..='\u{202e}'
            | '\u{2060}'..='\u{206f}'
            | '\u{3164}'
            | '\u{fe00}'..='\u{fe0f}'
            | '\u{feff}'
            | '\u{ffa0}'
            | '\u{fff0}'..='\u{fff8}'
            | '\u{1bca0}'..='\u{1bca3}'
            | '\u{1d173}'..='\u{1d17a}'
            | '\u{e0000}'..='\u{e0fff}'
    )
}

/// Pictographs and symbols an emoji sequence is built from (skin tones
/// among them).
fn emoji(c: char) -> bool {
    matches!(c, '\u{1f000}'..='\u{1faff}' | '\u{2190}'..='\u{21ff}' | '\u{2300}'..='\u{23ff}' | '\u{2600}'..='\u{27bf}' | '\u{2b00}'..='\u{2bff}' | '\u{a9}' | '\u{ae}')
}

/// `text` as a name or note keeps it: no control characters (a line break,
/// a bell) and nothing that draws nothing ([`invisible`]), so two names
/// that look the same are the same. A joiner between two emoji, and a
/// variation selector or tag after one, stay: they make one emoji of
/// several (a family, a flag).
pub fn clean(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut kept = String::with_capacity(text.len());
    let mut last: Option<char> = None;
    for (i, &c) in chars.iter().enumerate() {
        let keep = match c {
            c if c.is_control() => false,
            '\u{200d}' => last.is_some_and(|l| emoji(l) || l == '\u{fe0f}') && chars.get(i + 1).is_some_and(|&n| emoji(n)),
            '\u{fe00}'..='\u{fe0f}' | '\u{e0020}'..='\u{e007f}' => last.is_some_and(|l| emoji(l) || ('\u{e0020}'..='\u{e007f}').contains(&l)),
            c => !invisible(c),
        };
        if keep {
            kept.push(c);
            last = Some(c);
        }
    }
    kept
}

/// The Latin letter a letter of another script is drawn like (lower case),
/// or a digit like a letter: the confusables a name could hide behind.
///
/// Cherokee capitals are drawn like Latin ones (Ꭺ, Ꮪ), but lower case first
/// turns them into Cherokee small letters (U+AB70 on), which look nothing like
/// Latin, so it's those that are matched here, each beside its capital. The
/// pairs come from Unicode's confusables list.
fn latin(c: char) -> char {
    match c {
        'а' | 'α' | 'ɑ' => 'a',
        'в' | 'β' => 'b',
        'с' | 'ϲ' => 'c',
        'ԁ' => 'd',
        'е' | 'ё' | 'ε' => 'e',
        'ɡ' | 'ց' => 'g',
        'н' | 'һ' | 'η' | 'հ' => 'h',
        'і' | 'ї' | 'ι' | 'ı' => 'i',
        'ј' | 'ϳ' => 'j',
        'к' | 'κ' => 'k',
        'ӏ' | '1' | '|' => 'l',
        'м' | 'μ' => 'm',
        'ո' => 'n',
        'о' | 'ο' | '0' | 'օ' => 'o',
        'р' | 'ρ' => 'p',
        'ԛ' => 'q',
        'ѕ' => 's',
        'т' | 'τ' => 't',
        'υ' | 'ս' => 'u',
        'ѵ' | 'ν' => 'v',
        'ԝ' => 'w',
        'х' | 'χ' => 'x',
        'у' | 'ү' => 'y',
        'ζ' => 'z',
        // Cherokee, small and capital: Ꭺ Ᏼ Ꮯ Ꭰ Ꭼ Ꮐ Ꮋ Ꭻ Ꮶ Ꮮ Ꮇ Ꮲ Ꭱ Ꮪ Ꭲ Ꮩ Ꮃ Ꮓ.
        '\u{ab7a}' | '\u{13aa}' => 'a',
        '\u{13fc}' | '\u{13f4}' => 'b',
        '\u{abaf}' | '\u{13df}' => 'c',
        '\u{ab70}' | '\u{13a0}' => 'd',
        '\u{ab7c}' | '\u{13ac}' => 'e',
        '\u{ab90}' | '\u{13c0}' => 'g',
        '\u{ab8b}' | '\u{13bb}' => 'h',
        '\u{ab7b}' | '\u{13ab}' => 'j',
        '\u{abb6}' | '\u{13e6}' => 'k',
        '\u{abae}' | '\u{13de}' => 'l',
        '\u{ab87}' | '\u{13b7}' => 'm',
        '\u{abb2}' | '\u{13e2}' => 'p',
        '\u{ab71}' | '\u{13a1}' => 'r',
        '\u{abaa}' | '\u{13da}' => 's',
        '\u{ab72}' | '\u{13a2}' => 't',
        '\u{aba9}' | '\u{13d9}' => 'v',
        '\u{ab83}' | '\u{13b3}' => 'w',
        '\u{ab93}' | '\u{13c3}' => 'z',
        c => c,
    }
}

/// A name as it looks, to tell whether two could be taken for one another:
/// cleaned, in compatibility form (full-width letters, ligatures and the
/// like as plain letters), lower case, letters of other scripts that look
/// Latin as Latin, without spaces.
pub fn skeleton(name: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    clean(name).nfkc().flat_map(char::to_lowercase).filter(|c| !c.is_whitespace()).map(latin).collect()
}

/// A name with control characters taken out and spaces trimmed, cut to
/// [`NAME_MAX`] characters; `None` when nothing is left. For names that
/// come from payment codes.
pub fn clean_name(text: &str) -> Option<String> {
    let kept = clean(text);
    let cut: String = kept.trim().chars().take(NAME_MAX).collect();
    let cut = cut.trim_end();
    (!cut.is_empty()).then(|| cut.to_owned())
}

/// A typed name, checked: 1 to [`NAME_MAX`] characters once trimmed (control
/// characters, a pasted line break, dropped).
pub fn check_name(text: &str) -> Result<String> {
    let kept = clean(text);
    let name = kept.trim();
    match name.chars().count() {
        0 => Err(CodeError::NameEmpty),
        n if n > NAME_MAX => Err(CodeError::NameTooLong { max: NAME_MAX }),
        _ => Ok(name.to_owned()),
    }
}

/// A typed note, checked: blank is none, at most [`NOTE_MAX`] characters.
pub fn check_note(text: Option<&str>) -> Result<Option<String>> {
    let kept: Option<String> = text.map(clean);
    let Some(note) = kept.as_deref().map(str::trim).filter(|n| !n.is_empty()) else { return Ok(None) };
    if note.chars().count() > NOTE_MAX {
        return Err(CodeError::NoteTooLong { max: NOTE_MAX });
    }
    Ok(Some(note.to_owned()))
}

/// A little-endian number in `bytes` (at most four of them).
fn le(bytes: &[u8]) -> u32 {
    bytes.iter().rev().fold(0, |n, b| n << 8 | u32::from(*b))
}

/// The size a lossy (`VP8 `) image says it is: a key frame's, after its
/// start code.
fn vp8_size(body: &[u8]) -> Option<(u32, u32)> {
    let key_frame = body.first()? & 1 == 0;
    let start = body.get(3..6)? == [0x9d, 0x01, 0x2a];
    let (width, height) = (le(body.get(6..8)?) & 0x3fff, le(body.get(8..10)?) & 0x3fff);
    (key_frame && start).then_some((width, height))
}

/// The size a lossless (`VP8L`) image says it is: 14 bits each, less one.
fn vp8l_size(body: &[u8]) -> Option<(u32, u32)> {
    let bits = le(body.get(1..5)?);
    // The signature, then version 0 in the top three bits.
    (body[0] == 0x2f && bits >> 29 == 0).then_some(((bits & 0x3fff) + 1, (bits >> 14 & 0x3fff) + 1))
}

/// The pixel size of a WebP Fob keeps, from its headers: one still image,
/// lossy or lossless, alone or after an extended header (`VP8X`) and its
/// alpha. Anything else (animation, metadata, a chunk Fob doesn't know,
/// sizes that don't add up) is `None`, so no decoder is handed it.
fn webp_size(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.get(..4)? != b"RIFF" || bytes.get(8..12)? != b"WEBP" || le(&bytes[4..8]) as usize != bytes.len() - 8 {
        return None;
    }
    let (mut rest, mut canvas, mut image) = (&bytes[12..], None, None);
    let mut first = true;
    while !rest.is_empty() {
        let tag = rest.get(..4)?;
        let size = le(rest.get(4..8)?) as usize;
        let body = rest.get(8..8 + size)?;
        // A chunk of odd size is padded to even.
        rest = rest.get(8 + size + (size & 1)..)?;
        match tag {
            // The extended header: flags (no animation), three bytes kept, then the canvas, 24 bits a side, less one.
            b"VP8X" if first && size >= 10 && body[0] & 0x02 == 0 => canvas = Some((le(&body[4..7]) + 1, le(&body[7..10]) + 1)),
            b"ALPH" if canvas.is_some() && image.is_none() => {}
            b"VP8 " if image.is_none() => image = Some(vp8_size(body)?),
            b"VP8L" if image.is_none() => image = Some(vp8l_size(body)?),
            _ => return None,
        }
        first = false;
    }
    let image = image?;
    canvas.is_none_or(|c| c == image).then_some(image)
}

/// A photo Fob keeps: a still WebP of at most [`PHOTO_MAX`] bytes and
/// [`PHOTO_SIDE_MAX`] pixels a side, its headers read before anything
/// decodes it (a few bytes can claim a picture that takes a gigabyte to
/// draw).
pub fn check_photo(bytes: &[u8]) -> Result<()> {
    if bytes.len() > PHOTO_MAX {
        return Err(CodeError::PhotoTooBig { bytes: bytes.len(), max: PHOTO_MAX });
    }
    let Some((width, height)) = webp_size(bytes) else { return Err(CodeError::PhotoNotWebp) };
    if width == 0 || height == 0 || width > PHOTO_SIDE_MAX || height > PHOTO_SIDE_MAX {
        return Err(CodeError::PhotoTooWide { width, height, max: PHOTO_SIDE_MAX });
    }
    Ok(())
}

/// A valid lossless WebP header of `side` pixels a side, padded to `len`
/// bytes (even, at least 26; an odd length is never a valid WebP), for
/// tests.
#[cfg(any(test, feature = "test-support"))]
pub fn test_webp(len: usize, side: u32) -> Vec<u8> {
    let size = (len - 20) & !1;
    let bits = (side - 1) | (side - 1) << 14;
    let mut bytes = b"RIFF".to_vec();
    bytes.extend_from_slice(&(len as u32 - 8).to_le_bytes());
    bytes.extend_from_slice(b"WEBPVP8L");
    bytes.extend_from_slice(&(size as u32).to_le_bytes());
    bytes.push(0x2f);
    bytes.extend_from_slice(&bits.to_le_bytes());
    let header = bytes.len();
    bytes.extend((header..len).map(|i| (i * 37 % 251) as u8));
    bytes
}

/// The app's encoding of a photo as Fob keeps it, or `None` when it isn't
/// one (too big, say), for its loop of lowering the quality, then the size,
/// until it is. Newer Androids' WebP encoder adds an sRGB colour profile
/// (an `ICCP` chunk of about 450 bytes) under an extended header; Fob's
/// photos are sRGB anyway, so the profile is taken out, and the extended
/// header too when that leaves it nothing to say (no alpha).
pub fn photo_kept(bytes: &[u8]) -> Option<Vec<u8>> {
    let photo = without_profile(bytes).unwrap_or_else(|| bytes.to_vec());
    check_photo(&photo).ok().map(|()| photo)
}

/// A note as a request keeps it: control characters and the marks that
/// turn text round taken out, trimmed, cut to what a payment code holds.
pub fn clean_note(note: Option<&str>) -> Option<String> {
    let kept = clean(note?);
    let fitted = uri::fit_note(kept.trim()).0;
    let fitted = fitted.trim_end();
    (!fitted.is_empty()).then(|| fitted.to_owned())
}

/// `bytes` without its colour profile, or `None` if it has none (or isn't
/// a WebP whose chunks add up).
fn without_profile(bytes: &[u8]) -> Option<Vec<u8>> {
    if bytes.get(..4)? != b"RIFF" || bytes.get(8..12)? != b"WEBP" {
        return None;
    }
    let mut chunks = vec![];
    let mut rest = &bytes[12..];
    while !rest.is_empty() {
        let size = le(rest.get(4..8)?) as usize;
        let (chunk, after) = rest.split_at_checked(8 + size + (size & 1))?;
        chunks.push(chunk);
        rest = after;
    }
    let (header, others) = chunks.split_first()?;
    if &header[..4] != b"VP8X" || header.len() < 18 || !others.iter().any(|c| &c[..4] == b"ICCP") {
        return None;
    }
    let mut header = header.to_vec();
    // The extended header's flags: the profile's goes.
    header[8] &= !0x20;
    let mut body = if header[8] == 0 { vec![] } else { header };
    for chunk in others.iter().filter(|c| &c[..4] != b"ICCP") {
        body.extend_from_slice(chunk);
    }
    let mut photo = b"RIFF".to_vec();
    photo.extend_from_slice(&(body.len() as u32 + 4).to_le_bytes());
    photo.extend_from_slice(b"WEBP");
    photo.extend(body);
    Some(photo)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Greek, IPA, Armenian and Cherokee letters drawn like Latin ones don't
    /// make a second "Sam" that looks like the first.
    #[test]
    fn lookalikes_from_other_scripts_have_the_latin_skeleton() {
        assert_eq!(skeleton("Sam"), "sam");
        assert_eq!(skeleton("Sαm"), "sam", "Greek alpha");
        assert_eq!(skeleton("Sɑm"), "sam", "Latin alpha");
        assert_eq!(skeleton("Ꮪam"), "sam", "Cherokee DU, which lower case makes U+ABAA");
        assert_eq!(skeleton("ᏚᎪᎷ"), "sam", "all Cherokee capitals");
        assert_eq!(skeleton("Ꭺnna"), "anna");
        assert_eq!(skeleton("Ᏼob"), "bob", "Cherokee YV, small U+13FC");
        assert_eq!(skeleton("Jօhո"), "john", "Armenian oh and vo");
        assert_eq!(skeleton("\u{13b3}\u{13ac}\u{13de}"), "wel", "Cherokee LA, GV, TLE");
        assert_eq!(skeleton("Gսs"), skeleton("Gus"), "Armenian seh");
        assert_ne!(skeleton("Sam"), skeleton("Sum"), "a real difference stays one");
    }

    /// `real` (a plain one) as Android 16 encodes it: an extended
    /// header with the profile flag (and `flags` besides), an sRGB profile,
    /// then the image's chunks.
    fn with_profile(real: &[u8], flags: u8) -> Vec<u8> {
        let (body, header) = if &real[12..16] == b"VP8X" { (&real[30..], Some(&real[20..30])) } else { (&real[12..], None) };
        let mut x = header.map_or_else(|| vec![0, 0, 0, 0, 15, 0, 0, 15, 0, 0], <[u8]>::to_vec);
        x[0] |= 0x20 | flags;
        let mut bytes = b"RIFF\0\0\0\0WEBPVP8X\x0a\0\0\0".to_vec();
        bytes.extend(x);
        bytes.extend_from_slice(b"ICCP\xc8\x01\0\0");
        bytes.extend([7; 456]);
        bytes.extend_from_slice(body);
        let riff = (bytes.len() as u32 - 8).to_le_bytes();
        bytes[4..8].copy_from_slice(&riff);
        bytes
    }

    /// Android 16 puts an sRGB colour profile in every WebP it encodes:
    /// refused as it is, the profile is taken out and the photo kept.
    #[test]
    fn a_colour_profile_is_taken_out_of_a_photo() {
        for (real, flags) in [(LOSSY, 0), (LOSSLESS, 0), (ALPHA, 0x10)] {
            let plain = hex::decode(real).unwrap();
            let profiled = with_profile(&plain, flags);
            assert!(check_photo(&profiled).is_err(), "Fob reads no profile");
            assert_eq!(photo_kept(&profiled), Some(plain.clone()), "{real}");
            assert_eq!(photo_kept(&plain), Some(plain), "nothing to take out");
        }
        // Still too big without it: refused.
        let big = test_webp(PHOTO_MAX + 2, 16);
        assert_eq!(photo_kept(&with_profile(&big, 0)), None);
        assert_eq!(photo_kept(b"RIFF\x04\0\0\0WEBPVP8X"), None);
    }

    #[test]
    fn names_notes_and_photos_are_checked() {
        assert_eq!(check_name("  Sam \n").unwrap(), "Sam");
        assert!(check_name("   ").is_err());
        assert!(check_name(&"a".repeat(41)).is_err());
        // Characters, not bytes: 40 accented letters are fine.
        assert_eq!(check_name(&"é".repeat(40)).unwrap().chars().count(), 40);
        assert_eq!(check_note(Some("  ")).unwrap(), None);
        assert_eq!(check_note(Some(" Lunch money ")).unwrap().as_deref(), Some("Lunch money"));
        assert!(check_note(Some(&"n".repeat(81))).is_err());
        assert!(photo_fits(&webp(PHOTO_MAX)));
        assert!(!photo_fits(&webp(PHOTO_MAX + 1)));
        // Only a header, saying the old check let it through.
        assert!(!photo_fits(b"RIFF\0\0\0\0WEBPVP8 \x07\x07\x07\x07"));
        let mut png = webp(100);
        png[..4].copy_from_slice(b"\x89PNG");
        assert!(!photo_fits(&png));
        assert!(!photo_fits(b"RIFF"));
    }

    #[test]
    fn photos_are_checked_by_the_size_their_headers_claim() {
        for real in [LOSSY, LOSSLESS, ALPHA] {
            let bytes = hex::decode(real).unwrap();
            assert_eq!(webp_size(&bytes), Some((16, 16)), "{real}");
            assert!(photo_fits(&bytes));
        }
        // Each form claiming 16383 × 16383 in a few bytes (a gigabyte to decode): refused.
        let forged = |real: &str, at: usize, side: &[u8]| {
            let mut bytes = hex::decode(real).unwrap();
            bytes[at..at + side.len()].copy_from_slice(side);
            bytes
        };
        let lossy = forged(LOSSY, 26, &[0xff, 0x3f, 0xff, 0x3f]);
        assert_eq!(webp_size(&lossy), Some((16383, 16383)));
        assert!(!photo_fits(&lossy));
        let lossless = forged(LOSSLESS, 21, &[0xff, 0xff, 0xff, 0x0f]);
        assert_eq!(webp_size(&lossless), Some((16384, 16384)));
        assert!(!photo_fits(&lossless));
        // An extended header's canvas, and the image inside it, must agree, and both be small.
        let canvas = forged(ALPHA, 24, &[0xff, 0xff, 0xff, 0xff, 0xff, 0xff]);
        assert!(webp_size(&canvas).is_none());
        let both = forged(&hex::encode(&canvas), 58, &[0xff, 0x3f, 0xff, 0x3f]);
        assert!(webp_size(&both).is_none(), "16383 against a 16777216 canvas");
        // 129 a side is one too many; 128 is fine.
        assert!(photo_fits(&test_webp(200, 128)));
        assert!(!photo_fits(&test_webp(200, 129)));
        // Animation, metadata, an unknown chunk, a second image, a chunk running past the end, a wrong RIFF size: refused.
        let mut animated = hex::decode(ALPHA).unwrap();
        animated[20] |= 0x02;
        assert!(!photo_fits(&animated));
        let mut exif = hex::decode(LOSSLESS).unwrap();
        exif.extend_from_slice(b"EXIF\x02\0\0\0ab");
        let riff = (exif.len() as u32 - 8).to_le_bytes();
        exif[4..8].copy_from_slice(&riff);
        assert!(!photo_fits(&exif));
        let mut twice = hex::decode(LOSSLESS).unwrap();
        twice.extend_from_slice(&hex::decode(LOSSLESS).unwrap()[12..]);
        let riff = (twice.len() as u32 - 8).to_le_bytes();
        twice[4..8].copy_from_slice(&riff);
        assert!(!photo_fits(&twice));
        let mut long = hex::decode(LOSSLESS).unwrap();
        long[16] = 0x40;
        assert!(!photo_fits(&long));
        let mut riff = hex::decode(LOSSY).unwrap();
        riff[4] += 2;
        assert!(!photo_fits(&riff));
        // A lossy frame that isn't a key frame, a lossless one of another version.
        assert!(!photo_fits(&forged(LOSSY, 20, &[0x31])));
        assert!(!photo_fits(&forged(LOSSLESS, 24, &[0x2f])));
    }

    fn photo_fits(bytes: &[u8]) -> bool {
        photo_kept(bytes).is_some()
    }

    /// Encoded by libwebp: 16 × 16, lossy, lossless, and lossy with alpha
    /// (an extended header, then ALPH and VP8).
    const LOSSY: &str = "524946463600000057454250565038202a000000f001009d012a1000100002c04c25a00274ba01f800044c0000fedf803fdf7a03793fff929aff1b71d800";

    const LOSSLESS: &str = "524946461e000000574542505650384c110000002f0fc003000750a82297a2ff8188e87f0000";

    const ALPHA: &str = "524946465a00000057454250565038580a000000100000000f00000f0000414c50480a000000010750c088084444ff03565038202a000000f001009d012a1000100002c04c25a00274ba01f800044c0000fedf803fdf7a03793fff929aff1b71d800";

    fn webp(len: usize) -> Vec<u8> {
        test_webp(len, 96)
    }
}
