//! Fob's wallet: everything except drawing screens. Apps call this library
//! (Android and iOS through `fob-ffi`) and keep no wallet logic of their own.
//!
//! - [`Wallet`]: create, restore, open; sync, balance, activity; prepare and
//!   send payments.
//! - [`uri`]: payment codes (with ORF receipts) and wallet codes.
//! - [`zmq`]: instant updates from the node's ZMQ publisher, when turned on.
//! - [`price`]: what XMR is worth, now and when each payment moved.
//! - [`split`]: paying what's ready to spend back as pieces.
//! - [`contacts`]: the people this wallet pays, in the wallet file.
//! - [`requests`]: requests to be paid, each on an address of its own.
//! - [`profile`]: your name and photo, shared as a code on a new address
//!   each time.
//! - [`swap`]: touch phones, two Fobs swapping profiles over NFC.
//! - [`amount`], [`activity`], [`not_ready`], [`receipt`], [`qr`],
//!   [`pattern`]: text and pictures ready for the screen.

pub mod activity;
pub mod amount;
pub mod contacts;
mod decoys;
pub mod error;
pub mod fetch;
pub mod keys;
pub mod network;
pub mod pattern;
pub mod price;
pub mod profile;
pub mod not_ready;
pub mod node;
pub mod qr;
pub mod receipt;
pub mod requests;
pub mod split;
pub mod swap;
pub mod transport;
mod store;
pub mod uri;
pub mod wallet;
pub mod zmq;
mod zmtp;

use std::sync::LazyLock;

pub use error::{FobError, Result};
pub use keys::SeedKind;
pub use network::Network;
pub use zmq::ZmqStatus;
pub use wallet::{Activity, Balance, CodePreview, Direction, Prepared, Retried, SyncListener, SyncStatus, Wallet, preview_wallet_code, restore_point};

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
