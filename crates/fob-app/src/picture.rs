//! Reading a QR code from a picture, on a device with no camera.

/// The text of the first QR code in a greyscale picture, `width` × `height`,
/// one byte a pixel; `None` when there's no code it can read.
pub fn read_code(width: usize, height: usize, grey: &[u8]) -> Option<String> {
    if width == 0 || height == 0 || grey.len() < width.checked_mul(height)? {
        return None;
    }
    let mut image = rqrr::PreparedImage::prepare_from_greyscale(width, height, |x, y| grey[y * width + x]);
    image.detect_grids().into_iter().find_map(|grid| grid.decode().ok().map(|(_, text)| text))
}

#[cfg(test)]
mod tests {
    use super::*;

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
