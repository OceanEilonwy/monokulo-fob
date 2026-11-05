//! What a contact, request or profile can refuse.

use fob_codes::error::CodeError;
use fob_monero::error::MoneroError;

/// Something typed that a contact, request or profile can't take.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum ScreenError {
    /// A contact can't be saved as it is, and why.
    #[error("{0}")]
    Contact(String),
    /// A request can't be made or found, and why.
    #[error("{0}")]
    Request(String),
    /// Your profile can't be saved or shared as it is, and why.
    #[error("{0}")]
    Profile(String),
    #[error(transparent)]
    Code(#[from] CodeError),
    #[error(transparent)]
    Monero(#[from] MoneroError),
}

pub type Result<T> = std::result::Result<T, ScreenError>;
