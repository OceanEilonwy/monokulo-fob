//! Everything Fob says. The other crates describe what happened with enums
//! and plain values; this crate alone turns those into words, so Android and
//! the desktop say the same things and neither has to write them.
//!
//! English only for now. Words differ by the device they're read on
//! ([`Device`]), which an app sets once as it starts.

pub mod activity;
pub mod backup;
pub mod catch_up;
pub mod clock;
pub mod contacts;
pub mod errors;
pub mod light;
pub mod manual;
pub mod node;
pub mod not_ready;
pub mod prices;
pub mod profile;
pub mod requests;
pub mod scanning;
pub mod split;

use std::sync::atomic::{AtomicU8, Ordering};

/// `text` with its first letter a capital.
pub fn capitalised(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

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

    /// How something's chosen on it: "tap", "click".
    pub fn press(self) -> &'static str {
        match self {
            Device::Phone => "tap",
            Device::Computer => "click",
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
