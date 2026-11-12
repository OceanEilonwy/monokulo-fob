//! Pictures on a device with no camera: reading a QR code from one, and a
//! contact's photo made small enough for Rust to keep, and drawn again.

/// The text of the first QR code in a greyscale picture, `width` × `height`,
/// one byte a pixel; `None` when there's no code it can read.
pub fn read_code(width: usize, height: usize, grey: &[u8]) -> Option<String> {
    if width == 0 || height == 0 || grey.len() < width.checked_mul(height)? {
        return None;
    }
    let mut image = rqrr::PreparedImage::prepare_from_greyscale(width, height, |x, y| grey[y * width + x]);
    image.detect_grids().into_iter().find_map(|grid| grid.decode().ok().map(|(_, text)| text))
}

/// How big a picture may be before a client hands it to a decoder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub bytes: usize,
    /// The longer side, in pixels.
    pub side: u32,
    pub pixels: u64,
}

/// A picture of someone's code: it comes from someone else, so it's held to
/// what a screenshot or a photo of a QR code needs. 4096 × 4096 is 64 MiB as
/// RGBA, the most a decoder is let allocate for one.
pub const CODE_PICTURE: Limits = Limits { bytes: 4 << 20, side: 4096, pixels: 4096 * 4096 };

/// A picture of your own, made into your photo: a phone camera's full size
/// passes, a decompression bomb doesn't.
pub const OWN_PHOTO: Limits = Limits { bytes: 32 << 20, side: 12_000, pixels: 50_000_000 };

/// Whether `bytes` is a picture within `limits`, by the size its header says.
///
/// A desktop decodes pictures with the system's C libraries (GDK, gdk-pixbuf),
/// which Fob doesn't replace. What it can do is hand them only small pictures in
/// formats whose size it can read first: PNG, JPEG, GIF, WebP and BMP. Anything
/// else, TIFF and the rest of gdk-pixbuf's loaders among them, is refused, as
/// a code is never sent that way.
///
/// A GIF's header gives only its screen: each frame says its own size, and
/// gdk-pixbuf decodes every frame. So a GIF passes only with one frame,
/// inside that screen: a code is never animated.
pub fn picture_fits(bytes: &[u8], limits: Limits) -> bool {
    if bytes.len() > limits.bytes {
        return false;
    }
    let Some((width, height)) = picture_size(bytes) else { return false };
    let fits = width > 0 && height > 0 && width.max(height) <= limits.side && u64::from(width) * u64::from(height) <= limits.pixels;
    fits && (!bytes.starts_with(b"GIF") || gif_one_frame(bytes, width, height))
}

/// Whether a GIF, walked block by block, holds exactly one frame that lies
/// within its `width` × `height` screen. Anything that doesn't walk is refused.
fn gif_one_frame(bytes: &[u8], width: u32, height: u32) -> bool {
    let le16 = |at: usize| bytes.get(at..at + 2).map(|b| u32::from(u16::from_le_bytes([b[0], b[1]])));
    // Data sub-blocks from `at`: where the zero-length one that ends them ends.
    let past_blocks = |mut at: usize| -> Option<usize> {
        loop {
            let len = usize::from(*bytes.get(at)?);
            at += 1 + len;
            if len == 0 {
                return Some(at);
            }
        }
    };
    let table = |flags: u8| if flags & 0x80 != 0 { 3 << ((flags & 7) + 1) } else { 0 };
    let Some(&flags) = bytes.get(10) else { return false };
    let mut at = 13 + table(flags);
    let mut frames = 0;
    loop {
        match bytes.get(at) {
            // The trailer.
            Some(0x3b) => return frames == 1,
            // An extension: its label, then its sub-blocks.
            Some(0x21) => match past_blocks(at + 2) {
                Some(next) => at = next,
                None => return false,
            },
            Some(0x2c) => {
                let (Some(left), Some(top), Some(w), Some(h), Some(&flags)) = (le16(at + 1), le16(at + 3), le16(at + 5), le16(at + 7), bytes.get(at + 9)) else { return false };
                frames += 1;
                if frames > 1 || left + w > width || top + h > height {
                    return false;
                }
                // The local colour table, the LZW code size, then the pixels.
                match past_blocks(at + 10 + table(flags) + 1) {
                    Some(next) => at = next,
                    None => return false,
                }
            }
            // Cut short after its frame, it still holds only that one.
            None => return frames == 1,
            Some(_) => return false,
        }
    }
}

/// The width and height a picture's header gives, for the formats [`picture_fits`] takes.
pub fn picture_size(bytes: &[u8]) -> Option<(u32, u32)> {
    let be32 = |at: usize| Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?));
    let le16 = |at: usize| Some(u32::from(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?)));
    let le24 = |at: usize| Some(u32::from_le_bytes([*bytes.get(at)?, *bytes.get(at + 1)?, *bytes.get(at + 2)?, 0]));
    let le32 = |at: usize| Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?));
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") && bytes.get(12..16) == Some(b"IHDR") {
        return Some((be32(16)?, be32(20)?));
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some((le16(6)?, le16(8)?));
    }
    if bytes.starts_with(b"BM") {
        // Height is negative for a picture stored top down.
        return Some((le32(18)?, (le32(22)? as i32).unsigned_abs()));
    }
    if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        return match bytes.get(12..16)? {
            b"VP8 " if bytes.get(23..26) == Some(&[0x9d, 0x01, 0x2a]) => Some((le16(26)? & 0x3fff, le16(28)? & 0x3fff)),
            b"VP8L" if bytes.get(20) == Some(&0x2f) => {
                let bits = le32(21)?;
                Some(((bits & 0x3fff) + 1, ((bits >> 14) & 0x3fff) + 1))
            }
            b"VP8X" => Some((le24(24)? + 1, le24(27)? + 1)),
            _ => None,
        };
    }
    if bytes.starts_with(&[0xff, 0xd8]) {
        return jpeg_size(bytes);
    }
    None
}

/// A JPEG's size, from its first start-of-frame marker.
fn jpeg_size(bytes: &[u8]) -> Option<(u32, u32)> {
    let mut at = 2;
    loop {
        // Markers may be padded with any number of 0xFF.
        while *bytes.get(at)? == 0xff && *bytes.get(at + 1)? == 0xff {
            at += 1;
        }
        if *bytes.get(at)? != 0xff {
            return None;
        }
        let marker = *bytes.get(at + 1)?;
        at += 2;
        match marker {
            // Markers with no length.
            0x01 | 0xd0..=0xd7 => continue,
            // End of image, or the scan begun, with no frame seen.
            0xd9 | 0xda => return None,
            // Every start of frame, but DHT (C4), JPG (C8) and DAC (CC).
            0xc0..=0xcf if !matches!(marker, 0xc4 | 0xc8 | 0xcc) => {
                let height = u16::from_be_bytes(bytes.get(at + 3..at + 5)?.try_into().ok()?);
                let width = u16::from_be_bytes(bytes.get(at + 5..at + 7)?.try_into().ok()?);
                return Some((u32::from(width), u32::from(height)));
            }
            _ => {
                let length = usize::from(u16::from_be_bytes(bytes.get(at..at + 2)?.try_into().ok()?));
                if length < 2 {
                    return None;
                }
                at += length;
            }
        }
    }
}

/// (side, WebP quality) tried in turn until the photo fits: quality 90 down to
/// 40 at the larger sides, and only when none fits, lower ones and smaller
/// sides, so a busy photo fits without every other one going blocky.
const PHOTO_STEPS: [(u32, f32); 24] = [
    (128, 90.0), (128, 80.0), (128, 70.0), (128, 60.0), (128, 50.0), (128, 40.0),
    (112, 90.0), (112, 80.0), (112, 70.0), (112, 60.0), (112, 50.0), (112, 40.0),
    (96, 90.0), (96, 80.0), (96, 70.0), (96, 60.0), (96, 50.0), (96, 40.0),
    (96, 30.0), (96, 20.0), (72, 30.0), (72, 10.0), (64, 10.0), (48, 0.0),
];

/// A picture's centre as a small square WebP that Rust keeps, from `width` ×
/// `height` RGBA; `None` when it can't be made to fit.
pub fn small_photo(width: u32, height: u32, rgba: &[u8]) -> Option<Vec<u8>> {
    let side = width.min(height);
    let needed = (width as usize).checked_mul(height as usize)?.checked_mul(4)?;
    if side == 0 || rgba.len() < needed {
        return None;
    }
    let (left, top) = ((width - side) / 2, (height - side) / 2);
    PHOTO_STEPS.iter().find_map(|&(to, quality)| {
        let to = to.min(side);
        let pixels = shrink(rgba, width, left, top, side, to);
        let photo = webp::Encoder::from_rgba(&pixels, to, to).encode(quality).to_vec();
        fob_ffi::contact_photo_kept(photo)
    })
}

/// A square of a picture at a smaller side, each pixel the mean of those it covers.
fn shrink(rgba: &[u8], width: u32, left: u32, top: u32, side: u32, to: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity((to * to * 4) as usize);
    for y in 0..to {
        let (y0, y1) = (y * side / to, ((y + 1) * side / to).max(y * side / to + 1));
        for x in 0..to {
            let (x0, x1) = (x * side / to, ((x + 1) * side / to).max(x * side / to + 1));
            let mut sum = [0u64; 4];
            for sy in y0..y1 {
                for sx in x0..x1 {
                    let at = (((top + sy) as usize) * width as usize + (left + sx) as usize) * 4;
                    for (total, byte) in sum.iter_mut().zip(&rgba[at..at + 4]) {
                        *total += u64::from(*byte);
                    }
                }
            }
            let count = u64::from((y1 - y0) * (x1 - x0));
            out.extend(sum.map(|total| (total / count) as u8));
        }
    }
    out
}

/// A kept photo's pixels: (width, height, RGBA). Photos come from other
/// people's codes, so this is memory-safe Rust and only reads what Rust kept.
/// The decoder is fob-ffi's, which Android draws contact photos with too.
pub use fob_ffi::photo::photo_pixels;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_big_picture_becomes_a_photo_rust_keeps_and_it_draws_again() {
        let (w, h) = (640u32, 480u32);
        // Noise, the hardest thing to make small.
        let mut seed = 7u32;
        let rgba: Vec<u8> = (0..w * h * 4)
            .map(|i| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                if i % 4 == 3 { 255 } else { (seed >> 24) as u8 }
            })
            .collect();
        let photo = small_photo(w, h, &rgba).expect("fits at some step");
        assert!(photo.len() <= fob_wallet::contacts::PHOTO_MAX);
        let (pw, ph, pixels) = photo_pixels(&photo).expect("draws");
        assert_eq!(pw, ph);
        assert!(pw <= 128);
        assert_eq!(pixels.len(), (pw * ph * 4) as usize);
    }

    #[test]
    fn a_small_picture_is_not_grown_and_nonsense_is_not_read() {
        let flat = vec![200u8; 40 * 20 * 4];
        let (w, h, _) = photo_pixels(&small_photo(40, 20, &flat).unwrap()).unwrap();
        assert_eq!((w, h), (20, 20));
        assert_eq!(small_photo(0, 0, &[]), None);
        assert_eq!(small_photo(40, 20, &flat[..10]), None);
        assert_eq!(photo_pixels(b"RIFF\x04\0\0\0WEBPVP8X"), None);
    }

    /// A code as `fob-ffi` gives it, drawn with a quiet zone at 4 pixels a module.
    fn drawn(text: &str) -> (usize, Vec<u8>) {
        let code = fob_ffi::qr_code(text.into()).unwrap();
        let (size, scale, quiet) = (code.size as usize, 4, 4);
        let side = (size + 2 * quiet) * scale;
        let mut grey = vec![255u8; side * side];
        for y in 0..size {
            for x in 0..size {
                if code.dark[y * size + x] {
                    for py in 0..scale {
                        for px in 0..scale {
                            grey[((y + quiet) * scale + py) * side + (x + quiet) * scale + px] = 0;
                        }
                    }
                }
            }
        }
        (side, grey)
    }

    #[test]
    fn a_code_in_a_picture_is_read() {
        let (side, grey) = drawn("monero:hello");
        assert_eq!(read_code(side, side, &grey).as_deref(), Some("monero:hello"));
    }

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        bytes.extend(width.to_be_bytes());
        bytes.extend(height.to_be_bytes());
        bytes.extend([8, 6, 0, 0, 0]);
        bytes
    }

    fn jpeg(width: u16, height: u16) -> Vec<u8> {
        // SOI, an APP0 segment to skip, then a baseline frame.
        let mut bytes = vec![0xff, 0xd8, 0xff, 0xe0, 0x00, 0x04, 0x4a, 0x46, 0xff, 0xc0, 0x00, 0x11, 0x08];
        bytes.extend(height.to_be_bytes());
        bytes.extend(width.to_be_bytes());
        bytes.extend([3, 1, 0x22, 0, 2, 0x11, 1, 3, 0x11, 1]);
        bytes
    }

    #[test]
    fn a_picture_is_held_to_its_limits_before_it_is_decoded() {
        assert_eq!(picture_size(&png(640, 480)), Some((640, 480)));
        assert_eq!(picture_size(&jpeg(1024, 768)), Some((1024, 768)));
        assert_eq!(picture_size(b"GIF89a\x20\x00\x10\x00"), Some((32, 16)));
        assert!(picture_fits(&png(640, 480), CODE_PICTURE));
        assert!(picture_fits(&jpeg(4000, 3000), CODE_PICTURE));
        // A decompression bomb says how big it is before it says anything else.
        assert!(!picture_fits(&png(30_000, 30_000), CODE_PICTURE));
        assert!(!picture_fits(&png(30_000, 30_000), OWN_PHOTO));
        assert!(!picture_fits(&jpeg(5000, 100), CODE_PICTURE), "too long a side");
        assert!(picture_fits(&jpeg(8000, 6000), OWN_PHOTO), "a camera's own size");
        assert!(!picture_fits(&png(0, 10), OWN_PHOTO));
        let mut big = png(100, 100);
        big.resize(CODE_PICTURE.bytes + 1, 0);
        assert!(!picture_fits(&big, CODE_PICTURE), "too many bytes");
        // Formats whose size isn't read first aren't handed on.
        assert!(!picture_fits(b"II*\0\x08\0\0\0", OWN_PHOTO), "TIFF");
        assert!(!picture_fits(b"not a picture", OWN_PHOTO));
        assert!(!picture_fits(&[0xff, 0xd8, 0xff, 0xda], OWN_PHOTO), "a JPEG with no frame");
    }

    /// A GIF with a `width` × `height` screen and these frames (left, top,
    /// width, height), each one pixel's worth of data.
    fn gif(width: u16, height: u16, frames: &[(u16, u16, u16, u16)]) -> Vec<u8> {
        let mut bytes = b"GIF89a".to_vec();
        bytes.extend(width.to_le_bytes());
        bytes.extend(height.to_le_bytes());
        // A two-colour global table, then the table.
        bytes.extend([0x80, 0, 0, 0, 0, 0, 255, 255, 255]);
        for &(left, top, w, h) in frames {
            // A graphic control extension, then the frame.
            bytes.extend([0x21, 0xf9, 4, 0, 0, 0, 0, 0, 0x2c]);
            for n in [left, top, w, h] {
                bytes.extend(n.to_le_bytes());
            }
            bytes.extend([0, 2, 2, 0x44, 0x01, 0]);
        }
        bytes.push(0x3b);
        bytes
    }

    /// A GIF's header gives its screen, not its frames: they're walked too.
    #[test]
    fn a_gif_passes_only_as_one_frame_inside_its_screen() {
        assert!(picture_fits(&gif(32, 16, &[(0, 0, 32, 16)]), CODE_PICTURE));
        assert!(picture_fits(&gif(32, 16, &[(4, 4, 8, 8)]), CODE_PICTURE));
        assert!(!picture_fits(&gif(32, 16, &[(0, 0, 32, 16), (0, 0, 32, 16)]), CODE_PICTURE), "animated");
        assert!(!picture_fits(&gif(32, 16, &[(0, 0, 65_535, 65_535)]), CODE_PICTURE), "a frame bigger than its screen");
        assert!(!picture_fits(&gif(32, 16, &[(30, 0, 8, 8)]), CODE_PICTURE), "a frame off its screen");
        assert!(!picture_fits(&gif(32, 16, &[]), CODE_PICTURE), "no frame");
        let mut cut = gif(32, 16, &[(0, 0, 32, 16)]);
        cut.truncate(cut.len() - 3);
        assert!(!picture_fits(&cut, CODE_PICTURE), "cut inside its frame");
        let mut odd = gif(32, 16, &[(0, 0, 32, 16)]);
        odd.insert(odd.len() - 1, 0x99);
        assert!(!picture_fits(&odd, CODE_PICTURE), "a block that isn't one");
        // Hundreds of frames in a few kilobytes, the memory bomb.
        let many: Vec<_> = (0..500).map(|_| (0, 0, 4096, 4096)).collect();
        assert!(!picture_fits(&gif(4096, 4096, &many), CODE_PICTURE));
    }

    #[test]
    fn a_picture_with_no_code_reads_as_none() {
        assert_eq!(read_code(40, 40, &vec![255u8; 1600]), None);
        assert_eq!(read_code(40, 40, &[0u8; 10]), None, "fewer pixels than it says");
        assert_eq!(read_code(0, 0, &[]), None);
    }
}
