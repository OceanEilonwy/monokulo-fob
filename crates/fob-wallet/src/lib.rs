//! Fob's wallet: the stateful part. [`Wallet`] holds what a wallet knows and runs
//! its background work, calling the crates below it. Apps call this library
//! (Android, and later GTK, through `fob-ffi`) and keep no wallet logic of their own.
//!
//! - [`Wallet`]: create, restore, open; sync, balance, activity; prepare and
//!   send payments.
//! - `fob-screens`: what each screen shows, from the wallet's records and the clock.
//! - `fob-words`: everything Fob says, from the scenarios the others describe.
//! - `fob-codes`: payment, wallet and profile codes, receipts, QR modules and NFC.
//! - `fob-vault`: keys, the encrypted database, and the wallets on this device.
//! - `fob-price`: what XMR is worth, now and when each payment moved.
//! - `fob-chain`: talking to a node, and instant updates over ZMQ.
//! - `fob-scanner`: finding the blocks that may hold this wallet's money.
//! - `fob-monero`: networks, addresses, amounts and fiat maths.
//!
//! Their modules are re-exported here under the names apps know them by.

pub mod error;
pub mod price_alert;
pub mod trezor;
pub mod wallet;

pub use fob_chain::{fetch, light, node, transport, zmq};
pub use fob_codes::{connect, ndef, pay, qr, receipt, swap, uri};
pub use fob_monero::{address, amount, fiat, network, pattern};
pub use fob_price as price;
pub use fob_scanner as scanning;
pub use fob_screens::{activity, catch_up, clock, coin_bar, contacts, manual, not_ready, profile, requests, split};
pub use fob_vault::{files, keys, passphrase, process, wallets};
pub use fob_words as words;
pub use fob_words::backup;

use fob_chain::{decoys, send_context};
use fob_scanner::scan;
use fob_vault::{db, store};

use std::sync::LazyLock;

pub use error::{FobError, Result};
pub use keys::SeedKind;
pub use network::{Network, ZmqStatus};
pub use wallet::{Activity, Balance, CodePreview, Direction, Prepared, Preview, Retried, SyncListener, SyncStatus, Wallet, preview_wallet_code, restore_point};
pub use wallet::{Account, AccountPayment, AccountView, Ask, Connected, Granted, Incoming, Shares, KeyedRequest, Unexpected};

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
