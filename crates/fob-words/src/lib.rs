//! Everything Fob says. The other crates describe what happened with enums
//! and plain values; this crate alone turns those into words, so Android and
//! the desktop say the same things and neither has to write them.
//!
//! English only for now. Words differ by the device they're read on
//! ([`Device`]), which an app sets once as it starts.

pub mod backup;
pub mod errors;
pub mod prices;

use std::sync::atomic::{AtomicU8, Ordering};

/// What Fob runs on, for sentences that name it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Device {
    Phone,
    Computer,
}

impl Device {
    /// "phone", "computer".
    pub fn noun(self) -> &'static str {
        match self {
            Device::Phone => "phone",
            Device::Computer => "computer",
        }
    }
}

static DEVICE: AtomicU8 = AtomicU8::new(0);

/// Says which device Fob is on. Phone unless told otherwise.
pub fn set_device(device: Device) {
    DEVICE.store(device as u8, Ordering::Relaxed);
}

/// The device Fob is on.
pub fn device() -> Device {
    if DEVICE.load(Ordering::Relaxed) == Device::Computer as u8 { Device::Computer } else { Device::Phone }
}
