//! What Fob knows of Monero itself: which network, which address, how much,
//! and what that is in other money. No wallet state, no requests.

pub mod address;
pub mod amount;
pub mod error;
pub mod fiat;
pub mod network;
pub mod pattern;
