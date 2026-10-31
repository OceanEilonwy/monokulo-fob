//! Touch phones: two Fobs on Share my profile swap profiles over NFC in one
//! touch.
//!
//! Android dropped phone-to-phone NFC (Beam), so one phone acts as a card
//! (host card emulation) while the other reads it, and both switch roles at
//! random moments until one reads the other. The reader selects Fob's
//! application ([`AID`]), then sends its profile code in the commands and
//! gets the other's in the answers, a chunk each way per exchange, so both
//! end with the other's. Everything here is bytes in, bytes out: the app
//! only moves them between NFC and these.
//!
//! Each profile code travels as a frame: a version byte, its length (two
//! bytes, big-endian, at most [`CODE_MAX`]), the first four bytes of its
//! SHA3-256, then the code. Frames go in chunks of at most [`CHUNK`] bytes,
//! inside short APDUs (extended ones aren't on every phone):
//!
//! - `00 A4 04 00 08 <AID> 00`: select; the card answers its version.
//! - `80 E0 <index> [Lc <chunk>] 00`: the reader's chunk `index` (none once
//!   its frame is all sent); the card answers its own chunk `index` (none
//!   once its frame is). Indexes run from 0 with no gaps.
//! - `80 E2 00 00`: done; the reader has the card's code and has sent its
//!   own whole. The card checks it has the reader's and says so.
//!
//! A frame that's cut short, too long, of another version or that doesn't
//! match its checksum ends the touch with nothing taken; touching again
//! starts over.

use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

use sha3::{Digest, Sha3_256};

use crate::{
    error::{FobError, Result},
    network::Network,
    uri::{self, ProfileCode},
};

/// Fob's application on the card side: F0 (proprietary) and "FOBSWAP".
/// The app registers the same in `res/xml/apduservice.xml`.
pub const AID: [u8; 8] = [0xF0, b'F', b'O', b'B', b'S', b'W', b'A', b'P'];
/// The frame's first byte; another means another Fob.
pub const VERSION: u8 = 1;
/// The longest profile code a frame carries. The largest Fob makes (a
/// 95-character subaddress, 40 emoji, a 1536-byte photo) is 2664 bytes.
pub const CODE_MAX: usize = 3000;
/// The most bytes of a frame in one command or one answer.
pub const CHUNK: usize = 240;
/// Version, length and checksum.
const HEADER: usize = 1 + 2 + 4;

const CLA: u8 = 0x80;
const INS_SWAP: u8 = 0xE0;
const INS_DONE: u8 = 0xE2;

/// Status words, ISO 7816-4's.
const OK: [u8; 2] = [0x90, 0x00];
/// Not on Share my profile (or no such application): what anything that
/// selects Fob gets while the screen isn't in front.
pub const NOT_AVAILABLE: [u8; 2] = [0x6A, 0x82];
const WRONG_DATA: [u8; 2] = [0x6A, 0x80];
const WRONG_VERSION: [u8; 2] = [0x6A, 0x81];
const WRONG_INDEX: [u8; 2] = [0x6A, 0x86];
const WRONG_STATE: [u8; 2] = [0x69, 0x85];
const WRONG_LENGTH: [u8; 2] = [0x67, 0x00];
const WRONG_INS: [u8; 2] = [0x6D, 0x00];
const WRONG_CLA: [u8; 2] = [0x6E, 0x00];

/// How long an exchange may go quiet before the phone stops waiting for
/// it and goes back to switching roles.
const QUIET: Duration = Duration::from_secs(1);

/// Why a touch took nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Problem {
    /// The other phone answered, but not as a Fob on Share my profile.
    Away,
    /// Another version of the swap.
    Version,
    /// Cut short, too long, or not matching its checksum.
    Garbled,
    /// The other phone wouldn't take ours.
    Refused,
    /// Whole, but not a profile Fob takes (another network's, no name).
    NotProfile(String),
}

impl Problem {
    /// What the Touch phones screen says.
    pub fn text(&self) -> String {
        match self {
            Problem::Away => "The other phone isn't on Share my profile.".into(),
            Problem::Version => "The other phone has another version of Fob. Update both, or scan each other's code.".into(),
            Problem::Garbled => "That didn't come through whole. Hold the phones together again.".into(),
            Problem::Refused => "The other phone couldn't take your profile. Hold the phones together again.".into(),
            Problem::NotProfile(why) => why.clone(),
        }
    }
}

fn checksum(code: &[u8]) -> [u8; 4] {
    let hash = Sha3_256::digest(code);
    [hash[0], hash[1], hash[2], hash[3]]
}

/// A profile code as a frame: version, length, checksum, the code.
pub fn frame(code: &str) -> Result<Vec<u8>> {
    let bytes = code.as_bytes();
    if bytes.len() > CODE_MAX {
        return Err(FobError::Profile("Your profile is too big to swap by touching phones.".into()));
    }
    let mut out = Vec::with_capacity(HEADER + bytes.len());
    out.push(VERSION);
    out.extend_from_slice(&(bytes.len() as u16).to_be_bytes());
    out.extend_from_slice(&checksum(bytes));
    out.extend_from_slice(bytes);
    Ok(out)
}

/// The `index`th chunk of `frame`: empty past its end.
fn chunk(frame: &[u8], index: usize) -> &[u8] {
    let start = (index * CHUNK).min(frame.len());
    &frame[start..(start + CHUNK).min(frame.len())]
}

/// Whether the chunks up to `next` (not included) carry all of `frame`.
fn all_sent(frame: &[u8], next: usize) -> bool {
    next * CHUNK >= frame.len()
}

/// The other phone's frame, chunk by chunk.
#[derive(Clone, Debug, Default)]
struct Assembly {
    bytes: Vec<u8>,
}

impl Assembly {
    /// The length the frame says it has, once it's come.
    fn said(&self) -> Option<usize> {
        (self.bytes.len() >= 3).then(|| u16::from_be_bytes([self.bytes[1], self.bytes[2]]) as usize)
    }

    fn push(&mut self, chunk: &[u8]) -> std::result::Result<(), Problem> {
        if self.complete() || self.bytes.len() + chunk.len() > HEADER + CODE_MAX {
            return Err(Problem::Garbled);
        }
        self.bytes.extend_from_slice(chunk);
        if self.bytes[0] != VERSION {
            return Err(Problem::Version);
        }
        match self.said() {
            Some(len) if len > CODE_MAX || self.bytes.len() > HEADER + len => Err(Problem::Garbled),
            _ => Ok(()),
        }
    }

    fn complete(&self) -> bool {
        self.said().is_some_and(|len| self.bytes.len() == HEADER + len)
    }

    /// The code, once whole and matching its checksum.
    fn code(&self) -> std::result::Result<String, Problem> {
        if !self.complete() {
            return Err(Problem::Garbled);
        }
        let code = &self.bytes[HEADER..];
        if checksum(code) != self.bytes[3..HEADER] {
            return Err(Problem::Garbled);
        }
        String::from_utf8(code.to_vec()).map_err(|_| Problem::Garbled)
    }

    /// The profile it carries, read as a scanned one is.
    fn profile(&self, network: Network) -> std::result::Result<ProfileCode, Problem> {
        uri::parse_profile(network, &self.code()?).map_err(|e| Problem::NotProfile(e.to_string()))
    }
}

/// The select command for Fob's application.
pub fn select() -> Vec<u8> {
    let mut apdu = vec![0x00, 0xA4, 0x04, 0x00, AID.len() as u8];
    apdu.extend_from_slice(&AID);
    apdu.push(0x00);
    apdu
}

fn exchange(index: usize, data: &[u8]) -> Vec<u8> {
    let [hi, lo] = (index as u16).to_be_bytes();
    let mut apdu = vec![CLA, INS_SWAP, hi, lo];
    if !data.is_empty() {
        apdu.push(data.len() as u8);
        apdu.extend_from_slice(data);
    }
    apdu.push(0x00);
    apdu
}

fn done() -> Vec<u8> {
    vec![CLA, INS_DONE, 0x00, 0x00]
}

/// A short command APDU, read.
struct Command<'a> {
    cla: u8,
    ins: u8,
    p1: u8,
    p2: u8,
    data: &'a [u8],
}

fn command(apdu: &[u8]) -> Option<Command<'_>> {
    let (&[cla, ins, p1, p2], body) = apdu.split_first_chunk::<4>()?;
    let data = match body {
        // Header only, or with the length expected back.
        [] | [_] => &[][..],
        [lc, rest @ ..] => {
            let lc = *lc as usize;
            // Lc 0 opens an extended length, which Fob never sends.
            if lc == 0 || (rest.len() != lc && rest.len() != lc + 1) {
                return None;
            }
            &rest[..lc]
        }
    };
    Some(Command { cla, ins, p1, p2, data })
}

fn answer(data: &[u8], status: [u8; 2]) -> Vec<u8> {
    let mut out = data.to_vec();
    out.extend_from_slice(&status);
    out
}

/// The card side: answers a reader's commands with our frame and keeps
/// theirs.
#[derive(Debug)]
pub struct Card {
    network: Network,
    ours: Vec<u8>,
    theirs: Assembly,
    selected: bool,
    next: usize,
    /// Every chunk of ours went into an answer.
    gave: bool,
    got: Option<ProfileCode>,
}

impl Card {
    pub fn new(network: Network, frame: Vec<u8>) -> Card {
        Card { network, ours: frame, theirs: Assembly::default(), selected: false, next: 0, gave: false, got: None }
    }

    /// The link went (the phones moved apart): start over at the next select.
    pub fn reset(&mut self) {
        self.theirs = Assembly::default();
        self.selected = false;
        self.next = 0;
    }

    /// Midway through an exchange.
    pub fn busy(&self) -> bool {
        self.selected && self.got.is_none()
    }

    /// Their profile, once a reader has finished.
    pub fn got(&self) -> Option<&ProfileCode> {
        self.got.as_ref()
    }

    /// Whether our whole frame went into answers.
    pub fn gave(&self) -> bool {
        self.gave
    }

    /// Ends the exchange with `status`, ready for the next select.
    fn refuse(&mut self, status: [u8; 2]) -> Vec<u8> {
        self.reset();
        answer(&[], status)
    }

    /// The answer to one command.
    pub fn answer(&mut self, apdu: &[u8]) -> Vec<u8> {
        let Some(c) = command(apdu) else { return self.refuse(WRONG_LENGTH) };
        if (c.cla, c.ins, c.p1, c.p2) == (0x00, 0xA4, 0x04, 0x00) {
            if c.data != AID || self.got.is_some() {
                return self.refuse(NOT_AVAILABLE);
            }
            self.reset();
            self.selected = true;
            return answer(&[VERSION], OK);
        }
        if c.cla != CLA {
            return self.refuse(WRONG_CLA);
        }
        if !self.selected {
            return self.refuse(WRONG_STATE);
        }
        match c.ins {
            INS_SWAP => {
                if u16::from_be_bytes([c.p1, c.p2]) as usize != self.next {
                    return self.refuse(WRONG_INDEX);
                }
                // Theirs ended before it said it would.
                let pushed = if c.data.is_empty() { if self.theirs.complete() { Ok(()) } else { Err(Problem::Garbled) } } else { self.theirs.push(c.data) };
                // Checked once whole, so a garbled one is refused before done.
                if let Err(problem) = pushed.and_then(|_| if self.theirs.complete() { self.theirs.code().map(|_| ()) } else { Ok(()) }) {
                    return self.refuse(if problem == Problem::Version { WRONG_VERSION } else { WRONG_DATA });
                }
                let reply = chunk(&self.ours, self.next).to_vec();
                self.next += 1;
                self.gave |= all_sent(&self.ours, self.next);
                answer(&reply, OK)
            }
            INS_DONE => {
                if !self.theirs.complete() || !all_sent(&self.ours, self.next) {
                    return self.refuse(WRONG_STATE);
                }
                match self.theirs.profile(self.network) {
                    Ok(profile) => {
                        self.got = Some(profile);
                        self.selected = false;
                        answer(&[], OK)
                    }
                    Err(_) => self.refuse(WRONG_DATA),
                }
            }
            _ => self.refuse(WRONG_INS),
        }
    }
}

/// What the reader does next.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    /// Send this command and give back the answer.
    Send(Vec<u8>),
    /// Both have each other's profile: here's theirs.
    Done(ProfileCode),
    /// This touch took nothing.
    Failed(Problem),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Idle,
    Selecting,
    Swapping,
    Finishing,
}

/// The reader side: sends our frame in commands and keeps theirs from the
/// answers.
#[derive(Debug)]
pub struct Reader {
    network: Network,
    ours: Vec<u8>,
    theirs: Assembly,
    phase: Phase,
    next: usize,
    /// Every chunk of ours was taken by the card.
    gave: bool,
    /// Theirs, whole, while waiting for the card to say it has ours.
    pending: Option<ProfileCode>,
    got: Option<ProfileCode>,
}

impl Reader {
    pub fn new(network: Network, frame: Vec<u8>) -> Reader {
        Reader { network, ours: frame, theirs: Assembly::default(), phase: Phase::Idle, next: 0, gave: false, pending: None, got: None }
    }

    /// A card in the field: the first command to send.
    pub fn start(&mut self) -> Vec<u8> {
        self.lost();
        self.phase = Phase::Selecting;
        select()
    }

    /// The link went: nothing more until the next start.
    pub fn lost(&mut self) {
        self.theirs = Assembly::default();
        self.phase = Phase::Idle;
        self.next = 0;
        self.pending = None;
    }

    pub fn busy(&self) -> bool {
        self.phase != Phase::Idle
    }

    pub fn got(&self) -> Option<&ProfileCode> {
        self.got.as_ref()
    }

    pub fn gave(&self) -> bool {
        self.gave
    }

    fn fail(&mut self, problem: Problem) -> Step {
        self.lost();
        Step::Failed(problem)
    }

    fn send_next(&mut self) -> Step {
        Step::Send(exchange(self.next, chunk(&self.ours, self.next)))
    }

    /// What to do with the card's answer to the last command.
    pub fn next(&mut self, response: &[u8]) -> Step {
        let Some((data, status)) = response.split_last_chunk::<2>() else { return self.fail(Problem::Garbled) };
        let status = *status;
        match self.phase {
            Phase::Idle => self.fail(Problem::Garbled),
            Phase::Selecting => match (data, status) {
                ([VERSION], OK) => {
                    self.phase = Phase::Swapping;
                    self.send_next()
                }
                ([_], OK) => self.fail(Problem::Version),
                _ => self.fail(Problem::Away),
            },
            Phase::Swapping => {
                if status != OK {
                    return self.fail(if status == WRONG_VERSION { Problem::Version } else { Problem::Garbled });
                }
                let pushed = if data.is_empty() { if self.theirs.complete() { Ok(()) } else { Err(Problem::Garbled) } } else { self.theirs.push(data) };
                if let Err(problem) = pushed {
                    return self.fail(problem);
                }
                self.next += 1;
                self.gave |= all_sent(&self.ours, self.next);
                if !(all_sent(&self.ours, self.next) && self.theirs.complete()) {
                    return self.send_next();
                }
                match self.theirs.profile(self.network) {
                    Ok(profile) => {
                        self.pending = Some(profile);
                        self.phase = Phase::Finishing;
                        Step::Send(done())
                    }
                    Err(problem) => self.fail(problem),
                }
            }
            Phase::Finishing => {
                if status != OK {
                    return self.fail(Problem::Refused);
                }
                match self.pending.take() {
                    Some(profile) => {
                        self.phase = Phase::Idle;
                        self.got = Some(profile.clone());
                        Step::Done(profile)
                    }
                    None => self.fail(Problem::Garbled),
                }
            }
        }
    }
}

/// One Share my profile screen's swap: our profile on its own address,
/// both sides, and what came. Shared between the card service and the
/// reader, which call it from their own threads.
#[derive(Debug)]
pub struct Session {
    subaddress: u32,
    address: String,
    state: Mutex<State>,
}

#[derive(Debug)]
struct State {
    card: Card,
    reader: Reader,
    /// The last command or answer: an exchange quiet for longer has gone.
    last: Option<Instant>,
    ended: bool,
}

impl Session {
    /// A swap of `code`, our profile on `address` (subaddress `subaddress`).
    pub fn new(network: Network, code: &str, subaddress: u32, address: String) -> Result<Session> {
        let frame = frame(code)?;
        Ok(Session {
            subaddress,
            address,
            state: Mutex::new(State { card: Card::new(network, frame.clone()), reader: Reader::new(network, frame), last: None, ended: false }),
        })
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn subaddress(&self) -> u32 {
        self.subaddress
    }

    pub fn address(&self) -> &str {
        &self.address
    }

    /// The card's answer to a command; once the swap is over, or after it
    /// has theirs, a select is answered "not available".
    pub fn card_answer(&self, apdu: &[u8]) -> Vec<u8> {
        let mut state = self.state();
        if state.ended || state.reader.got.is_some() {
            return answer(&[], NOT_AVAILABLE);
        }
        state.last = Some(Instant::now());
        state.card.answer(apdu)
    }

    pub fn card_lost(&self) {
        self.state().card.reset();
    }

    /// A card in the field: the select to send, or none once over.
    pub fn reader_start(&self) -> Option<Vec<u8>> {
        let mut state = self.state();
        if state.ended || state.card.got.is_some() {
            return None;
        }
        state.last = Some(Instant::now());
        Some(state.reader.start())
    }

    pub fn reader_next(&self, response: &[u8]) -> Step {
        let mut state = self.state();
        if state.ended {
            state.reader.lost();
            return Step::Failed(Problem::Away);
        }
        state.last = Some(Instant::now());
        state.reader.next(response)
    }

    pub fn reader_lost(&self) {
        self.state().reader.lost();
    }

    /// Midway through an exchange that hasn't gone quiet: don't switch
    /// roles now.
    pub fn busy(&self) -> bool {
        let state = self.state();
        (state.card.busy() || state.reader.busy()) && state.last.is_some_and(|t| t.elapsed() < QUIET)
    }

    /// Their profile, from whichever side finished.
    pub fn got(&self) -> Option<ProfileCode> {
        let state = self.state();
        state.reader.got.clone().or_else(|| state.card.got.clone())
    }

    /// Ends it: nothing more is answered or read. Returns theirs, if it
    /// came, and whether ours went out whole.
    pub fn end(&self) -> (Option<ProfileCode>, bool) {
        let got = self.got();
        let mut state = self.state();
        state.ended = true;
        (got, state.card.gave || state.reader.gave)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{contacts, keys::Keys};

    fn webp(len: usize) -> Vec<u8> {
        let mut bytes = b"RIFF\0\0\0\0WEBPVP8 ".to_vec();
        bytes.resize(len, 9);
        bytes
    }

    fn largest(keys: &Keys) -> String {
        uri::profile_uri(&keys.address(1_000_000), &"😀".repeat(contacts::NAME_MAX), Some(&webp(contacts::PHOTO_MAX)))
    }

    /// One touch between `reader` and `card`, `tamper` seeing each command
    /// (true) or answer (false) with its number on the way, and returning
    /// false to drop the link there.
    fn touch(reader: &mut Reader, card: &mut Card, mut tamper: impl FnMut(usize, bool, &mut Vec<u8>) -> bool) -> Option<Step> {
        let mut apdu = reader.start();
        for n in 0.. {
            assert!(n < 64, "a touch ends");
            if !tamper(n, true, &mut apdu) {
                reader.lost();
                card.reset();
                return None;
            }
            assert!(apdu.len() <= 4 + 1 + 255 + 1, "a short APDU");
            let mut response = card.answer(&apdu);
            assert!(response.len() <= 256 + 2, "a short answer");
            if !tamper(n, false, &mut response) {
                reader.lost();
                card.reset();
                return None;
            }
            match reader.next(&response) {
                Step::Send(next) => apdu = next,
                step => return Some(step),
            }
        }
        unreachable!()
    }

    fn clean(_: usize, _: bool, _: &mut Vec<u8>) -> bool {
        true
    }

    fn peers(a: &str, b: &str) -> (Reader, Card) {
        (Reader::new(Network::Stagenet, frame(a).unwrap()), Card::new(Network::Stagenet, frame(b).unwrap()))
    }

    #[test]
    fn one_touch_gives_each_phone_the_other_s_profile() {
        let (henry, sam) = (Keys::generate(Network::Stagenet).keys, Keys::generate(Network::Stagenet).keys);
        let big = largest(&henry);
        assert_eq!(big.len(), 2664);
        let small = uri::profile_uri(&sam.address(3), "Sam", None);
        // Largest each way, and one much shorter than the other both ways.
        for (a, b) in [(big.clone(), big.clone()), (big.clone(), small.clone()), (small.clone(), big.clone()), (small.clone(), small.clone())] {
            let (mut reader, mut card) = peers(&a, &b);
            assert!(!reader.gave() && !card.gave());
            let Some(Step::Done(theirs)) = touch(&mut reader, &mut card, clean) else { panic!("swapped") };
            assert_eq!(theirs, uri::parse_profile(Network::Stagenet, &b).unwrap());
            assert_eq!(card.got(), Some(&uri::parse_profile(Network::Stagenet, &a).unwrap()));
            assert_eq!(reader.got(), Some(&theirs));
            assert!(reader.gave() && card.gave() && !reader.busy() && !card.busy());
        }
        // A second reader once the card has a profile: not available.
        let (mut reader, mut card) = peers(&small, &small);
        touch(&mut reader, &mut card, clean).unwrap();
        assert_eq!(card.answer(&select()), NOT_AVAILABLE.to_vec());
    }

    #[test]
    fn a_frame_is_a_version_a_length_a_checksum_and_the_code() {
        let f = frame("monero:abc").unwrap();
        assert_eq!(&f[..3], &[VERSION, 0, 10]);
        assert_eq!(&f[HEADER..], b"monero:abc");
        assert!(frame(&"x".repeat(CODE_MAX)).is_ok());
        assert!(frame(&"x".repeat(CODE_MAX + 1)).is_err(), "over the cap");
        let mut a = Assembly::default();
        a.push(&f[..5]).unwrap();
        assert!(!a.complete());
        a.push(&f[5..]).unwrap();
        assert_eq!(a.code().unwrap(), "monero:abc");
        assert_eq!(a.push(b"x"), Err(Problem::Garbled), "more after the end");
        // Says more than the cap, or carries more than it says.
        assert_eq!(Assembly::default().push(&[VERSION, 0x0B, 0xB9]), Err(Problem::Garbled));
        assert_eq!(Assembly::default().push(&[VERSION, 0, 1, 0, 0, 0, 0, b'a', b'b']), Err(Problem::Garbled));
        assert_eq!(Assembly::default().push(&[2, 0, 1]), Err(Problem::Version));
        let mut flipped = f.clone();
        flipped[HEADER + 2] ^= 1;
        let mut a = Assembly::default();
        a.push(&flipped).unwrap();
        assert_eq!(a.code(), Err(Problem::Garbled), "checksum");
    }

    #[test]
    fn a_touch_cut_short_garbled_oversize_or_of_another_version_takes_nothing_and_the_next_one_works() {
        let (henry, sam) = (Keys::generate(Network::Stagenet).keys, Keys::generate(Network::Stagenet).keys);
        let (a, b) = (largest(&henry), uri::profile_uri(&sam.address(3), "Sam", Some(&webp(900))));
        let failed = |step: Option<Step>| match step {
            Some(Step::Failed(p)) => p,
            other => panic!("expected a failure, got {other:?}"),
        };
        type Tamper = Box<dyn FnMut(usize, bool, &mut Vec<u8>) -> bool>;
        let cases: Vec<(&str, Tamper, Problem)> = vec![
            // An answer's chunk cut short: theirs never adds up.
            ("truncated answer", Box::new(|n, cmd, b: &mut Vec<u8>| { if n == 2 && !cmd { b.drain(b.len() - 12..b.len() - 2); } true }), Problem::Garbled),
            // A byte flipped in an answer: the reader's checksum fails.
            ("garbled answer", Box::new(|n, cmd, b: &mut Vec<u8>| { if n == 1 && !cmd { b[20] ^= 0x40; } true }), Problem::Garbled),
            // A byte flipped in a command: the card refuses it once whole.
            ("garbled command", Box::new(|n, cmd, b: &mut Vec<u8>| { if n == 3 && cmd { b[30] ^= 0x01; } true }), Problem::Garbled),
            // A frame saying it's longer than any profile.
            ("oversize", Box::new(|n, cmd, b: &mut Vec<u8>| { if n == 1 && !cmd { b[1] = 0xFF; } true }), Problem::Garbled),
            // Another version in the select's answer, and in a command's frame.
            ("version on select", Box::new(|n, cmd, b: &mut Vec<u8>| { if n == 0 && !cmd { b[0] = 2; } true }), Problem::Version),
            ("version in a frame", Box::new(|n, cmd, b: &mut Vec<u8>| { if n == 1 && cmd { b[5] = 2; } true }), Problem::Version),
            // A chunk out of order.
            ("skipped index", Box::new(|n, cmd, b: &mut Vec<u8>| { if n == 2 && cmd { b[3] += 1; } true }), Problem::Garbled),
            // Nothing Fob understands back, or no answer bytes at all.
            ("empty answer", Box::new(|n, cmd, b: &mut Vec<u8>| { if n == 1 && !cmd { b.clear(); } true }), Problem::Garbled),
            // The card refusing done.
            ("done refused", Box::new(|_, cmd, b: &mut Vec<u8>| { if cmd && b[1] == INS_DONE { b[2] = 1; b.truncate(4); b[1] = 0x99; } true }), Problem::Refused),
        ];
        for (name, mut tamper, problem) in cases {
            let (mut reader, mut card) = peers(&a, &b);
            assert_eq!(failed(touch(&mut reader, &mut card, &mut *tamper)), problem, "{name}");
            assert_eq!((reader.got(), card.got()), (None, None), "{name}: nothing taken");
            card.reset();
            assert!(matches!(touch(&mut reader, &mut card, clean), Some(Step::Done(_))), "{name}: touching again works");
            assert!(card.got().is_some(), "{name}");
        }
    }

    #[test]
    fn the_link_dropping_midway_takes_nothing_and_says_whether_ours_went_out() {
        let (henry, sam) = (Keys::generate(Network::Stagenet).keys, Keys::generate(Network::Stagenet).keys);
        let (a, b) = (largest(&henry), largest(&sam));
        let (mut reader, mut card) = peers(&a, &b);
        // Gone after the fourth command.
        assert_eq!(touch(&mut reader, &mut card, |n, _, _| n < 4), None);
        assert_eq!((reader.got(), card.got(), reader.gave(), card.gave(), reader.busy(), card.busy()), (None, None, false, false, false, false));
        // Gone just before done: both frames went whole, neither phone has a profile.
        let (mut reader, mut card) = peers(&a, &b);
        assert_eq!(touch(&mut reader, &mut card, |_, cmd, apdu| !(cmd && apdu[1] == INS_DONE)), None);
        assert_eq!((reader.got(), card.got(), reader.gave(), card.gave()), (None, None, true, true));
        // The answer to done lost: the card has it, the reader doesn't.
        let (mut reader, mut card) = peers(&a, &b);
        let mut finishing = false;
        assert_eq!(touch(&mut reader, &mut card, |_, cmd, apdu| { if cmd { finishing = apdu[1] == INS_DONE; true } else { !finishing } }), None);
        assert!(card.got().is_some() && reader.got().is_none());
    }

    #[test]
    fn a_profile_fob_wouldn_t_take_from_a_scan_isn_t_taken_by_touch() {
        let (henry, main) = (Keys::generate(Network::Stagenet).keys, Keys::generate(Network::Mainnet).keys);
        let ours = uri::profile_uri(&henry.address(1), "Henry", None);
        let fail = |a: &str, b: &str| {
            let (mut reader, mut card) = peers(a, b);
            let step = touch(&mut reader, &mut card, clean);
            (step, card.got().cloned())
        };
        let mainnet = uri::profile_uri(&main.address(1), "Main", None);
        assert_eq!(fail(&ours, &mainnet), (Some(Step::Failed(Problem::NotProfile(uri::parse_profile(Network::Stagenet, &mainnet).unwrap_err().to_string()))), None));
        // The card won't take a reader's code without a name: done refused.
        let bare = format!("monero:{}", henry.address(2));
        assert_eq!(fail(&bare, &ours), (Some(Step::Failed(Problem::Refused)), None));
        assert_eq!(Problem::NotProfile("x".into()).text(), "x");
        assert_eq!(Problem::Garbled.text(), "That didn't come through whole. Hold the phones together again.");
    }

    #[test]
    fn the_card_answers_only_fob_s_select_and_commands_in_order() {
        let henry = Keys::generate(Network::Stagenet).keys;
        let mut card = Card::new(Network::Stagenet, frame(&uri::profile_uri(&henry.address(1), "Henry", None)).unwrap());
        let status = |r: Vec<u8>| r[r.len() - 2..].to_vec();
        // Another application, a payment terminal's say.
        assert_eq!(card.answer(&[0x00, 0xA4, 0x04, 0x00, 0x07, 0xA0, 0, 0, 0, 0x04, 0x10, 0x10, 0x00]), NOT_AVAILABLE.to_vec());
        assert_eq!(status(card.answer(&exchange(0, b"x"))), WRONG_STATE, "nothing before select");
        assert_eq!(status(card.answer(&[0x80])), WRONG_LENGTH);
        assert_eq!(status(card.answer(&[0x80, 0xE0, 0, 0, 5, 1])), WRONG_LENGTH, "Lc says more than came");
        assert_eq!(status(card.answer(&[0x80, 0xE0, 0, 0, 0, 0, 1])), WRONG_LENGTH, "extended length");
        assert_eq!(card.answer(&select()), vec![VERSION, 0x90, 0x00]);
        assert!(card.busy());
        assert_eq!(status(card.answer(&[0x00, 0xE0, 0, 0, 0x00])), WRONG_CLA);
        assert_eq!(card.answer(&select()), vec![VERSION, 0x90, 0x00]);
        assert_eq!(status(card.answer(&[0x80, 0x10, 0, 0])), WRONG_INS);
        assert_eq!(card.answer(&select()), vec![VERSION, 0x90, 0x00]);
        assert_eq!(status(card.answer(&done())), WRONG_STATE, "done before anything came");
        assert!(!card.busy(), "refused, waiting for the next select");
        assert_eq!(command(&select()).unwrap().data, AID);
    }

    #[test]
    fn a_session_answers_until_it_ends_and_says_what_came() {
        let (henry, sam) = (Keys::generate(Network::Stagenet).keys, Keys::generate(Network::Stagenet).keys);
        let (a, b) = (uri::profile_uri(&henry.address(4), "Henry", None), uri::profile_uri(&sam.address(5), "Sam", Some(&webp(700))));
        let ours = Session::new(Network::Stagenet, &a, 4, henry.address(4)).unwrap();
        let theirs = Session::new(Network::Stagenet, &b, 5, sam.address(5)).unwrap();
        assert!(Session::new(Network::Stagenet, &"x".repeat(CODE_MAX + 1), 1, String::new()).is_err());
        assert!(!ours.busy());
        // Ours reads theirs.
        let mut apdu = ours.reader_start().unwrap();
        assert!(ours.busy());
        let step = loop {
            match ours.reader_next(&theirs.card_answer(&apdu)) {
                Step::Send(next) => apdu = next,
                step => break step,
            }
        };
        assert_eq!(step, Step::Done(uri::parse_profile(Network::Stagenet, &b).unwrap()));
        assert_eq!(ours.got().unwrap().name, "Sam");
        assert_eq!(theirs.got().unwrap().name, "Henry");
        assert!(!ours.busy() && !theirs.busy());
        // Done: neither answers nor reads again.
        assert_eq!(ours.card_answer(&select()), NOT_AVAILABLE.to_vec());
        assert_eq!(theirs.reader_start(), None);
        assert_eq!(ours.end(), (Some(uri::parse_profile(Network::Stagenet, &b).unwrap()), true));
        assert_eq!(ours.reader_start(), None);
        // A session nobody touched: nothing came, nothing went.
        let idle = Session::new(Network::Stagenet, &a, 4, henry.address(4)).unwrap();
        assert_eq!(idle.card_answer(&exchange(0, b"x"))[..], WRONG_STATE[..]);
        idle.card_lost();
        assert_eq!(idle.end(), (None, false));
        assert_eq!(idle.card_answer(&select()), NOT_AVAILABLE.to_vec(), "ended");
        assert_eq!((idle.subaddress(), idle.address()), (4, henry.address(4).as_str()));
    }

    #[test]
    fn the_app_registers_the_same_application_id() {
        let xml = include_str!("../../../android/app/src/main/res/xml/apduservice.xml");
        let aid: String = AID.iter().map(|b| format!("{b:02X}")).collect();
        assert!(xml.contains(&format!("android:name=\"{aid}\"")), "{aid} in apduservice.xml");
        assert!(xml.contains("android:requireDeviceUnlock=\"true\"") && xml.contains("android:category=\"other\""));
    }
}
