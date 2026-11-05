//! What can be wrong with an amount or an address. `fob-words` says it.

use crate::network::Network;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoneroError {
    BadAmount,
    /// Not an address on this network, or on any.
    BadAddress(Network),
    /// An address or code for the other network.
    WrongNetwork { expected: Network, found: Network },
}

pub type Result<T> = std::result::Result<T, MoneroError>;
