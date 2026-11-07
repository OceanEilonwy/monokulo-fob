//! Trezor's wire protocol (version 1): a message is a type and its protocol
//! buffer bytes, cut into 64-byte reports. The first report starts `?##`,
//! then the type (2 bytes, big-endian) and the length (4 bytes); every
//! further report starts `?` and carries 63 more bytes.

/// One USB report.
pub const REPORT: usize = 64;

/// The reports that carry a message of `msg_type`.
pub fn frames(msg_type: u16, data: &[u8]) -> Vec<[u8; REPORT]> {
    let mut buffer = Vec::with_capacity(8 + data.len());
    buffer.extend_from_slice(b"##");
    buffer.extend_from_slice(&msg_type.to_be_bytes());
    buffer.extend_from_slice(&(data.len() as u32).to_be_bytes());
    buffer.extend_from_slice(data);
    buffer
        .chunks(REPORT - 1)
        .map(|chunk| {
            let mut report = [0u8; REPORT];
            report[0] = b'?';
            report[1..=chunk.len()].copy_from_slice(chunk);
            report
        })
        .collect()
}

/// Puts a message back together from its reports.
pub struct Reassembler {
    msg_type: u16,
    length: usize,
    data: Vec<u8>,
}

impl Reassembler {
    /// Starts from the first report, which names the type and length.
    pub fn first(report: &[u8]) -> Result<Reassembler, String> {
        if report.len() < 9 || &report[..3] != b"?##" {
            return Err("the device's reply didn't start as a message".into());
        }
        let msg_type = u16::from_be_bytes([report[3], report[4]]);
        let length = u32::from_be_bytes([report[5], report[6], report[7], report[8]]) as usize;
        if length > 4 * 1024 * 1024 {
            return Err("the device's reply is unreasonably long".into());
        }
        let mut data = Vec::with_capacity(length);
        data.extend_from_slice(&report[9..report.len().min(9 + length)]);
        Ok(Reassembler { msg_type, length, data })
    }

    pub fn done(&self) -> bool {
        self.data.len() >= self.length
    }

    /// Takes a further report.
    pub fn next(&mut self, report: &[u8]) -> Result<(), String> {
        if report.first() != Some(&b'?') {
            return Err("the device's reply broke off".into());
        }
        let wanted = self.length - self.data.len();
        let body = &report[1..];
        self.data.extend_from_slice(&body[..body.len().min(wanted)]);
        Ok(())
    }

    pub fn finish(self) -> (u16, Vec<u8>) {
        (self.msg_type, self.data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(msg_type: u16, data: &[u8]) {
        let reports = frames(msg_type, data);
        let mut r = Reassembler::first(&reports[0]).unwrap();
        for report in &reports[1..] {
            assert!(!r.done());
            r.next(report).unwrap();
        }
        assert!(r.done());
        assert_eq!(r.finish(), (msg_type, data.to_vec()));
    }

    #[test]
    fn a_short_message_takes_one_report_and_a_long_one_several() {
        assert_eq!(frames(0, &[]).len(), 1);
        assert_eq!(frames(17, &[1; 55]).len(), 1, "55 bytes fill the first report exactly");
        assert_eq!(frames(17, &[1; 56]).len(), 2);
        assert_eq!(frames(17, &[1; 55 + 63]).len(), 2);
        assert_eq!(frames(17, &[1; 55 + 64]).len(), 3);
        let first = frames(0x0203, b"ab")[0];
        assert_eq!(&first[..11], &[b'?', b'#', b'#', 2, 3, 0, 0, 0, 2, b'a', b'b']);
        assert!(first[11..].iter().all(|&b| b == 0));
    }

    #[test]
    fn messages_come_back_whole() {
        roundtrip(0, &[]);
        roundtrip(543, &[7; 55]);
        roundtrip(543, &[7; 56]);
        roundtrip(518, &(0..=255u8).cycle().take(1000).collect::<Vec<_>>());
    }

    #[test]
    fn refuses_reports_that_aren_t_a_message() {
        assert!(Reassembler::first(b"?#x").is_err());
        assert!(Reassembler::first(&[0; 64]).is_err());
        let mut r = Reassembler::first(&frames(1, &[1; 100])[0]).unwrap();
        assert!(r.next(&[0; 64]).is_err());
    }
}
