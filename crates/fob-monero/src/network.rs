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

/// How far from the anchor a height's date is still worth naming: a year
/// of blocks.
const DATED_SPAN: u64 = 365 * 24 * 3600 / BLOCK_SECONDS;

impl Network {
    pub fn name(self) -> &'static str {
        match self {
            Network::Mainnet => "mainnet",
            Network::Stagenet => "stagenet",
        }
    }

    pub fn monero(self) -> monero_wallet::address::Network {
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
                "https://xmr-node.cakewallet.com:18081",
                "https://node.sethforprivacy.com:443",
                "https://node.monerodevs.org:18089",
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

    /// When `height` was mined, close enough to name the day: within a year
    /// of the anchor. Further off, blocks that didn't come every two
    /// minutes on average make the estimate weeks or months out (mainnet's
    /// first two-minute block, in September 2015, comes out as March 2016).
    pub fn dated(self, height: u64) -> Option<u64> {
        (height.abs_diff(self.anchor().height) <= DATED_SPAN).then(|| self.timestamp_for(height))
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

/// The amount and its unit: ("1.8", "MB"), ("12", "MB"), or ("640", "kB")
/// under a megabyte (decimal units).
pub fn link_amount(bytes_per_second: f64) -> (String, &'static str) {
    let mb = bytes_per_second / 1e6;
    match () {
        _ if mb >= 9.95 => (format!("{}", mb.round()), "MB"),
        _ if mb >= 0.995 => (format!("{mb:.1}"), "MB"),
        _ => (format!("{}", (bytes_per_second / 1e3).round()), "kB"),
    }
}

/// "1.8 MB/s", "12 MB/s", or "640 kB/s" under a megabyte (decimal units).
pub fn link_speed(bytes_per_second: f64) -> String {
    let (amount, unit) = link_amount(bytes_per_second);
    format!("{amount} {unit}/s")
}

/// How instant updates are doing, for Settings.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum ZmqStatus {
    /// Turned off: polling alone.
    #[default]
    Off,
    /// Turned on, not connected yet (or not syncing just now).
    Connecting,
    /// Connected and subscribed.
    Listening,
    /// The last try failed, and why, in a few words. Retried with a
    /// growing pause, up to a minute.
    CantConnect(String),
}

impl ZmqStatus {
    /// The subtitle under "Instant updates (ZMQ)":
    /// "node.example:18083 · listening", or "Off".
    pub fn text(&self, endpoint: Option<&str>) -> String {
        let word = match self {
            ZmqStatus::Off => return "Off".to_owned(),
            ZmqStatus::Connecting => "connecting",
            ZmqStatus::Listening => "listening",
            ZmqStatus::CantConnect(_) => "can't connect",
        };
        let place = endpoint.map(|e| e.trim_start_matches("tcp://")).unwrap_or("");
        format!("{place} · {word}")
    }
}

/// Whether `host` is an onion service's (`….onion`, any case, with or
/// without the root's dot).
pub fn is_onion(host: &str) -> bool {
    let host = host.trim_end_matches('.');
    let n = host.len();
    host.eq_ignore_ascii_case("onion") || n > 6 && host.is_char_boundary(n - 6) && host[n - 6..].eq_ignore_ascii_case(".onion")
}

/// Confirmations before money can be spent.
pub const UNLOCK: u64 = monero_wallet::DEFAULT_LOCK_WINDOW as u64;

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
    fn only_heights_within_a_year_of_the_anchor_are_dated() {
        let a = MAINNET_ANCHOR;
        assert_eq!(Network::Mainnet.dated(a.height), Some(a.timestamp));
        assert_eq!(Network::Mainnet.dated(a.height - DATED_SPAN), Some(a.timestamp - DATED_SPAN * BLOCK_SECONDS));
        assert_eq!(Network::Mainnet.dated(a.height - DATED_SPAN - 1), None);
        assert_eq!(Network::Mainnet.dated(a.height + DATED_SPAN + 1), None);
        assert_eq!(Network::Stagenet.dated(0), None);
    }

    #[test]
    fn height_and_timestamp_estimates_agree() {
        let t = Network::Stagenet.timestamp_for(2_000_000);
        assert_eq!(Network::Stagenet.restore_height_for(t), 2_000_000 - RESTORE_MARGIN);
    }
}
