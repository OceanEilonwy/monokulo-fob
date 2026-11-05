//! Monero addresses: whether one is good on a network, and how it's written.

use monero_wallet::address::MoneroAddress;

use crate::{
    error::{FobError, Result},
    network::Network,
};

/// The address's network, for a clear error when a code is for the other one.
pub fn check_address(network: Network, address: &str) -> Result<()> {
    if MoneroAddress::from_str(network.monero(), address).is_ok() {
        return Ok(());
    }
    let other = match network {
        Network::Mainnet => Network::Stagenet,
        Network::Stagenet => Network::Mainnet,
    };
    if MoneroAddress::from_str(other.monero(), address).is_ok() {
        return Err(FobError::WrongNetwork { expected: network.name(), found: other.name() });
    }
    Err(FobError::BadAddress(network.name()))
}

/// A piece of an address as Receive shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddressGroup {
    pub text: String,
    /// The first, middle and last groups stand out: the ones people check.
    pub highlighted: bool,
}

/// An address in groups of five characters (19 for a 95-character address;
/// an integrated one splits the same way, its last group shorter), with the
/// first, the middle (index count ÷ 2) and the last highlighted.
pub fn address_groups(address: &str) -> Vec<AddressGroup> {
    let chars: Vec<char> = address.trim().chars().collect();
    let count = chars.len().div_ceil(5);
    chars
        .chunks(5)
        .enumerate()
        .map(|(i, group)| AddressGroup { text: group.iter().collect(), highlighted: i == 0 || i == count / 2 || i + 1 == count })
        .collect()
}

/// An address in a field, its first and last five: "84Bv9 … 99fQ2". Short
/// text is kept as it is.
pub fn short(address: &str) -> String {
    let chars: Vec<char> = address.trim().chars().collect();
    if chars.len() <= 12 {
        return chars.into_iter().collect();
    }
    let (head, tail): (String, String) = (chars[..5].iter().collect(), chars[chars.len() - 5..].iter().collect());
    format!("{head} … {tail}")
}

/// An address cut to its ends, for a title: "86Ux … 7kQa".
pub fn short_title(address: &str) -> String {
    let n = address.len();
    if n <= 8 { address.to_owned() } else { format!("{} … {}", &address[..4], &address[n - 4..]) }
}
