//! Just enough protocol buffers for Trezor's messages: varints, bytes and
//! nested messages, written and read by field number. No schema is compiled
//! in; each message in [`messages`](super::messages) names its own fields.

use std::fmt;

/// A message being written, field by field.
#[derive(Default, Clone)]
pub struct Writer(Vec<u8>);

fn put_varint(out: &mut Vec<u8>, mut v: u64) {
    loop {
        let byte = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

impl Writer {
    pub fn new() -> Writer {
        Writer(Vec::new())
    }

    fn tag(&mut self, field: u32, wire_type: u8) {
        put_varint(&mut self.0, (u64::from(field) << 3) | u64::from(wire_type));
    }

    /// A `uint32`, `uint64`, `bool` or enum field.
    pub fn uint(&mut self, field: u32, v: u64) -> &mut Self {
        self.tag(field, 0);
        put_varint(&mut self.0, v);
        self
    }

    pub fn bool(&mut self, field: u32, v: bool) -> &mut Self {
        self.uint(field, u64::from(v))
    }

    /// A `bytes` or `string` field.
    pub fn bytes(&mut self, field: u32, v: &[u8]) -> &mut Self {
        self.tag(field, 2);
        put_varint(&mut self.0, v.len() as u64);
        self.0.extend_from_slice(v);
        self
    }

    /// A nested message field.
    pub fn message(&mut self, field: u32, v: &Writer) -> &mut Self {
        self.bytes(field, &v.0)
    }

    pub fn finish(self) -> Vec<u8> {
        self.0
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

/// Why bytes couldn't be read as a message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Malformed;

impl fmt::Display for Malformed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the device's message couldn't be read")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Value {
    Varint(u64),
    Bytes(Vec<u8>),
    Fixed64([u8; 8]),
    Fixed32([u8; 4]),
}

/// A message read into its fields, looked up by number. A field given
/// several times (`repeated`) keeps every value in order; a lookup for one
/// value takes the last, as protocol buffers do.
#[derive(Clone, Debug, Default)]
pub struct Reader {
    fields: Vec<(u32, Value)>,
}

fn get_varint(data: &[u8], at: &mut usize) -> Result<u64, Malformed> {
    let mut v: u64 = 0;
    let mut shift = 0;
    loop {
        let byte = *data.get(*at).ok_or(Malformed)?;
        *at += 1;
        if shift >= 64 {
            return Err(Malformed);
        }
        v |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Ok(v);
        }
        shift += 7;
    }
}

impl Drop for Reader {
    /// Wiped as it goes: a reply may carry a key.
    fn drop(&mut self) {
        for (_, value) in &mut self.fields {
            match value {
                Value::Varint(n) => *n = 0,
                Value::Bytes(b) => zeroize::Zeroize::zeroize(b),
                Value::Fixed64(b) => *b = [0; 8],
                Value::Fixed32(b) => *b = [0; 4],
            }
        }
    }
}

impl Reader {
    pub fn parse(data: &[u8]) -> Result<Reader, Malformed> {
        let mut at = 0;
        let mut fields = Vec::new();
        while at < data.len() {
            let tag = get_varint(data, &mut at)?;
            let field = u32::try_from(tag >> 3).map_err(|_| Malformed)?;
            let value = match tag & 7 {
                0 => Value::Varint(get_varint(data, &mut at)?),
                1 => {
                    let bytes: [u8; 8] = data.get(at..at + 8).ok_or(Malformed)?.try_into().map_err(|_| Malformed)?;
                    at += 8;
                    Value::Fixed64(bytes)
                }
                2 => {
                    let len = usize::try_from(get_varint(data, &mut at)?).map_err(|_| Malformed)?;
                    let bytes = data.get(at..at.checked_add(len).ok_or(Malformed)?).ok_or(Malformed)?.to_vec();
                    at += len;
                    Value::Bytes(bytes)
                }
                5 => {
                    let bytes: [u8; 4] = data.get(at..at + 4).ok_or(Malformed)?.try_into().map_err(|_| Malformed)?;
                    at += 4;
                    Value::Fixed32(bytes)
                }
                _ => return Err(Malformed),
            };
            fields.push((field, value));
        }
        Ok(Reader { fields })
    }

    pub fn uint(&self, field: u32) -> Option<u64> {
        self.fields.iter().rev().find_map(|(f, v)| match v {
            Value::Varint(n) if *f == field => Some(*n),
            _ => None,
        })
    }

    pub fn bool(&self, field: u32) -> Option<bool> {
        self.uint(field).map(|n| n != 0)
    }

    pub fn all_uint(&self, field: u32) -> Vec<u64> {
        self.fields
            .iter()
            .filter_map(|(f, v)| match v {
                Value::Varint(n) if *f == field => Some(*n),
                _ => None,
            })
            .collect()
    }

    pub fn bytes(&self, field: u32) -> Option<&[u8]> {
        self.fields.iter().rev().find_map(|(f, v)| match v {
            Value::Bytes(b) if *f == field => Some(b.as_slice()),
            _ => None,
        })
    }

    pub fn all_bytes(&self, field: u32) -> Vec<&[u8]> {
        self.fields
            .iter()
            .filter_map(|(f, v)| match v {
                Value::Bytes(b) if *f == field => Some(b.as_slice()),
                _ => None,
            })
            .collect()
    }

    pub fn string(&self, field: u32) -> Option<String> {
        self.bytes(field).map(|b| String::from_utf8_lossy(b).into_owned())
    }

    pub fn message(&self, field: u32) -> Result<Option<Reader>, Malformed> {
        self.bytes(field).map(Reader::parse).transpose()
    }

    pub fn all_messages(&self, field: u32) -> Result<Vec<Reader>, Malformed> {
        self.all_bytes(field).into_iter().map(Reader::parse).collect()
    }
}

/// Monero's own varint, as its serialization and Trezor's hashes use it.
pub fn varint(v: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(10);
    put_varint(&mut out, v);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_and_reads_every_kind_of_field() {
        let mut inner = Writer::new();
        inner.uint(1, 7).bytes(2, b"xy");
        let mut w = Writer::new();
        w.uint(1, 300).bool(2, true).bytes(3, b"hello").message(4, &inner).uint(5, 1).uint(5, 2).bytes(6, b"a").bytes(6, b"b");
        let bytes = w.finish();
        // 300 = 0xac 0x02 after the tag 0x08.
        assert_eq!(&bytes[..3], &[0x08, 0xac, 0x02]);
        let r = Reader::parse(&bytes).unwrap();
        assert_eq!(r.uint(1), Some(300));
        assert_eq!(r.bool(2), Some(true));
        assert_eq!(r.bytes(3), Some(&b"hello"[..]));
        assert_eq!(r.string(3).as_deref(), Some("hello"));
        let nested = r.message(4).unwrap().unwrap();
        assert_eq!(nested.uint(1), Some(7));
        assert_eq!(nested.bytes(2), Some(&b"xy"[..]));
        assert_eq!(r.all_uint(5), vec![1, 2]);
        assert_eq!(r.all_bytes(6), vec![&b"a"[..], &b"b"[..]]);
        assert_eq!(r.uint(5), Some(2), "a repeated field looked up once gives the last");
        assert_eq!(r.uint(9), None);
        assert!(r.message(9).unwrap().is_none());
    }

    #[test]
    fn refuses_cut_off_messages() {
        assert!(Reader::parse(&[0x08]).is_err(), "tag without its varint");
        assert!(Reader::parse(&[0x12, 0x05, 0x61]).is_err(), "bytes shorter than their length");
        assert!(Reader::parse(&[0x0b]).is_err(), "group wire types aren't used");
        assert!(Reader::parse(&[]).unwrap().fields.is_empty());
    }

    #[test]
    fn varints_match_monero_s() {
        assert_eq!(varint(0), vec![0]);
        assert_eq!(varint(127), vec![127]);
        assert_eq!(varint(128), vec![0x80, 0x01]);
        assert_eq!(varint(u64::MAX).len(), 10);
    }
}
