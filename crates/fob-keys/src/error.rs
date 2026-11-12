//! What can be wrong with a key or a seed phrase. `fob-words` says it,
//! through `fob-vault`'s `VaultError`.

use fob_monero::error::MoneroError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyError {
    BadSeed,
    BadKey,
    ViewOnly,
    Monero(MoneroError),
}

impl From<MoneroError> for KeyError {
    fn from(e: MoneroError) -> Self {
        KeyError::Monero(e)
    }
}

impl std::fmt::Display for KeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            KeyError::BadSeed => "not a valid seed phrase",
            KeyError::BadKey => "not a valid key",
            KeyError::ViewOnly => "a view-only wallet has no spend key",
            KeyError::Monero(_) => "not a valid address",
        })
    }
}

impl std::error::Error for KeyError {}

pub type Result<T> = std::result::Result<T, KeyError>;
