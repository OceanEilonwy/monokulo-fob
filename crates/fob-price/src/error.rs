//! Why a price can't be had or a price setting can't be taken. `fob-words` says it.

use crate::{Failure, Transport};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PriceError {
    /// A currency Fob doesn't know.
    NoPrices { code: String },
    /// `provider` doesn't price this currency (named in the plural).
    NotPriced { provider: &'static str, currency: &'static str },
    TorNotYet,
    NotOverTransport { provider: &'static str, transport: Transport },
    /// Kraken has no market for this currency.
    NoRoute { currency: String },
    BadUrl { url: String },
    HttpsOnly,
    OnionSource,
    /// The HTTP client wouldn't start, and why.
    Client(String),
    /// A provider gave no price.
    Failed(Failure),
}

pub type Result<T> = std::result::Result<T, PriceError>;
