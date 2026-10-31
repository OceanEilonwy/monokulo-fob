//! Fob's wallet: everything except drawing screens. Apps call this library
//! (Android and iOS through `fob-ffi`) and keep no wallet logic of their own.
//!
//! - [`Wallet`]: create, restore, open; sync, balance, activity; prepare and
//!   send payments.
//! - [`uri`]: payment codes (with ORF receipts) and wallet codes.
//! - [`amount`], [`receipt`], [`qr`]: text and pictures ready for the screen.

pub mod amount;
pub mod error;
pub mod fetch;
pub mod keys;
pub mod network;
pub mod qr;
pub mod receipt;
pub mod transport;
mod store;
pub mod uri;
pub mod wallet;

use std::sync::LazyLock;

pub use error::{FobError, Result};
pub use keys::SeedKind;
pub use network::Network;
pub use wallet::{Activity, Balance, Direction, Prepared, SyncListener, SyncStatus, Wallet, restore_point};

static RUNTIME: LazyLock<tokio::runtime::Runtime> = LazyLock::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("fob")
        .enable_all()
        .build()
        .expect("Fob's runtime starts")
});

/// The runtime Fob's background syncing runs on.
pub fn runtime() -> &'static tokio::runtime::Runtime {
    &RUNTIME
}
