//! Touch to pay: the phone on Receive answers as a card with the payment
//! code it shows, and the phone on Send (or Home) reads it, so two Fobs
//! pay each other without a camera.
//!
//! One side only gives and the other only takes, so none of
//! [`swap`](crate::swap)'s taking turns is needed: Receive is always the
//! card and Send always the reader. The code is the same text a QR code
//! carries, framed as a swap's is: a version byte, its length (two bytes,
//! big-endian, at most [`CODE_MAX`]), the first four bytes of its SHA3-256,
//! then the code, in chunks of at most [`CHUNK`] bytes inside short APDUs.
//!
//! - `00 A4 04 00 08 <AID> 00`: select; the card answers its version.
//! - `80 F0 <index> 00`: the card answers chunk `index` (none past its
//!   end). Indexes run from 0.
//!
//! Nothing is sent by this: the reader only gets what a camera would, and
//! the person on Send still confirms the payment.

use std::{sync::Mutex, time::Instant};


use crate::{
    nfc,
    error::CodeErrorResult as Result,
    ndef,
    network::Network,
    uri,
};

/// Fob's payment application on the card side: F0 and "FOBPAY!". The app
/// registers it beside the swap's in `res/xml/apduservice.xml`.
pub const AID: [u8; 8] = [0xF0, b'F', b'O', b'B', b'P', b'A', b'Y', b'!'];
pub const VERSION: u8 = 1;
/// The longest payment code a frame carries: an address, an amount and the
/// longest note take well under this.
pub const CODE_MAX: usize = 1500;
pub const CHUNK: usize = 240;
const HEADER: usize = 1 + 2 + 4;
const CLA: u8 = 0x80;
const INS_READ: u8 = 0xF0;
const OK: [u8; 2] = [0x90, 0x00];
/// Not on Receive: what a select gets while no code is offered.
pub const NOT_AVAILABLE: [u8; 2] = [0x6A, 0x82];
const WRONG_STATE: [u8; 2] = [0x69, 0x85];
const WRONG_LENGTH: [u8; 2] = [0x67, 0x00];
const WRONG_INS: [u8; 2] = [0x6D, 0x00];
const WRONG_CLA: [u8; 2] = [0x6E, 0x00];

/// Why a touch took nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Problem {
    /// The other phone isn't on Receive.
    Away,
    /// Another version.
    Version,
    /// Cut short, too long or not matching its checksum.
    Garbled,
    /// Whole, but not a payment code Fob takes (another network's).
    NotPayment(String),
}

impl Problem {
    pub fn text(&self) -> String {
        match self {
            Problem::Away => "The other phone isn't on Receive, and isn't showing a Monero tag.".into(),
            Problem::Version => "The other phone has another version of Fob. Update both, or scan its code.".into(),
            Problem::Garbled => "That didn't come through whole. Hold the phones together again.".into(),
            Problem::NotPayment(why) => why.clone(),
        }
    }
}

/// The name a select-by-name command asks for, if `apdu` is one.
fn selected_name(apdu: &[u8]) -> Option<&[u8]> {
    let (&[_, 0xA4, 0x04, _], body) = apdu.split_first_chunk::<4>()? else { return None };
    let lc = *body.first()? as usize;
    body.get(1..1 + lc)
}

/// Whether `apdu` selects one of the payment applications, Fob's own or
/// the standard NDEF tag (so the service knows a reader chose Receive's
/// code, not the swap).
pub fn is_select(apdu: &[u8]) -> bool {
    selected_name(apdu).is_some_and(|name| name == AID || name == ndef::AID)
}

fn read_command(index: usize) -> Vec<u8> {
    let [hi, lo] = (index as u16).to_be_bytes();
    vec![CLA, INS_READ, hi, lo, 0x00]
}

fn answer(data: &[u8], status: [u8; 2]) -> Vec<u8> {
    let mut out = data.to_vec();
    out.extend_from_slice(&status);
    out
}

#[derive(Debug, Default)]
struct CardState {
    /// The reader chose the standard NDEF tag, which answers from here on.
    ndef: bool,
    selected: bool,
    /// Every chunk went into an answer.
    read: bool,
    /// Something was asked recently, for [`PayCard::busy`].
    last: Option<Instant>,
}

/// The card side: the code on Receive, answered chunk by chunk.
#[derive(Debug)]
pub struct PayCard {
    frame: Vec<u8>,
    tag: Mutex<ndef::Tag>,
    state: Mutex<CardState>,
}

impl PayCard {
    pub fn new(code: &str) -> Result<PayCard> {
        Ok(PayCard { frame: frame(code)?, tag: Mutex::new(ndef::Tag::new(code.trim())), state: Mutex::new(CardState::default()) })
    }

    fn state(&self) -> std::sync::MutexGuard<'_, CardState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The link went: start over at the next select.
    pub fn lost(&self) {
        let mut state = self.state();
        state.selected = false;
        state.ndef = false;
        self.tag().lost();
    }

    fn tag(&self) -> std::sync::MutexGuard<'_, ndef::Tag> {
        self.tag.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// A reader took the whole code (it may not be a Fob, or may not pay).
    pub fn read(&self) -> bool {
        self.state().read || self.tag().read()
    }

    /// The answer to one command.
    pub fn answer(&self, apdu: &[u8]) -> Vec<u8> {
        let mut state = self.state();
        state.last = Some(Instant::now());
        let Some((&[cla, ins, p1, p2], _)) = apdu.split_first_chunk::<4>() else { return answer(&[], WRONG_LENGTH) };
        // A select by name picks the application; the standard tag takes over until the next one.
        if let Some(name) = selected_name(apdu) {
            state.ndef = name == ndef::AID;
            if state.ndef {
                state.selected = false;
                return self.tag().answer_to(apdu);
            }
            if name != AID {
                return answer(&[], NOT_AVAILABLE);
            }
            state.selected = true;
            return answer(&[VERSION], OK);
        }
        if state.ndef {
            return self.tag().answer_to(apdu);
        }

        if cla != CLA {
            return answer(&[], WRONG_CLA);
        }
        if !state.selected {
            return answer(&[], WRONG_STATE);
        }
        if ins != INS_READ {
            return answer(&[], WRONG_INS);
        }
        let index = u16::from_be_bytes([p1, p2]) as usize;
        let start = (index * CHUNK).min(self.frame.len());
        let end = (start + CHUNK).min(self.frame.len());
        if end == self.frame.len() {
            state.read = true;
        }
        answer(&self.frame[start..end], OK)
    }
}

/// What the reader does next.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    /// Send this command and give back the answer.
    Send(Vec<u8>),
    /// The whole code: here it is.
    Done(String),
    Failed(Problem),
}

/// Where a read of the standard tag has got to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TagPhase {
    SelectApp,
    SelectCapability,
    ReadCapability,
    SelectFile,
    ReadLength,
    ReadMessage,
}

#[derive(Debug, Default)]
struct ReaderState {
    /// Reading the standard tag, after the card didn't know Fob's application.
    tag: Option<TagPhase>,
    file_id: u16,
    max_read: usize,
    message_len: usize,
    selecting: bool,
    reading: bool,
    next: usize,
    bytes: Vec<u8>,
}

/// The reader side: asks for chunks until the frame is whole.
#[derive(Debug)]
pub struct PayReader {
    network: Network,
    state: Mutex<ReaderState>,
}

impl PayReader {
    pub fn new(network: Network) -> PayReader {
        PayReader { network, state: Mutex::new(ReaderState::default()) }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, ReaderState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// A card in the field: the select to send.
    pub fn start(&self) -> Vec<u8> {
        *self.state() = ReaderState { selecting: true, ..Default::default() };
        select()
    }

    pub fn lost(&self) {
        *self.state() = ReaderState::default();
    }

    fn fail(state: &mut ReaderState, problem: Problem) -> Step {
        *state = ReaderState::default();
        Step::Failed(problem)
    }

    /// The next command in reading a standard NFC tag, the way any phone's own reader does it.
    fn next_tag(&self, state: &mut ReaderState, phase: TagPhase, data: &[u8], status: [u8; 2]) -> Step {
        if status != OK {
            return Self::fail(state, if phase == TagPhase::SelectApp { Problem::Away } else { Problem::Garbled });
        }
        match phase {
            TagPhase::SelectApp => {
                state.tag = Some(TagPhase::SelectCapability);
                Step::Send(ndef::select_file(ndef::CC_ID))
            }
            TagPhase::SelectCapability => {
                state.tag = Some(TagPhase::ReadCapability);
                Step::Send(ndef::read_binary(0, 15))
            }
            TagPhase::ReadCapability => {
                // 15 bytes: the NDEF file control TLV (type 4, length 6) starts at 7.
                if data.len() < 15 || data[7] != 0x04 || data[8] != 0x06 {
                    return Self::fail(state, Problem::Garbled);
                }
                state.file_id = u16::from_be_bytes([data[9], data[10]]);
                state.max_read = (u16::from_be_bytes([data[3], data[4]]) as usize).clamp(1, 250);
                state.tag = Some(TagPhase::SelectFile);
                Step::Send(ndef::select_file(state.file_id))
            }
            TagPhase::SelectFile => {
                state.tag = Some(TagPhase::ReadLength);
                Step::Send(ndef::read_binary(0, 2))
            }
            TagPhase::ReadLength => {
                if data.len() < 2 {
                    return Self::fail(state, Problem::Garbled);
                }
                let len = u16::from_be_bytes([data[0], data[1]]) as usize;
                if len == 0 || len > CODE_MAX + 64 {
                    return Self::fail(state, Problem::Garbled);
                }
                state.message_len = len;
                state.bytes.clear();
                state.tag = Some(TagPhase::ReadMessage);
                Step::Send(ndef::read_binary(2, len.min(state.max_read)))
            }
            TagPhase::ReadMessage => {
                if data.is_empty() {
                    return Self::fail(state, Problem::Garbled);
                }
                state.bytes.extend_from_slice(data);
                if state.bytes.len() < state.message_len {
                    let left = state.message_len - state.bytes.len();
                    return Step::Send(ndef::read_binary(2 + state.bytes.len(), left.min(state.max_read)));
                }
                if state.bytes.len() > state.message_len {
                    return Self::fail(state, Problem::Garbled);
                }
                let read = ndef::parse_uri(&state.bytes).map_err(Problem::NotPayment).and_then(|text| {
                    uri::parse_payment(self.network, &text).map(|_| text).map_err(|e| Problem::NotPayment(e.to_string()))
                });
                *state = ReaderState::default();
                match read {
                    Ok(text) => Step::Done(text),
                    Err(problem) => Step::Failed(problem),
                }
            }
        }
    }

    /// What to do with the card's answer to the last command.
    pub fn next(&self, response: &[u8]) -> Step {
        let mut state = self.state();
        let Some((data, status)) = response.split_last_chunk::<2>() else { return Self::fail(&mut state, Problem::Garbled) };
        if let Some(phase) = state.tag {
            return self.next_tag(&mut state, phase, data, *status);
        }
        if state.selecting {
            return match (data, *status) {
                ([VERSION], OK) => {
                    state.selecting = false;
                    state.reading = true;
                    Step::Send(read_command(0))
                }
                ([_], OK) => Self::fail(&mut state, Problem::Version),
                // Not Fob's application: another wallet's, a sticker's or a till's standard tag, perhaps.
                _ => {
                    state.selecting = false;
                    state.tag = Some(TagPhase::SelectApp);
                    Step::Send(ndef::select_app())
                }
            };
        }
        if !state.reading || *status != OK {
            return Self::fail(&mut state, Problem::Garbled);
        }
        if state.bytes.len() + data.len() > HEADER + CODE_MAX || (data.is_empty() && state.bytes.len() < HEADER) {
            return Self::fail(&mut state, Problem::Garbled);
        }
        state.bytes.extend_from_slice(data);
        if state.bytes.len() >= HEADER && state.bytes[0] != VERSION {
            return Self::fail(&mut state, Problem::Version);
        }
        if state.bytes.len() < HEADER {
            state.next += 1;
            return Step::Send(read_command(state.next));
        }
        let said = u16::from_be_bytes([state.bytes[1], state.bytes[2]]) as usize;
        if said > CODE_MAX || state.bytes.len() > HEADER + said {
            return Self::fail(&mut state, Problem::Garbled);
        }
        if state.bytes.len() < HEADER + said {
            if data.is_empty() {
                return Self::fail(&mut state, Problem::Garbled);
            }
            state.next += 1;
            return Step::Send(read_command(state.next));
        }
        let code = &state.bytes[HEADER..];
        if nfc::checksum(code) != state.bytes[3..HEADER] {
            return Self::fail(&mut state, Problem::Garbled);
        }
        let Ok(text) = String::from_utf8(code.to_vec()) else { return Self::fail(&mut state, Problem::Garbled) };
        let parsed = uri::parse_payment(self.network, &text);
        *state = ReaderState::default();
        match parsed {
            Ok(_) => Step::Done(text),
            Err(e) => Step::Failed(Problem::NotPayment(e.to_string())),
        }
    }
}

/// A payment code as a frame.
pub fn frame(code: &str) -> Result<Vec<u8>> {
    nfc::frame(code, VERSION, CODE_MAX, "That payment code is too big to send by touching phones.")
}

/// The select command.
pub fn select() -> Vec<u8> {
    nfc::select(&AID)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::Keys;

    fn code(note: &str) -> String {
        let keys = Keys::generate(Network::Stagenet).keys;
        uri::payment_uri(&keys.address(0), Some(120_000_000_000), Some(note), Some("Ana"))
    }

    /// A whole touch, in memory.
    fn touch(card: &PayCard, reader: &PayReader) -> Step {
        let mut apdu = reader.start();
        loop {
            match reader.next(&card.answer(&apdu)) {
                Step::Send(next) => apdu = next,
                done => return done,
            }
        }
    }

    #[test]
    fn a_reader_gets_the_code_receive_shows() {
        for note in ["", "Coffee", &"x".repeat(300)] {
            let shown = code(note);
            let card = PayCard::new(&shown).unwrap();
            assert!(!card.read());
            assert_eq!(touch(&card, &PayReader::new(Network::Stagenet)), Step::Done(shown));
            assert!(card.read());
        }
    }

    #[test]
    fn a_touch_can_be_made_again() {
        let shown = code("Coffee");
        let card = PayCard::new(&shown).unwrap();
        let reader = PayReader::new(Network::Stagenet);
        assert_eq!(touch(&card, &reader), Step::Done(shown.clone()));
        card.lost();
        assert_eq!(touch(&card, &reader), Step::Done(shown));
    }

    #[test]
    fn another_network_s_code_is_not_taken() {
        let card = PayCard::new(&code("Coffee")).unwrap();
        assert!(matches!(touch(&card, &PayReader::new(Network::Mainnet)), Step::Failed(Problem::NotPayment(_))));
    }

    #[test]
    fn a_garbled_or_cut_short_frame_takes_nothing() {
        let card = PayCard::new(&code("Coffee")).unwrap();
        let reader = PayReader::new(Network::Stagenet);
        let mut apdu = reader.start();
        let mut first = true;
        loop {
            let mut response = card.answer(&apdu);
            if !first && response.len() > 3 {
                response[2] ^= 0xFF;
            }
            first = false;
            match reader.next(&response) {
                Step::Send(next) => apdu = next,
                step => {
                    assert_eq!(step, Step::Failed(Problem::Garbled));
                    break;
                }
            }
        }
    }

    #[test]
    fn a_card_that_isn_t_receive_is_away() {
        let reader = PayReader::new(Network::Stagenet);
        reader.start();
        // Not Fob's application: the standard tag is tried, and a card with neither is away.
        assert_eq!(reader.next(&NOT_AVAILABLE), Step::Send(ndef::select_app()));
        assert_eq!(reader.next(&NOT_AVAILABLE), Step::Failed(Problem::Away));
    }

    #[test]
    fn the_card_answers_only_after_select_and_only_its_own_application() {
        let card = PayCard::new(&code("Coffee")).unwrap();
        assert_eq!(card.answer(&read_command(0)), WRONG_STATE.to_vec());
        let mut other = select();
        other[5] = 0xF1;
        assert_eq!(card.answer(&other), NOT_AVAILABLE.to_vec());
        assert!(is_select(&select()));
        assert!(!is_select(&other));
    }

    #[test]
    fn the_apdu_service_file_lists_the_standard_tag_too() {
        let xml = include_str!("../../../android/app/src/main/res/xml/apduservice.xml");
        assert!(xml.contains("android:name=\"D2760000850101\""));
    }

    #[test]
    fn the_apdu_service_file_lists_this_application() {
        let xml = include_str!("../../../android/app/src/main/res/xml/apduservice.xml");
        assert!(xml.contains(&format!("android:name=\"{}\"", AID.iter().map(|b| format!("{b:02X}")).collect::<String>())));
    }

    /// The card with Fob's own application hidden, as a phone that only offers the standard tag.
    fn standard_tag_only(card: &PayCard, apdu: &[u8]) -> Vec<u8> {
        if apdu == select().as_slice() { answer(&[], NOT_AVAILABLE) } else { card.answer(apdu) }
    }

    #[test]
    fn a_fob_reads_a_standard_tag_too() {
        for note in ["", "Coffee", &"x".repeat(300)] {
            let shown = code(note);
            let card = PayCard::new(&shown).unwrap();
            let reader = PayReader::new(Network::Stagenet);
            let mut apdu = reader.start();
            let step = loop {
                match reader.next(&standard_tag_only(&card, &apdu)) {
                    Step::Send(next) => apdu = next,
                    done => break done,
                }
            };
            assert_eq!(step, Step::Done(shown));
            assert!(card.read());
        }
    }

    #[test]
    fn a_tag_with_another_networks_code_or_no_code_is_not_taken() {
        let card = PayCard::new(&code("Coffee")).unwrap();
        let reader = PayReader::new(Network::Mainnet);
        let mut apdu = reader.start();
        let step = loop {
            match reader.next(&standard_tag_only(&card, &apdu)) {
                Step::Send(next) => apdu = next,
                done => break done,
            }
        };
        assert!(matches!(step, Step::Failed(Problem::NotPayment(_))), "{step:?}");
        let blank = PayCard::new("monero:").unwrap();
        let reader = PayReader::new(Network::Stagenet);
        let mut apdu = reader.start();
        let step = loop {
            match reader.next(&standard_tag_only(&blank, &apdu)) {
                Step::Send(next) => apdu = next,
                done => break done,
            }
        };
        assert!(matches!(step, Step::Failed(Problem::NotPayment(_))), "{step:?}");
    }

    #[test]
    fn the_card_answers_the_standard_tag_when_asked_and_fob_s_own_after_it() {
        let card = PayCard::new(&code("Coffee")).unwrap();
        assert!(is_select(&ndef::select_app()) && is_select(&select()));
        assert_eq!(card.answer(&ndef::select_app()), vec![0x90, 0x00]);
        card.lost();
        assert_eq!(card.answer(&select()), vec![VERSION, 0x90, 0x00]);
        assert_eq!(card.answer(&read_command(0))[..1], [VERSION]);
    }

    #[test]
    fn too_big_a_code_is_refused() {
        assert!(frame(&"x".repeat(CODE_MAX + 1)).is_err());
    }
}
