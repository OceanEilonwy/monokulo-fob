//! A standard NFC tag a phone can emulate: NFC Forum Type 4, holding one
//! NDEF record, a `monero:` payment link.
//!
//! Receive offers it beside [`pay`](crate::pay)'s own protocol, so any
//! phone's own tag reading gets the payment code (Android then opens the
//! wallet registered for `monero:` links, Fob included, even when Fob
//! isn't running), and Fob reads the same kind of tag from any other
//! wallet, sticker or till. Everything here is bytes in, bytes out.
//!
//! The reader's commands, as the NFC Forum's Type 4 Tag spec has them:
//!
//! - `00 A4 04 00 07 <AID> [00]`: select the NDEF application ([`AID`]).
//! - `00 A4 00 0C 02 <id>`: select a file, the capability container
//!   ([`CC_ID`]) or the NDEF file ([`FILE_ID`]).
//! - `00 B0 <offset> <Le>`: read binary from the selected file.
//!
//! The NDEF file is a two-byte length then the message: one record of type
//! `U` (a URI), no prefix abbreviation, in the long form (a 4-byte payload
//! length) so a long note never needs the short one. Nothing can be written.

/// The NDEF Type 4 Tag application: D2760000850101.
pub const AID: [u8; 7] = [0xD2, 0x76, 0x00, 0x00, 0x85, 0x01, 0x01];
/// The capability container's file id.
pub const CC_ID: u16 = 0xE103;
/// The NDEF file's id.
pub const FILE_ID: u16 = 0xE104;
/// The most bytes one read returns, which the capability container says.
pub const MLE: u16 = 0x00F0;

const OK: [u8; 2] = [0x90, 0x00];
const NOT_FOUND: [u8; 2] = [0x6A, 0x82];
const WRONG_OFFSET: [u8; 2] = [0x6B, 0x00];
const DENIED: [u8; 2] = [0x69, 0x85];
const WRONG_INS: [u8; 2] = [0x6D, 0x00];

/// The capability container for an NDEF file of `size` bytes (its length
/// and the message): version 2.0, reads of up to [`MLE`], read only.
pub fn capability(size: u16) -> Vec<u8> {
    let mut cc = vec![0x00, 0x0F, 0x20];
    cc.extend_from_slice(&MLE.to_be_bytes());
    cc.extend_from_slice(&0x00F0u16.to_be_bytes());
    // The NDEF file control TLV: type 4, length 6, its id, its size, read open, write never.
    cc.extend_from_slice(&[0x04, 0x06]);
    cc.extend_from_slice(&FILE_ID.to_be_bytes());
    cc.extend_from_slice(&size.to_be_bytes());
    cc.extend_from_slice(&[0x00, 0xFF]);
    cc
}

/// One NDEF record: a URI with no prefix abbreviation, long form.
pub fn message(uri: &str) -> Vec<u8> {
    let payload_len = (1 + uri.len()) as u32;
    // MB, ME, TNF 1 (well known); no short record, no ID.
    let mut m = vec![0xC1, 0x01];
    m.extend_from_slice(&payload_len.to_be_bytes());
    m.push(b'U');
    m.push(0x00);
    m.extend_from_slice(uri.as_bytes());
    m
}

/// The NDEF file: the message's length, then the message.
pub fn file(uri: &str) -> Vec<u8> {
    let m = message(uri);
    let mut f = (m.len() as u16).to_be_bytes().to_vec();
    f.extend_from_slice(&m);
    f
}

/// The select command for the NDEF application.
pub fn select_app() -> Vec<u8> {
    let mut apdu = vec![0x00, 0xA4, 0x04, 0x00, AID.len() as u8];
    apdu.extend_from_slice(&AID);
    apdu.push(0x00);
    apdu
}

/// The select command for a file.
pub fn select_file(id: u16) -> Vec<u8> {
    let mut apdu = vec![0x00, 0xA4, 0x00, 0x0C, 0x02];
    apdu.extend_from_slice(&id.to_be_bytes());
    apdu
}

/// Read `len` bytes (at most 255) from `offset`.
pub fn read_binary(offset: usize, len: usize) -> Vec<u8> {
    let [hi, lo] = (offset as u16).to_be_bytes();
    vec![0x00, 0xB0, hi, lo, len as u8]
}

/// The URI in an NDEF message: its first URI record, read with the
/// prefix abbreviation the record names. Err says why it isn't one.
pub fn parse_uri(message: &[u8]) -> Result<String, crate::error::CodeError> {
    const PREFIXES: [&str; 5] = ["", "http://www.", "https://www.", "http://", "https://"];
    let bad = || crate::error::CodeError::TagHasNoCode;
    let mut at = 0;
    while at < message.len() {
        let flags = *message.get(at).ok_or_else(bad)?;
        let type_len = *message.get(at + 1).ok_or_else(bad)? as usize;
        let (payload_len, mut next) = if flags & 0x10 != 0 {
            (*message.get(at + 2).ok_or_else(bad)? as usize, at + 3)
        } else {
            let b = message.get(at + 2..at + 6).ok_or_else(bad)?;
            (u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize, at + 6)
        };
        let id_len = if flags & 0x08 != 0 {
            let n = *message.get(next).ok_or_else(bad)? as usize;
            next += 1;
            n
        } else {
            0
        };
        let kind = message.get(next..next + type_len).ok_or_else(bad)?;
        let payload_at = next + type_len + id_len;
        let payload = message.get(payload_at..payload_at + payload_len).ok_or_else(bad)?;
        if flags & 0x07 == 0x01 && kind == b"U" && !payload.is_empty() {
            let prefix = PREFIXES.get(payload[0] as usize).copied().unwrap_or("");
            let text = String::from_utf8(payload[1..].to_vec()).map_err(|_| bad())?;
            return Ok(format!("{prefix}{text}"));
        }
        at = payload_at + payload_len;
        if flags & 0x40 != 0 {
            break;
        }
    }
    Err(bad())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Open {
    Nothing,
    Capability,
    Message,
}

/// The tag side: answers a reader's commands with the capability container
/// and the NDEF file for one URI.
#[derive(Debug)]
pub struct Tag {
    file: Vec<u8>,
    capability: Vec<u8>,
    selected: bool,
    open: Open,
    /// Every byte of the message went into an answer.
    whole: bool,
}

impl Tag {
    pub fn new(uri: &str) -> Tag {
        let file = file(uri);
        let capability = capability(file.len() as u16);
        Tag { file, capability, selected: false, open: Open::Nothing, whole: false }
    }

    /// The link went: start over at the next select.
    pub fn lost(&mut self) {
        self.selected = false;
        self.open = Open::Nothing;
    }

    /// A reader read the whole message.
    pub fn read(&self) -> bool {
        self.whole
    }

    fn answer(data: &[u8], status: [u8; 2]) -> Vec<u8> {
        let mut out = data.to_vec();
        out.extend_from_slice(&status);
        out
    }

    /// The answer to one command.
    pub fn answer_to(&mut self, apdu: &[u8]) -> Vec<u8> {
        let Some((&[_cla, ins, p1, p2], body)) = apdu.split_first_chunk::<4>() else { return Self::answer(&[], WRONG_OFFSET) };
        match ins {
            0xA4 => {
                let lc = body.first().copied().unwrap_or(0) as usize;
                let name = body.get(1..1 + lc).unwrap_or(&[]);
                if p1 == 0x04 {
                    self.open = Open::Nothing;
                    self.selected = name == AID;
                    return Self::answer(&[], if self.selected { OK } else { NOT_FOUND });
                }
                if !self.selected || p1 != 0x00 {
                    return Self::answer(&[], NOT_FOUND);
                }
                self.open = match name {
                    [hi, lo] if u16::from_be_bytes([*hi, *lo]) == CC_ID => Open::Capability,
                    [hi, lo] if u16::from_be_bytes([*hi, *lo]) == FILE_ID => Open::Message,
                    _ => return Self::answer(&[], NOT_FOUND),
                };
                Self::answer(&[], OK)
            }
            0xB0 => {
                let content = match self.open {
                    Open::Capability => &self.capability,
                    Open::Message => &self.file,
                    Open::Nothing => return Self::answer(&[], NOT_FOUND),
                };
                let offset = (((p1 & 0x7F) as usize) << 8) | p2 as usize;
                if offset > content.len() {
                    return Self::answer(&[], WRONG_OFFSET);
                }
                // No Le (or Le 0) means as much as there is, up to 256.
                let le = match body.last() {
                    Some(0) | None => 256,
                    Some(n) => *n as usize,
                };
                let end = (offset + le).min(content.len());
                if self.open == Open::Message && end == content.len() {
                    self.whole = true;
                }
                Self::answer(&content[offset..end], OK)
            }
            // Update binary, and anything else: the tag is read only.
            0xD6 => Self::answer(&[], DENIED),
            _ => Self::answer(&[], WRONG_INS),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const URI: &str = "monero:73a4nWuvkYoYoksGurDbtGXvvmRmWQpdXK4UvLgFBkE8eQpGTUh1Cs2Zn9fFRb3VeDqqwgU83wqR1iKPo1EdoYN13DY6VM7?tx_amount=0.5&tx_description=Coffee";

    /// A whole read as a phone's tag reader does it.
    fn read_all(tag: &mut Tag) -> Vec<u8> {
        assert_eq!(tag.answer_to(&select_app()), OK.to_vec());
        assert_eq!(tag.answer_to(&select_file(CC_ID)), OK.to_vec());
        let cc = tag.answer_to(&read_binary(0, 15));
        assert_eq!(&cc[cc.len() - 2..], OK);
        assert_eq!(cc.len() - 2, 15);
        assert_eq!(tag.answer_to(&select_file(FILE_ID)), OK.to_vec());
        let len = tag.answer_to(&read_binary(0, 2));
        let nlen = u16::from_be_bytes([len[0], len[1]]) as usize;
        let mut got = vec![];
        while got.len() < nlen {
            let part = tag.answer_to(&read_binary(2 + got.len(), (nlen - got.len()).min(MLE as usize)));
            assert_eq!(&part[part.len() - 2..], OK);
            got.extend_from_slice(&part[..part.len() - 2]);
        }
        got
    }

    #[test]
    fn a_reader_gets_the_uri_from_the_tag() {
        let mut tag = Tag::new(URI);
        assert!(!tag.read());
        let message = read_all(&mut tag);
        assert_eq!(parse_uri(&message).unwrap(), URI);
        assert!(tag.read());
    }

    #[test]
    fn a_long_note_still_fits_one_record() {
        let long = format!("{URI}{}", "x".repeat(800));
        let mut tag = Tag::new(&long);
        assert_eq!(parse_uri(&read_all(&mut tag)).unwrap(), long);
    }

    #[test]
    fn the_capability_container_says_what_the_spec_needs() {
        let tag = Tag::new(URI);
        let cc = &tag.capability;
        assert_eq!(&cc[..3], [0x00, 0x0F, 0x20]);
        assert_eq!(&cc[7..9], [0x04, 0x06]);
        assert_eq!(u16::from_be_bytes([cc[9], cc[10]]), FILE_ID);
        assert_eq!(u16::from_be_bytes([cc[11], cc[12]]) as usize, tag.file.len());
        assert_eq!(&cc[13..], [0x00, 0xFF], "read open, never written");
    }

    #[test]
    fn nothing_is_read_before_the_application_and_a_file_are_selected_or_written_ever() {
        let mut tag = Tag::new(URI);
        assert_eq!(tag.answer_to(&read_binary(0, 2)), NOT_FOUND.to_vec());
        assert_eq!(tag.answer_to(&select_file(FILE_ID)), NOT_FOUND.to_vec());
        tag.answer_to(&select_app());
        assert_eq!(tag.answer_to(&select_file(0x1234)), NOT_FOUND.to_vec());
        tag.answer_to(&select_file(FILE_ID));
        assert_eq!(tag.answer_to(&[0x00, 0xD6, 0x00, 0x00, 0x01, 0xFF]), DENIED.to_vec());
        assert_eq!(tag.answer_to(&read_binary(5000, 2)), WRONG_OFFSET.to_vec());
    }

    #[test]
    fn short_records_prefixes_and_other_records_are_read_too() {
        // A short-form record with the https:// prefix, after a text record.
        let text = [0x91, 0x01, 0x04, b'T', 0x02, b'e', b'n', b'x'];
        let mut uri = vec![0x51, 0x01, 0x08, b'U', 0x04];
        uri.extend_from_slice(b"a.org");
        uri[2] = 6;
        let mut message = text.to_vec();
        message.extend_from_slice(&uri);
        assert_eq!(parse_uri(&message).unwrap(), "https://a.org");
        assert!(parse_uri(&text).is_err());
        assert!(parse_uri(&[]).is_err());
        assert!(parse_uri(&[0xD1, 0x01, 0xFF, b'U']).is_err());
    }
}
