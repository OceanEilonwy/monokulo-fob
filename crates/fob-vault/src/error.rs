//! What can go wrong with keys and the files that hold them. `fob-words` says it.

use fob_monero::error::MoneroError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VaultError {
    BadSeed,
    BadKey,
    ViewOnly,
    /// The file system or database's own words: they name no scenario Fob has.
    Storage(String),
    /// Sealing a file failed.
    Sealing,
    NotAWalletFile,
    WrongKey,
    NameEmpty,
    NameTooLong { max: usize },
    NameTaken { name: String },
    NotOnThisDevice,
    NotAWalletFileName,
    Monero(MoneroError),
}

impl From<MoneroError> for VaultError {
    fn from(e: MoneroError) -> Self {
        VaultError::Monero(e)
    }
}

pub type Result<T> = std::result::Result<T, VaultError>;
