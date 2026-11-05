//! What can go wrong asking for a price.

use crate::Failure;

/// A price that couldn't be had.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum PriceError {
    /// Prices couldn't be had, or a price setting isn't one Fob has.
    #[error("{0}")]
    Price(String),
    /// A provider gave no price, and why in a word or two.
    #[error("{}", .0.text())]
    Failed(Failure),
}

pub type Result<T> = std::result::Result<T, PriceError>;
