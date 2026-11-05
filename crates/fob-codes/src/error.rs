//! What can go wrong reading or making a code.

use fob_monero::error::MoneroError;

/// A code that can't be read or made.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum CodeError {
    #[error("that code isn't a payment or wallet code Fob can read")]
    BadCode,
    /// A scanned profile can't be read, or a code is too big to send, and why.
    #[error("{0}")]
    Profile(String),
    /// A name, note or photo can't be kept as it is, and why.
    #[error("{0}")]
    Contact(String),
    #[error(transparent)]
    Monero(#[from] MoneroError),
}

pub type Result<T> = std::result::Result<T, CodeError>;
