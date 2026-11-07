//! A conversation with a Trezor: messages over a [`Link`] the app provides
//! (USB on Android), with the device's own questions (press a button, type
//! the PIN, the passphrase) answered through [`Prompts`].

use zeroize::Zeroizing;

use super::{crypto, messages as m, wire};
use crate::error::{FobError, Result};
use crate::network::Network;

/// The bytes between Fob and the device, one 64-byte report at a time.
/// Reads block until the device answers: it may be waiting for a finger.
pub trait Link: Send + Sync {
    fn write(&self, report: &[u8]) -> std::result::Result<(), String>;
    fn read(&self) -> std::result::Result<Vec<u8>, String>;
}

/// What the person says to a passphrase prompt.
pub enum Passphrase {
    /// Typed on the device itself.
    OnDevice,
    /// Typed on the phone; empty for the default wallet.
    Typed(Zeroizing<String>),
    Cancelled,
}

/// The device's questions, put to the person by the app. Called on the
/// thread talking to the device, which waits for the answer.
pub trait Prompts: Send + Sync {
    /// The device shows something to confirm; `what` names the screen.
    fn button(&self, what: &str);
    /// The device wants its PIN, typed against the scrambled keypad it
    /// shows: the digits are the keypad positions (1 to 9). `None` cancels.
    fn pin(&self) -> Option<Zeroizing<String>>;
    /// `on_device` says the device can take the passphrase itself.
    fn passphrase(&self, on_device: bool) -> Passphrase;
    /// A step of a long exchange is done: `done` of `total`.
    fn progress(&self, done: u32, total: u32);
}

/// An opened conversation, after `Initialize`.
pub struct Session<'a> {
    link: &'a dyn Link,
    prompts: &'a dyn Prompts,
    pub features: m::Features,
}

fn device(text: impl Into<String>) -> FobError {
    FobError::Trezor(text.into())
}

/// Firmware older than this doesn't know client version 4 of the Monero
/// protocol.
const MIN_VERSION: (u64, u64, u64) = (2, 5, 2);

impl<'a> Session<'a> {
    /// Says hello and reads what the device is. Refuses one that can't do
    /// Monero, isn't set up, or has firmware too old to talk to.
    pub fn open(link: &'a dyn Link, prompts: &'a dyn Prompts) -> Result<Session<'a>> {
        let mut session = Session { link, prompts, features: m::Features::default() };
        let data = session.expect(m::INITIALIZE, &m::initialize(None), m::FEATURES)?;
        let features = m::Features::parse(&data).map_err(|e| device(e.to_string()))?;
        if !features.can(m::CAPABILITY_MONERO) {
            return Err(device(format!("{} can't hold Monero: Trezor Model One doesn't, and a Trezor set up for Bitcoin only needs its full firmware", features.name())));
        }
        if !features.initialized {
            return Err(device(format!("{} isn't set up yet: give it a seed first", features.name())));
        }
        if features.version < MIN_VERSION {
            return Err(device(format!("{} needs firmware {}.{}.{} or newer", features.name(), MIN_VERSION.0, MIN_VERSION.1, MIN_VERSION.2)));
        }
        session.features = features;
        Ok(session)
    }

    fn write(&self, msg_type: u16, data: &[u8]) -> Result<()> {
        // A PIN or passphrase goes this way: the reports are wiped after.
        let reports = Zeroizing::new(wire::frames(msg_type, data));
        for report in reports.iter() {
            self.link.write(report).map_err(|e| device(format!("couldn't reach the device: {e}")))?;
        }
        Ok(())
    }

    fn read(&self) -> Result<(u16, Vec<u8>)> {
        let read = || self.link.read().map_err(|e| device(format!("the device stopped answering: {e}")));
        let mut message = wire::Reassembler::first(&read()?).map_err(device)?;
        while !message.done() {
            message.next(&read()?).map_err(device)?;
        }
        Ok(message.finish())
    }

    /// Sends a message and brings back the device's answer, seeing to its
    /// questions on the way.
    pub fn call(&self, msg_type: u16, data: &[u8]) -> Result<(u16, Vec<u8>)> {
        self.write(msg_type, data)?;
        loop {
            let (reply, data) = self.read()?;
            match reply {
                m::BUTTON_REQUEST => {
                    self.prompts.button(&m::button_request(&data));
                    self.write(m::BUTTON_ACK, &[])?;
                }
                m::PIN_MATRIX_REQUEST => match self.prompts.pin() {
                    Some(pin) => self.write(m::PIN_MATRIX_ACK, &m::pin_matrix_ack(&pin))?,
                    None => self.write(m::CANCEL, &[])?,
                },
                m::PASSPHRASE_REQUEST => {
                    let on_device = self.features.can(m::CAPABILITY_PASSPHRASE_ENTRY);
                    match self.prompts.passphrase(on_device) {
                        Passphrase::OnDevice => self.write(m::PASSPHRASE_ACK, &m::passphrase_ack(None, true))?,
                        Passphrase::Typed(text) => self.write(m::PASSPHRASE_ACK, &m::passphrase_ack(Some(&text), false))?,
                        Passphrase::Cancelled => self.write(m::CANCEL, &[])?,
                    }
                }
                m::FAILURE => {
                    // Fob's own words for each code: the device's text isn't
                    // shown, as a hostile one could say anything there.
                    let (code, _) = m::failure(&data);
                    return Err(match code {
                        m::FAILURE_ACTION_CANCELLED | m::FAILURE_PIN_CANCELLED => FobError::TrezorCancelled,
                        m::FAILURE_PIN_INVALID => device("the PIN was wrong"),
                        m::FAILURE_UNEXPECTED_MESSAGE => device("the Trezor wasn't expecting that: unplug it and try again"),
                        m::FAILURE_DATA_ERROR | m::FAILURE_PROCESS_ERROR => device("the Trezor refused what Fob sent (its firmware may be older than Fob knows)"),
                        m::FAILURE_NOT_ENOUGH_FUNDS => device("the Trezor says the amounts don't add up"),
                        m::FAILURE_NOT_INITIALIZED => device("the Trezor isn't set up yet"),
                        m::FAILURE_BUSY => device("the Trezor is busy with another app"),
                        _ => device(format!("the Trezor refused (code {code})")),
                    });
                }
                _ => return Ok((reply, data)),
            }
        }
    }

    pub(super) fn prompts(&self) -> &dyn Prompts {
        self.prompts
    }

    /// `call`, expecting an answer of `wanted`.
    pub fn expect(&self, msg_type: u16, data: &[u8], wanted: u16) -> Result<Vec<u8>> {
        let (reply, data) = self.call(msg_type, data)?;
        if reply != wanted {
            return Err(device(format!("the device answered with message {reply} where {wanted} was expected")));
        }
        Ok(data)
    }

    /// Asks the device for the wallet's private view key and primary
    /// address; the device asks the person first.
    pub fn watch_key(&self, network: Network) -> Result<(Zeroizing<[u8; 32]>, String)> {
        let data = Zeroizing::new(self.expect(m::MONERO_GET_WATCH_KEY, &m::get_watch_key(network), m::MONERO_WATCH_KEY)?);
        let (key, address) = m::watch_key(&data).map_err(|e| device(e.to_string()))?;
        Ok((Zeroizing::new(key), address))
    }

    /// Asks the device for the wallet's primary address and to show it on
    /// its own screen, so the person can compare: what came over the cable
    /// can't be trusted on its own.
    pub fn address_shown(&self, network: Network) -> Result<String> {
        let data = self.expect(m::MONERO_GET_ADDRESS, &m::get_address(network, true, 0), m::MONERO_ADDRESS)?;
        m::address(&data).map_err(|e| device(e.to_string()))
    }

    /// Asks the device for `transfers`' key images (its spend key makes
    /// them), each checked against the proof it comes with. `minors` are
    /// the subaddress indices (of account 0) the outputs may be on.
    pub fn key_images(&self, network: Network, transfers: &[m::Transfer], minors: &[u32]) -> Result<Vec<[u8; 32]>> {
        if transfers.is_empty() {
            return Ok(vec![]);
        }
        let mut all = Vec::with_capacity(32 * transfers.len());
        for t in transfers {
            all.extend_from_slice(&crypto::transfer_hash(&t.out_key, &t.tx_pub_key, t.additional.as_ref(), t.index_in_tx));
        }
        let hash = crypto::keccak(&all);
        self.expect(m::MONERO_KEY_IMAGE_EXPORT_INIT_REQUEST, &m::key_image_export_init(transfers.len(), &hash, network, minors), m::MONERO_KEY_IMAGE_EXPORT_INIT_ACK)?;
        let mut sealed = Vec::with_capacity(transfers.len());
        const BATCH: usize = 10;
        for (i, batch) in transfers.chunks(BATCH).enumerate() {
            let data = self.expect(m::MONERO_KEY_IMAGE_SYNC_STEP_REQUEST, &m::key_image_sync_step(batch), m::MONERO_KEY_IMAGE_SYNC_STEP_ACK)?;
            let images = m::key_image_sync_step_ack(&data).map_err(|e| device(e.to_string()))?;
            if images.len() != batch.len() {
                return Err(device("the device gave back the wrong number of key images"));
            }
            sealed.extend(images);
            self.prompts.progress(((i * BATCH) + batch.len()) as u32, transfers.len() as u32);
        }
        let data = self.expect(m::MONERO_KEY_IMAGE_SYNC_FINAL_REQUEST, &[], m::MONERO_KEY_IMAGE_SYNC_FINAL_ACK)?;
        let key = m::key_image_sync_final_ack(&data).map_err(|e| device(e.to_string()))?;
        let mut images = Vec::with_capacity(transfers.len());
        for ((nonce, blob), transfer) in sealed.iter().zip(transfers) {
            let plain = crypto::open(&key, nonce, blob).map_err(device)?;
            let [image, c, r]: [[u8; 32]; 3] = match plain.len() {
                96 => [plain[..32].try_into().expect("32"), plain[32..64].try_into().expect("32"), plain[64..].try_into().expect("32")],
                _ => return Err(device("a key image from the device has the wrong size")),
            };
            if !crypto::key_image_checks_out(&image, &c, &r, &transfer.out_key) {
                return Err(device("a key image from the device doesn't match its output"));
            }
            images.push(image);
        }
        Ok(images)
    }
}

/// A Trezor in software, for tests: answers as the device would, with a
/// script of the questions it asks on the way.
#[cfg(test)]
pub mod fake {
    use std::sync::Mutex;

    use super::*;
    use crate::trezor::proto::{Reader, Writer};

    /// What a fake wants of the person before it answers a request.
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub enum Asks {
        Button,
        Pin,
        Passphrase,
    }

    /// Makes a key image, and its signature's c and r, for an output.
    pub type KeyImageMaker = dyn Fn(&m::Transfer) -> ([u8; 32], [u8; 32], [u8; 32]) + Send + Sync;

    pub struct Fake {
        /// The next reports to hand out.
        pending: Mutex<Vec<[u8; 64]>>,
        /// The request being received.
        incoming: Mutex<Option<wire::Reassembler>>,
        /// The questions asked before each real answer, then forgotten.
        pub asks: Mutex<Vec<Asks>>,
        /// What the fake is in the middle of: a question whose answer is awaited.
        asking: Mutex<Option<(Asks, u16, Vec<u8>)>>,
        pub pin: String,
        pub features: m::Features,
        pub view_key: [u8; 32],
        pub address: String,
        /// What the device shows (and says) when asked for the address; the
        /// exported one unless a test puts something else here.
        pub shown_address: Option<String>,
        /// Makes a key image, its signature's c and r, for an output's key.
        pub key_image: Box<KeyImageMaker>,
        pub enc_key: [u8; 32],
        /// Every request received, by type.
        pub log: Mutex<Vec<u16>>,
        /// Answers this type with a Failure of this code instead.
        pub fail: Mutex<Option<(u16, u64)>>,
    }

    impl Fake {
        pub fn new() -> Fake {
            Fake {
                pending: Mutex::new(vec![]),
                incoming: Mutex::new(None),
                asks: Mutex::new(vec![]),
                asking: Mutex::new(None),
                pin: "1234".into(),
                features: m::Features { vendor: "trezor.io".into(), version: (2, 8, 0), initialized: true, capabilities: vec![1, m::CAPABILITY_MONERO], model: Some("Safe 3".into()), ..m::Features::default() },
                view_key: [0; 32],
                address: String::new(),
                shown_address: None,
                key_image: Box::new(|_| ([0; 32], [0; 32], [0; 32])),
                enc_key: [5; 32],
                log: Mutex::new(vec![]),
                fail: Mutex::new(None),
            }
        }

        fn features_bytes(&self) -> Vec<u8> {
            let f = &self.features;
            let mut w = Writer::new();
            w.bytes(1, f.vendor.as_bytes()).uint(2, f.version.0).uint(3, f.version.1).uint(4, f.version.2).bool(12, f.initialized);
            if let Some(model) = &f.model {
                w.bytes(21, model.as_bytes());
            }
            for c in &f.capabilities {
                w.uint(30, *c);
            }
            w.finish()
        }

        fn failure(code: u64, text: &str) -> (u16, Vec<u8>) {
            let mut w = Writer::new();
            w.uint(1, code).bytes(2, text.as_bytes());
            (m::FAILURE, w.finish())
        }

        /// The device's answer to a request, after any questions.
        fn answer(&self, msg_type: u16, data: &[u8]) -> (u16, Vec<u8>) {
            if let Some((failing, code)) = *self.fail.lock().unwrap()
                && failing == msg_type
            {
                return Self::failure(code, "refused");
            }
            match msg_type {
                m::INITIALIZE => (m::FEATURES, self.features_bytes()),
                m::MONERO_GET_WATCH_KEY => {
                    let mut w = Writer::new();
                    w.bytes(1, &self.view_key).bytes(2, self.address.as_bytes());
                    (m::MONERO_WATCH_KEY, w.finish())
                }
                m::MONERO_GET_ADDRESS => {
                    let mut w = Writer::new();
                    w.bytes(1, self.shown_address.as_deref().unwrap_or(&self.address).as_bytes());
                    (m::MONERO_ADDRESS, w.finish())
                }
                m::MONERO_KEY_IMAGE_EXPORT_INIT_REQUEST => (m::MONERO_KEY_IMAGE_EXPORT_INIT_ACK, vec![]),
                m::MONERO_KEY_IMAGE_SYNC_STEP_REQUEST => {
                    let r = Reader::parse(data).unwrap();
                    let mut w = Writer::new();
                    for (i, td) in r.all_messages(1).unwrap().iter().enumerate() {
                        let transfer = m::Transfer {
                            out_key: td.bytes(1).unwrap().try_into().unwrap(),
                            tx_pub_key: td.bytes(2).unwrap().try_into().unwrap(),
                            additional: td.bytes(3).map(|a| a.try_into().unwrap()),
                            index_in_tx: td.uint(4).unwrap(),
                            subaddress_minor: td.uint(6).unwrap_or(0) as u32,
                        };
                        let (image, c, rr) = (self.key_image)(&transfer);
                        let mut plain = image.to_vec();
                        plain.extend_from_slice(&c);
                        plain.extend_from_slice(&rr);
                        let nonce = [i as u8; 12];
                        let mut k = Writer::new();
                        k.bytes(1, &nonce).bytes(3, &crypto::seal(&self.enc_key, &nonce, &plain));
                        w.message(1, &k);
                    }
                    (m::MONERO_KEY_IMAGE_SYNC_STEP_ACK, w.finish())
                }
                m::MONERO_KEY_IMAGE_SYNC_FINAL_REQUEST => {
                    let mut w = Writer::new();
                    w.bytes(1, &self.enc_key);
                    (m::MONERO_KEY_IMAGE_SYNC_FINAL_ACK, w.finish())
                }
                _ => Self::failure(1, "unexpected message"),
            }
        }

        fn queue(&self, msg_type: u16, data: &[u8]) {
            self.pending.lock().unwrap().extend(wire::frames(msg_type, data));
        }

        /// Handles a whole request: asks what's scripted, else answers.
        fn received(&self, msg_type: u16, data: Vec<u8>) {
            self.log.lock().unwrap().push(msg_type);
            // An answer to a question of ours?
            let asking = self.asking.lock().unwrap().take();
            if let Some((ask, original, original_data)) = asking {
                match (ask, msg_type) {
                    (Asks::Button, m::BUTTON_ACK) | (Asks::Passphrase, m::PASSPHRASE_ACK) => {}
                    (Asks::Pin, m::PIN_MATRIX_ACK) => {
                        let pin = Reader::parse(&data).unwrap().string(1).unwrap_or_default();
                        if pin != self.pin {
                            let (t, d) = Self::failure(m::FAILURE_PIN_INVALID, "PIN invalid");
                            self.queue(t, &d);
                            return;
                        }
                    }
                    (_, m::CANCEL) => {
                        let (t, d) = Self::failure(m::FAILURE_ACTION_CANCELLED, "Cancelled");
                        self.queue(t, &d);
                        return;
                    }
                    _ => {
                        let (t, d) = Self::failure(1, "unexpected message");
                        self.queue(t, &d);
                        return;
                    }
                }
                self.proceed(original, original_data);
                return;
            }
            self.proceed(msg_type, data);
        }

        fn proceed(&self, msg_type: u16, data: Vec<u8>) {
            let next = {
                let mut asks = self.asks.lock().unwrap();
                if asks.is_empty() { None } else { Some(asks.remove(0)) }
            };
            match next {
                Some(Asks::Button) => {
                    let mut w = Writer::new();
                    w.uint(1, 8).bytes(4, b"confirm_tx");
                    self.queue(m::BUTTON_REQUEST, &w.finish());
                    *self.asking.lock().unwrap() = Some((Asks::Button, msg_type, data));
                }
                Some(Asks::Pin) => {
                    let mut w = Writer::new();
                    w.uint(1, 1);
                    self.queue(m::PIN_MATRIX_REQUEST, &w.finish());
                    *self.asking.lock().unwrap() = Some((Asks::Pin, msg_type, data));
                }
                Some(Asks::Passphrase) => {
                    self.queue(m::PASSPHRASE_REQUEST, &[]);
                    *self.asking.lock().unwrap() = Some((Asks::Passphrase, msg_type, data));
                }
                None => {
                    let (t, d) = self.answer(msg_type, &data);
                    self.queue(t, &d);
                }
            }
        }
    }

    impl Link for Fake {
        fn write(&self, report: &[u8]) -> std::result::Result<(), String> {
            let mut incoming = self.incoming.lock().unwrap();
            match incoming.as_mut() {
                None => *incoming = Some(wire::Reassembler::first(report)?),
                Some(message) => message.next(report)?,
            }
            if incoming.as_ref().is_some_and(wire::Reassembler::done) {
                let (msg_type, data) = incoming.take().expect("just checked").finish();
                drop(incoming);
                self.received(msg_type, data);
            }
            Ok(())
        }

        fn read(&self) -> std::result::Result<Vec<u8>, String> {
            let mut pending = self.pending.lock().unwrap();
            if pending.is_empty() {
                return Err("nothing to read".into());
            }
            Ok(pending.remove(0).to_vec())
        }
    }

    /// Answers every prompt the same way, counting them.
    pub struct Person {
        pub pin: Option<String>,
        pub passphrase: Passphrase,
        pub buttons: Mutex<Vec<String>>,
        pub pins_asked: Mutex<u32>,
        pub progress: Mutex<Vec<(u32, u32)>>,
    }

    impl Person {
        pub fn new() -> Person {
            Person { pin: Some("1234".into()), passphrase: Passphrase::Typed(Zeroizing::new(String::new())), buttons: Mutex::new(vec![]), pins_asked: Mutex::new(0), progress: Mutex::new(vec![]) }
        }
    }

    impl Prompts for Person {
        fn button(&self, what: &str) {
            self.buttons.lock().unwrap().push(what.to_owned());
        }
        fn pin(&self) -> Option<Zeroizing<String>> {
            *self.pins_asked.lock().unwrap() += 1;
            self.pin.clone().map(Zeroizing::new)
        }
        fn passphrase(&self, _on_device: bool) -> Passphrase {
            match &self.passphrase {
                Passphrase::OnDevice => Passphrase::OnDevice,
                Passphrase::Typed(t) => Passphrase::Typed(t.clone()),
                Passphrase::Cancelled => Passphrase::Cancelled,
            }
        }
        fn progress(&self, done: u32, total: u32) {
            self.progress.lock().unwrap().push((done, total));
        }
    }
}

#[cfg(test)]
mod tests {
    use curve25519_dalek::{EdwardsPoint, Scalar, constants::ED25519_BASEPOINT_TABLE};
    use monero_wallet::ed25519::Point;

    use super::fake::{Asks, Fake, Person};
    use super::*;

    #[test]
    fn opening_reads_the_features_and_answers_the_questions_on_the_way() {
        let mut fake = Fake::new();
        fake.asks = std::sync::Mutex::new(vec![Asks::Pin, Asks::Passphrase, Asks::Button]);
        fake.view_key = [7; 32];
        fake.address = "4abc".into();
        let person = Person::new();
        let session = Session::open(&fake, &person).unwrap();
        assert_eq!(session.features.name(), "Trezor Safe 3");
        let (key, address) = session.watch_key(Network::Mainnet).unwrap();
        assert_eq!((*key, address.as_str()), ([7; 32], "4abc"));
        assert_eq!(*person.pins_asked.lock().unwrap(), 1);
        assert_eq!(*person.buttons.lock().unwrap(), vec!["confirm_tx".to_string()]);
        assert_eq!(*fake.log.lock().unwrap(), vec![m::INITIALIZE, m::PIN_MATRIX_ACK, m::PASSPHRASE_ACK, m::BUTTON_ACK, m::MONERO_GET_WATCH_KEY]);
    }

    #[test]
    fn a_wrong_pin_and_a_cancel_are_told_apart() {
        let fake = Fake::new();
        *fake.asks.lock().unwrap() = vec![Asks::Pin];
        let mut person = Person::new();
        person.pin = Some("9999".into());
        assert!(matches!(Session::open(&fake, &person), Err(FobError::Trezor(text)) if text.contains("PIN")));
        *fake.asks.lock().unwrap() = vec![Asks::Pin];
        person.pin = None;
        assert!(matches!(Session::open(&fake, &person), Err(FobError::TrezorCancelled)));
        *fake.asks.lock().unwrap() = vec![Asks::Passphrase];
        person.passphrase = Passphrase::Cancelled;
        assert!(matches!(Session::open(&fake, &person), Err(FobError::TrezorCancelled)));
        *fake.fail.lock().unwrap() = Some((m::MONERO_GET_WATCH_KEY, m::FAILURE_ACTION_CANCELLED));
        person.passphrase = Passphrase::OnDevice;
        let session = Session::open(&fake, &person).unwrap();
        assert!(matches!(session.watch_key(Network::Stagenet), Err(FobError::TrezorCancelled)));
        // Any other refusal is said in Fob's words, never the device's.
        *fake.fail.lock().unwrap() = Some((m::MONERO_GET_WATCH_KEY, m::FAILURE_BUSY));
        assert!(matches!(session.watch_key(Network::Stagenet), Err(FobError::Trezor(t)) if t.contains("busy") && !t.contains("refused")));
        *fake.fail.lock().unwrap() = Some((m::MONERO_GET_WATCH_KEY, 77));
        assert!(matches!(session.watch_key(Network::Stagenet), Err(FobError::Trezor(t)) if t == "the Trezor refused (code 77)"));
    }

    #[test]
    fn the_address_the_device_shows_is_asked_for() {
        let mut fake = Fake::new();
        fake.address = "4abc".into();
        fake.shown_address = Some("4xyz".into());
        let person = Person::new();
        let session = Session::open(&fake, &person).unwrap();
        assert_eq!(session.address_shown(Network::Mainnet).unwrap(), "4xyz");
        assert_eq!(fake.log.lock().unwrap().last(), Some(&m::MONERO_GET_ADDRESS));
    }

    #[test]
    fn refuses_a_device_that_can_t_do_monero_or_isn_t_set_up_or_is_too_old() {
        let person = Person::new();
        let mut fake = Fake::new();
        fake.features.capabilities = vec![1];
        assert!(matches!(Session::open(&fake, &person), Err(FobError::Trezor(t)) if t.contains("can't hold Monero")));
        let mut fake = Fake::new();
        fake.features.initialized = false;
        assert!(matches!(Session::open(&fake, &person), Err(FobError::Trezor(t)) if t.contains("isn't set up")));
        let mut fake = Fake::new();
        fake.features.version = (2, 4, 3);
        assert!(matches!(Session::open(&fake, &person), Err(FobError::Trezor(t)) if t.contains("firmware 2.5.2")));
    }

    #[test]
    fn key_images_come_back_opened_and_checked_in_batches() {
        let secret = Scalar::from(4242u64);
        let out_key = (&secret * ED25519_BASEPOINT_TABLE).compress().to_bytes();
        let hashed: EdwardsPoint = Point::biased_hash(out_key).into();
        let image = (hashed * secret).compress().to_bytes();
        let mut fake = Fake::new();
        fake.key_image = Box::new(move |t: &m::Transfer| {
            let (c, r) = crypto::sign_key_image(&image, &secret, &t.out_key);
            (image, c, r)
        });
        let person = Person::new();
        let session = Session::open(&fake, &person).unwrap();
        let transfer = m::Transfer { out_key, tx_pub_key: [2; 32], additional: None, index_in_tx: 1, subaddress_minor: 0 };
        let images = session.key_images(Network::Stagenet, &vec![transfer.clone(); 23], &[0, 1]).unwrap();
        assert_eq!(images.len(), 23);
        assert!(images.iter().all(|i| *i == image));
        assert_eq!(*person.progress.lock().unwrap(), vec![(10, 23), (20, 23), (23, 23)]);
        assert_eq!(fake.log.lock().unwrap().iter().filter(|t| **t == m::MONERO_KEY_IMAGE_SYNC_STEP_REQUEST).count(), 3);
        assert!(session.key_images(Network::Stagenet, &[], &[]).unwrap().is_empty());

        // An image that isn't the output's is refused.
        let wrong = m::Transfer { out_key: (&Scalar::from(9u64) * ED25519_BASEPOINT_TABLE).compress().to_bytes(), ..transfer };
        assert!(matches!(session.key_images(Network::Stagenet, &[wrong], &[0]), Err(FobError::Trezor(t)) if t.contains("doesn't match")));
    }
}
