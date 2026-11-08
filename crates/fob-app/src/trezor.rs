//! A Trezor on the computer: its USB link, and its questions put to the
//! client's screens. Fob talks to the device on a thread of its own and
//! waits there for each answer; the client draws the question and answers
//! on its main thread, through a [`TrezorRequest`]'s reply.

use std::sync::{Arc, Mutex, mpsc};

use fob_ffi::{FobError, TrezorLink, TrezorPassphrase, TrezorPrompts};
use fob_wallet::trezor::usb::UsbLink;
use zeroize::Zeroizing;

/// What the device asks of the person.
#[derive(Clone, Debug)]
pub enum TrezorAsk {
    /// No device has answered yet: Fob looks for one on USB until one does.
    Looking,
    /// The device answered the hello: its name, and whether it has a PIN
    /// and passphrase protection, which decide the steps to count.
    Hello { name: String, pin: bool, passphrase: bool },
    /// Look at the device: it shows something to confirm, named `what` by
    /// its firmware (`confirm_tx`, `show_address`, …).
    Button(String),
    /// Its PIN, by the positions of the digits on the keypad it shows.
    Pin,
    /// Its passphrase; `on_device` when the device can take it itself.
    Passphrase { on_device: bool },
    /// A long exchange: `done` of `total`.
    Progress { done: u32, total: u32 },
    /// The exchange is over, one way or the other: nothing more to show.
    Over,
}

/// The person's answer to a question that has one.
pub enum TrezorAnswer {
    Pin(Option<Zeroizing<String>>),
    Passphrase(TrezorPassphrase),
}

/// A question for the screens, with where the answer goes (for a question
/// that has one) and a way to give up on the whole exchange.
pub struct TrezorRequest {
    pub ask: TrezorAsk,
    pub reply: Option<mpsc::Sender<TrezorAnswer>>,
    pub cancel: Arc<dyn Fn() + Send + Sync>,
}

impl std::fmt::Debug for TrezorRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TrezorRequest({:?})", self.ask)
    }
}

/// Puts the device's questions to the client through `to`, which runs on
/// Fob's thread and must hand the request to the client's own without
/// waiting. The answers come back over the request's channel.
pub struct Prompter {
    to: Box<dyn Fn(TrezorRequest) + Send + Sync>,
    /// The device being talked to, for the cancel to let go of.
    link: Arc<Mutex<Option<Arc<Usb>>>>,
    /// The person gave up, perhaps before any device answered.
    cancelled: Arc<std::sync::atomic::AtomicBool>,
}

impl Prompter {
    pub fn new(to: impl Fn(TrezorRequest) + Send + Sync + 'static) -> Arc<Prompter> {
        Arc::new(Prompter { to: Box::new(to), link: Arc::new(Mutex::new(None)), cancelled: Arc::new(std::sync::atomic::AtomicBool::new(false)) })
    }

    fn cancel_handle(&self) -> Arc<dyn Fn() + Send + Sync> {
        let link = self.link.clone();
        let cancelled = self.cancelled.clone();
        Arc::new(move || {
            cancelled.store(true, std::sync::atomic::Ordering::SeqCst);
            if let Some(link) = link.lock().expect("link lock").as_ref() {
                link.close();
            }
        })
    }

    /// Whether the person gave up while Fob was still looking for a device.
    pub(crate) fn cancelled(&self) -> bool {
        self.cancelled.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Fob is looking for a device on USB.
    pub(crate) fn looking(&self) {
        self.cancelled.store(false, std::sync::atomic::Ordering::SeqCst);
        self.send(TrezorAsk::Looking);
    }

    /// Gives up: the device is let go, so Fob's wait ends with an error.
    pub fn cancel(&self) {
        (self.cancel_handle())();
    }

    fn send(&self, ask: TrezorAsk) {
        (self.to)(TrezorRequest { ask, reply: None, cancel: self.cancel_handle() });
    }

    fn ask(&self, ask: TrezorAsk) -> Option<TrezorAnswer> {
        let (reply, answer) = mpsc::channel();
        (self.to)(TrezorRequest { ask, reply: Some(reply), cancel: self.cancel_handle() });
        answer.recv().ok()
    }

    /// The exchange is over: the screens can close what they showed.
    pub(crate) fn over(&self) {
        *self.link.lock().expect("link lock") = None;
        self.send(TrezorAsk::Over);
    }

    pub(crate) fn hold(&self, link: Arc<Usb>) {
        *self.link.lock().expect("link lock") = Some(link);
    }
}

impl TrezorPrompts for Prompter {
    fn on_hello(&self, name: String, pin_protection: bool, passphrase_protection: bool) -> Result<(), FobError> {
        self.send(TrezorAsk::Hello { name, pin: pin_protection, passphrase: passphrase_protection });
        Ok(())
    }

    fn on_button(&self, what: String) -> Result<(), FobError> {
        self.send(TrezorAsk::Button(what));
        Ok(())
    }

    fn pin(&self) -> Result<Option<String>, FobError> {
        match self.ask(TrezorAsk::Pin) {
            Some(TrezorAnswer::Pin(pin)) => Ok(pin.map(|p| String::clone(&p))),
            _ => Ok(None),
        }
    }

    fn passphrase(&self, on_device: bool) -> Result<TrezorPassphrase, FobError> {
        match self.ask(TrezorAsk::Passphrase { on_device }) {
            Some(TrezorAnswer::Passphrase(p)) => Ok(p),
            _ => Ok(TrezorPassphrase::Cancelled),
        }
    }

    fn progress(&self, done: u32, total: u32) -> Result<(), FobError> {
        self.send(TrezorAsk::Progress { done, total });
        Ok(())
    }
}

/// The computer's USB link to the device, as the wallet library takes it.
pub struct Usb(UsbLink);

impl Usb {
    /// Opens the Trezor plugged in, or says why not.
    pub fn open() -> Result<Arc<Usb>, FobError> {
        UsbLink::open().map(|link| Arc::new(Usb(link))).map_err(|reason| FobError::Trezor { reason })
    }

    /// Ends any wait on the device.
    pub fn close(&self) {
        self.0.close();
    }
}

impl TrezorLink for Usb {
    fn write(&self, report: Vec<u8>) -> Result<(), FobError> {
        fob_wallet::trezor::Link::write(&self.0, &report).map_err(|reason| FobError::Trezor { reason })
    }

    fn read(&self) -> Result<Vec<u8>, FobError> {
        fob_wallet::trezor::Link::read(&self.0).map_err(|reason| FobError::Trezor { reason })
    }
}

/// Whether a Trezor is plugged in.
pub fn present() -> bool {
    fob_wallet::trezor::usb::present()
}
