//! A wallet's keys: seed phrases, view-only keys, addresses in any account,
//! the key-image and output helpers, and how secrets are held in memory.
//!
//! Nothing here touches a database or the file system, so an app that only
//! watches a wallet (Monokular's Till) can take it alone. `fob-vault` keeps
//! the keys at rest and re-exports these modules under its own names.

pub mod error;
pub mod keys;
pub mod locked;
pub mod sealed;

pub use error::{KeyError, Result};
pub use keys::{Device, Keys, Restored, SeedKind};
pub use monero_wallet::address::SubaddressIndex;
