//! What can go wrong with keys and the files that hold them.

use fob_monero::error::MoneroError;

/// Keys and the files that hold them.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum VaultError {
    #[error("that isn't a valid seed phrase")]
    BadSeed,
    #[error("that key isn't valid")]
    BadKey,
    #[error("this wallet can only watch: it has no spend key")]
    ViewOnly,
    #[error("couldn't read or save the wallet: {0}")]
    Storage(String),
    #[error("the wallet file can't be opened with this key")]
    WrongKey,
    /// A wallet's name can't be used, or the wallet isn't on the list, and why.
    #[error("{0}")]
    BadName(String),
    #[error(transparent)]
    Monero(#[from] MoneroError),
}

pub type Result<T> = std::result::Result<T, VaultError>;
