//! The app behind a client's screens. [`Engine`] holds what's unlocked and
//! which wallet is open, keeps it syncing, and publishes what the screens
//! show; [`form`] has the rules of the forms. A client draws these and does
//! what only it can do: windows, files, the clipboard.

pub mod engine;
pub mod picture;

pub use engine::{Engine, Listener, Met, Options, Phase, Update};
pub use fob_ffi::form;
pub use fob_ffi as views;
pub use fob_ffi::{Device, FobError, set_device};
pub use fob_wallet::words;

/// The device's clock, which only the client knows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Clock {
    /// Seconds since the epoch.
    pub now: u64,
    /// The offset from UTC now, in seconds.
    pub utc_offset: i32,
    /// "09:12" rather than "9:12 am".
    pub hours_24: bool,
}
