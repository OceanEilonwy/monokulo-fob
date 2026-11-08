//! Trezor hardware wallets. The spend key never leaves the device: Fob
//! holds the view key it exports and watches with that, asks it for the key
//! images that show when money is spent, and hands it each payment to sign.
//!
//! Only Trezors running trezor-core (Model T, Safe 3, Safe 5) hold Monero.
//! The protocol is Monero's own cold-signing one with the device, after
//! `src/device_trezor` in monero-project and `apps/monero` in
//! trezor-firmware: the device makes the transaction's keys, one-time
//! addresses, commitments and signatures; Fob supplies inputs with their
//! rings, builds the range proof when there are more than two outputs,
//! checks what comes back against what it sent, and puts the transaction
//! together.

pub(crate) mod crypto;
#[cfg(test)]
pub(crate) mod fake_signer;
pub mod messages;
pub mod proto;
pub(crate) mod session;
pub mod sign;
mod wire;
#[cfg(feature = "usb")]
pub mod usb;

pub use crypto::clear_torsion;
pub use messages::Transfer;
pub use session::{Link, Passphrase, Prompts, Session};
