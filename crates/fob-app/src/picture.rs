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
pub fn photo_pixels(photo: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let kept = fob_ffi::contact_photo_kept(photo.to_vec())?;
    let mut decoder = image_webp::WebPDecoder::new(std::io::Cursor::new(kept)).ok()?;
    let (width, height) = decoder.dimensions();
    let mut pixels = vec![0u8; decoder.output_buffer_size()?];
    decoder.read_image(&mut pixels).ok()?;
    if decoder.has_alpha() {
        return Some((width, height, pixels));
    }
    Some((width, height, pixels.as_chunks::<3>().0.iter().flat_map(|p| [p[0], p[1], p[2], 255]).collect()))
}

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

    #[test]
    fn a_picture_with_no_code_reads_as_none() {
        assert_eq!(read_code(40, 40, &vec![255u8; 1600]), None);
        assert_eq!(read_code(40, 40, &[0u8; 10]), None, "fewer pixels than it says");
        assert_eq!(read_code(0, 0, &[]), None);
    }
}
