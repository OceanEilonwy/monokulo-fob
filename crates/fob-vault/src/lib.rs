//! A wallet's secrets at rest: its keys, the encrypted database and files that
//! hold them, and the list of wallets on this device.

pub mod db;
pub mod error;
pub mod files;
pub mod keys;
pub mod locked;
pub mod passphrase;
pub mod process;
pub mod sealed;
pub mod store;
pub mod wallets;
