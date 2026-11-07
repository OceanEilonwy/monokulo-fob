//! Contact photos drawn in memory-safe Rust, for every app.
//!
//! A contact's photo comes from someone else: a profile code, or a phone
//! touched while Meet is open, which saves it without asking. Fob checks only
//! its headers when it keeps one (`contacts::photo_kept`), not the compressed
//! picture inside, so handing it to the platform's C decoder (Android's Skia
//! libwebp) would let a stranger's bytes reach the kind of bug CVE-2023-4863
//! was. It's decoded here instead, with the pure-Rust image-webp, and the apps
//! only ever see pixels. It lives in this crate, not fob-app (which desktop
//! already used for it), because fob-app depends on this one.

use fob_wallet::contacts;

/// A contact's photo as pixels: `width` × `height`, four bytes a pixel, red,
/// green, blue and alpha, not premultiplied.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct DecodedPhoto {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// A kept photo's pixels; none when it isn't a photo Fob keeps, or doesn't
/// decode. Photos from other people are drawn only through this, never by the
/// platform's own image decoder.
#[uniffi::export]
pub fn decode_photo(photo: Vec<u8>) -> Option<DecodedPhoto> {
    photo_pixels(&photo).map(|(width, height, rgba)| DecodedPhoto { width, height, rgba })
}

/// A kept photo's pixels: (width, height, RGBA). Only what Fob would keep is
/// decoded, so the picture is at most [`contacts::PHOTO_SIDE_MAX`] a side and
/// the decoder never allocates more than that needs.
pub fn photo_pixels(photo: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let kept = contacts::photo_kept(photo)?;
    let mut decoder = image_webp::WebPDecoder::new(std::io::Cursor::new(kept)).ok()?;
    let (width, height) = decoder.dimensions();
    // The headers photo_kept read and the decoder's own agree, but a photo
    // that somehow says otherwise is refused rather than drawn big.
    if width == 0 || height == 0 || width > contacts::PHOTO_SIDE_MAX || height > contacts::PHOTO_SIDE_MAX {
        return None;
    }
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
    fn a_kept_photo_draws_and_anything_else_does_not() {
        let encode = |side: u32, color: image_webp::ColorType, pixel: &[u8]| {
            let mut bytes = Vec::new();
            image_webp::WebPEncoder::new(&mut bytes).encode(&pixel.repeat((side * side) as usize), side, side, color).unwrap();
            bytes
        };
        // Opaque: drawn with an alpha byte of 255 added.
        let photo = decode_photo(encode(16, image_webp::ColorType::Rgb8, &[200, 10, 10])).expect("a kept photo draws");
        assert_eq!((photo.width, photo.height, photo.rgba.len()), (16, 16, 16 * 16 * 4));
        assert_eq!(&photo.rgba[..4], &[200, 10, 10, 255]);
        // With alpha: as it is.
        let photo = decode_photo(encode(8, image_webp::ColorType::Rgba8, &[0, 0, 255, 128])).unwrap();
        assert_eq!(&photo.rgba[..4], &[0, 0, 255, 128]);
        // Wider than Fob keeps: not decoded at all.
        assert_eq!(decode_photo(encode(contacts::PHOTO_SIDE_MAX + 1, image_webp::ColorType::Rgb8, &[0, 0, 0])), None);
        assert_eq!(decode_photo(b"RIFF\x04\0\0\0WEBPVP8X".to_vec()), None);
        assert_eq!(decode_photo(b"not a photo".to_vec()), None);
        assert_eq!(decode_photo(Vec::new()), None);
    }
}
