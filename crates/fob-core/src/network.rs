use serde::{Deserialize, Serialize};

/// The Monero network a wallet lives on. Fob starts on stagenet: test coins,
/// nothing to lose.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Network {
    Mainnet,
    Stagenet,
}

/// A block on each network, to estimate a height from a date without asking
/// the node.
struct Anchor {
    height: u64,
    timestamp: u64,
}

const MAINNET_ANCHOR: Anchor = Anchor { height: 3_795_761, timestamp: 1_793_406_263 };
const STAGENET_ANCHOR: Anchor = Anchor { height: 2_240_272, timestamp: 1_793_406_347 };

/// Monero's target block time.
const BLOCK_SECONDS: u64 = 120;

/// How far before an estimated height to start scanning, so an early guess
/// never skips a payment: a week of blocks.
pub const RESTORE_MARGIN: u64 = 7 * 24 * 3600 / BLOCK_SECONDS;

impl Network {
    pub fn name(self) -> &'static str {
        match self {
            Network::Mainnet => "mainnet",
            Network::Stagenet => "stagenet",
        }
    }

    pub(crate) fn monero(self) -> monero_wallet::address::Network {
        match self {
            Network::Mainnet => monero_wallet::address::Network::Mainnet,
            Network::Stagenet => monero_wallet::address::Network::Stagenet,
        }
    }

    /// Public nodes Fob suggests, first one the default. Any can be replaced
    /// in Settings.
    pub fn default_nodes(self) -> &'static [&'static str] {
        match self {
            Network::Mainnet => &[
                "https://node.monerodevs.org:18089",
                "http://node.sethforprivacy.com:18089",
                "https://xmr-node.cakewallet.com:18081",
            ],
            Network::Stagenet => &[
                "http://stagenet.xmr-tw.org:38081",
                "http://node.monerodevs.org:38089",
                "http://node2.monerodevs.org:38089",
            ],
        }
    }

    fn anchor(self) -> Anchor {
        match self {
            Network::Mainnet => MAINNET_ANCHOR,
            Network::Stagenet => STAGENET_ANCHOR,
        }
    }

    /// The block height to start scanning from for a wallet made at
    /// `timestamp` (seconds since the epoch), with [`RESTORE_MARGIN`] taken
    /// off.
    pub fn restore_height_for(self, timestamp: u64) -> u64 {
        self.height_at(timestamp).saturating_sub(RESTORE_MARGIN)
    }

    /// The block height at `timestamp`, estimated, with no margin: for a
    /// Polyseed birthday, which is already rounded down to the month.
    pub fn height_at(self, timestamp: u64) -> u64 {
        let anchor = self.anchor();
        if timestamp >= anchor.timestamp {
            anchor.height + (timestamp - anchor.timestamp) / BLOCK_SECONDS
        } else {
            anchor.height.saturating_sub((anchor.timestamp - timestamp) / BLOCK_SECONDS)
        }
    }

    /// Roughly when `height` was mined, for showing "starts from 12 Mar".
    pub fn timestamp_for(self, height: u64) -> u64 {
        let anchor = self.anchor();
        if height >= anchor.height {
            anchor.timestamp + (height - anchor.height) * BLOCK_SECONDS
        } else {
            anchor.timestamp.saturating_sub((anchor.height - height) * BLOCK_SECONDS)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_date_a_week_after_the_anchor_starts_a_week_of_blocks_early() {
        let week = 7 * 24 * 3600;
        let height = Network::Mainnet.restore_height_for(MAINNET_ANCHOR.timestamp + week);
        assert_eq!(height, MAINNET_ANCHOR.height);
    }

    #[test]
    fn dates_before_monero_start_from_block_zero() {
        assert_eq!(Network::Stagenet.restore_height_for(0), 0);
    }

    #[test]
    fn height_and_timestamp_estimates_agree() {
        let t = Network::Stagenet.timestamp_for(2_000_000);
        assert_eq!(Network::Stagenet.restore_height_for(t), 2_000_000 - RESTORE_MARGIN);
    }
}
