//! What can go wrong with an amount or an address.

/// An amount or an address that isn't right.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum MoneroError {
    #[error("that amount isn't valid")]
    BadAmount,
    #[error("that isn't a Monero address for {0}")]
    BadAddress(&'static str),
    #[error("the wallet code is for {found}, but this wallet is on {expected}")]
    WrongNetwork { expected: &'static str, found: &'static str },
}

pub type Result<T> = std::result::Result<T, MoneroError>;
