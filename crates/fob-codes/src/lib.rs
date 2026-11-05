//! What passes between wallets: payment, wallet and profile codes, ORF
//! receipts, QR modules, and the NFC protocols that carry codes.

pub mod error;
pub mod names;
pub mod ndef;
pub mod nfc;
pub mod pay;
pub mod qr;
pub mod receipt;
pub mod swap;
pub mod uri;
