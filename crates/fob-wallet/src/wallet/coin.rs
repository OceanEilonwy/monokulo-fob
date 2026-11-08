//! Where a coin stands, and the one function that moves it.
//!
//! Every coin the wallet owns is in one of three states: free, held or
//! spent. Everything that happens to it is an [`Event`], a fact some
//! observer saw (a block, the pool, the device, the node, the person), and
//! [`step`] is the whole account of what each fact does to each state. The
//! match in `step` is exhaustive over the pair, so a forgotten case is a
//! build that fails, not a balance that climbs back eight minutes later
//! (decision 399). Nothing here does IO or knows a wallet: `State::apply`
//! in `state.rs` is the one door through which a coin changes, and it
//! calls `step`.
//!
//! The table. Rows are the state before, columns the event; each cell the
//! state after, and a dash no change. The arms of `step` are this table,
//! in this order.
//!
//! | from \ event | LearnedKeyImage | MinedSpend | PoolSpend | PoolLeft(tx) | DeviceSaid Unspent | DeviceSaid OnChain | DeviceSaid InPool | SentByUs | Released(tx) | Rewind |
//! | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
//! | Free{Unknown} | Free{Known} | Spent | Held InPool | – | – | Held DeviceChain | Held DevicePool | Held OurPayment | – | – |
//! | Free{Known} | – | Spent | Held InPool | – | – | Held DeviceChain | Held DevicePool | Held OurPayment | – | – |
//! | Held InPool{t} | ki | Spent | – | Free if t | – | Held DeviceChain | – | Held OurPayment | – | – |
//! | Held OurPayment{t} | ki | Spent | – | – | – | – | – | – | Free if t | – |
//! | Held DeviceChain | ki | Spent | – | – (364) | Free | – | Held DevicePool | Held OurPayment | – | – (364) |
//! | Held DevicePool | ki | Spent | Held InPool (399) | – (399) | Free | Held DeviceChain | – | Held OurPayment | – | – (364) |
//! | Spent{h} | ki | – | – | – | – | – | – | – | – | Free{ki} if from ≤ h, else – |
//!
//! The cells that carry a decision:
//!
//! - `MinedSpend` wins over everything but a later rewind: the chain is the
//!   truth. A `Held OurPayment` seen mined becomes `Spent`, and the
//!   payment's height is set by the same observation.
//! - `PoolLeft` frees only a hold that names that transaction. The device
//!   holds name none, so they stay (decision 364; the bug of 399).
//! - `PoolSpend` on `Held DevicePool` names the transaction (decision 399),
//!   so the pool's leaving can free it.
//! - `DeviceSaid Unspent` frees a device hold: the node was asked again and
//!   says no (decision 364). It does not free `InPool` or `OurPayment`:
//!   those name a transaction the device sync didn't look at.
//! - `Rewind` applies to the coin list, not just a coin: a coin whose own
//!   height is at or above `from` is dropped from the list. `step` sees
//!   `Rewind` only for coins that stay, and for those it only undoes a
//!   `Spent` whose spending height is at or above `from`.
//! - `LearnedKeyImage` changes `ki` in any state and nothing else.

use serde::{Deserialize, Serialize};

/// A transaction's hash. Never a string: the words that used to stand in
/// for one can't be written by accident, because they aren't hashes.
pub type TxHash = [u8; 32];

/// A coin's key image: a device's coin has none until the device is asked,
/// and a view-only wallet's never has one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyImage {
    Unknown,
    Known(#[serde(with = "hex32")] [u8; 32]),
}

/// Where a coin stands. Confirmations are not part of it: they are a
/// function of its height and the tip, decided when the question is asked.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Coin {
    /// Spendable once confirmed, if its key image is known.
    Free { ki: KeyImage },
    /// Not spendable now; the hold can be lifted.
    Held { ki: KeyImage, hold: Hold },
    /// Spent on the chain. Undone only by a rewind below `height`.
    Spent {
        ki: KeyImage,
        #[serde(with = "hex32")]
        tx: TxHash,
        height: u64,
    },
}

/// Why a coin isn't spendable now.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Hold {
    /// A transaction in the node's or the server's pool spends it.
    InPool {
        #[serde(with = "hex32")]
        tx: TxHash,
    },
    /// A payment of ours spends it; lifted by releasing the payment, or
    /// when the payment is seen mined (then `Spent`).
    OurPayment {
        #[serde(with = "hex32")]
        tx: TxHash,
    },
    /// A device's key image sync: the node said spent on the chain, and no
    /// transaction is named. Lifted only by the next sync (decision 364).
    /// `asked` is the chain's height when the node said so.
    DeviceChain { asked: u64 },
    /// A device's key image sync: in the pool only. Tentative: the next
    /// sync decides, or the pool transaction, once named, leaving.
    DevicePool { asked: u64 },
}

/// What an observer saw. Each is a fact about the world, not a decision.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// The device gave the coin's key image.
    LearnedKeyImage([u8; 32]),
    /// A block scanned, or the server, shows this transaction spending it.
    MinedSpend { tx: TxHash, height: u64 },
    /// The node's pool, or the server's, has this transaction spending it.
    PoolSpend { tx: TxHash },
    /// This transaction is no longer in the pool and wasn't seen mined.
    PoolLeft { tx: TxHash },
    /// The node's answer to `is_key_image_spent` at a device sync, with the
    /// chain's height when it answered.
    DeviceSaid { answer: Said, at: u64 },
    /// We built and broadcast a payment spending it.
    SentByUs { tx: TxHash },
    /// The person released that payment.
    Released { tx: TxHash },
    /// Blocks from `from` on are forgotten: a reorganisation or a rescan.
    Rewind { from: u64 },
}

/// The node's word on a key image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Said {
    Unspent,
    OnChain,
    InPool,
}

/// The one function: the state after `event` happens to `coin`. Total over
/// every pair; the table in the module comment, arm by arm.
pub fn step(coin: Coin, event: &Event) -> Coin {
    use Coin::{Free, Held, Spent};
    use Event::{
        DeviceSaid, LearnedKeyImage, MinedSpend, PoolLeft, PoolSpend, Released, Rewind, SentByUs,
    };
    use Hold::{DeviceChain, DevicePool, InPool, OurPayment};
    use KeyImage::Known;
    match (coin, *event) {
        // The key image is learned in any state, and nothing else moves.
        (Free { .. }, LearnedKeyImage(image)) => Free { ki: Known(image) },
        (Held { hold, .. }, LearnedKeyImage(image)) => Held { ki: Known(image), hold },
        (Spent { tx, height, .. }, LearnedKeyImage(image)) => {
            Spent { ki: Known(image), tx, height }
        }

        // The chain is the truth: a spend seen mined wins over every hold.
        (Free { ki } | Held { ki, .. }, MinedSpend { tx, height }) => Spent { ki, tx, height },
        (Spent { .. }, MinedSpend { .. }) => coin,

        // Free.
        (Free { ki }, PoolSpend { tx }) => Held { ki, hold: InPool { tx } },
        (Free { .. }, PoolLeft { .. }) => coin,
        (Free { .. }, DeviceSaid { answer: Said::Unspent, .. }) => coin,
        (Free { ki }, DeviceSaid { answer: Said::OnChain, at }) => {
            Held { ki, hold: DeviceChain { asked: at } }
        }
        (Free { ki }, DeviceSaid { answer: Said::InPool, at }) => {
            Held { ki, hold: DevicePool { asked: at } }
        }
        (Free { ki }, SentByUs { tx }) => Held { ki, hold: OurPayment { tx } },
        (Free { .. }, Released { .. }) => coin,
        (Free { .. }, Rewind { .. }) => coin,

        // Held by a pool transaction.
        (Held { hold: InPool { .. }, .. }, PoolSpend { .. }) => coin,
        (Held { ki, hold: InPool { tx: held } }, PoolLeft { tx }) => {
            if held == tx {
                Free { ki }
            } else {
                coin
            }
        }
        (Held { hold: InPool { .. }, .. }, DeviceSaid { answer: Said::Unspent, .. }) => coin,
        (Held { ki, hold: InPool { .. } }, DeviceSaid { answer: Said::OnChain, at }) => {
            Held { ki, hold: DeviceChain { asked: at } }
        }
        (Held { hold: InPool { .. }, .. }, DeviceSaid { answer: Said::InPool, .. }) => coin,
        (Held { ki, hold: InPool { .. } }, SentByUs { tx }) => Held { ki, hold: OurPayment { tx } },
        (Held { hold: InPool { .. }, .. }, Released { .. }) => coin,
        (Held { hold: InPool { .. }, .. }, Rewind { .. }) => coin,

        // Held by a payment of ours: only its release lets go.
        (Held { hold: OurPayment { .. }, .. }, PoolSpend { .. }) => coin,
        (Held { hold: OurPayment { .. }, .. }, PoolLeft { .. }) => coin,
        (Held { hold: OurPayment { .. }, .. }, DeviceSaid { .. }) => coin,
        (Held { hold: OurPayment { .. }, .. }, SentByUs { .. }) => coin,
        (Held { ki, hold: OurPayment { tx: held } }, Released { tx }) => {
            if held == tx {
                Free { ki }
            } else {
                coin
            }
        }
        (Held { hold: OurPayment { .. }, .. }, Rewind { .. }) => coin,

        // Held by a device sync that found it spent on the chain: it names
        // no transaction, so the pool's leaving isn't about it (decision
        // 364, and the bug of 399). Only the next sync lets go.
        (Held { hold: DeviceChain { .. }, .. }, PoolSpend { .. }) => coin,
        (Held { hold: DeviceChain { .. }, .. }, PoolLeft { .. }) => coin,
        (Held { ki, hold: DeviceChain { .. } }, DeviceSaid { answer: Said::Unspent, .. }) => {
            Free { ki }
        }
        (Held { hold: DeviceChain { .. }, .. }, DeviceSaid { answer: Said::OnChain, .. }) => coin,
        (Held { ki, hold: DeviceChain { .. } }, DeviceSaid { answer: Said::InPool, at }) => {
            Held { ki, hold: DevicePool { asked: at } }
        }
        (Held { ki, hold: DeviceChain { .. } }, SentByUs { tx }) => {
            Held { ki, hold: OurPayment { tx } }
        }
        (Held { hold: DeviceChain { .. }, .. }, Released { .. }) => coin,
        (Held { hold: DeviceChain { .. }, .. }, Rewind { .. }) => coin,

        // Held by a device sync that found it in the pool only: tentative.
        // The pool transaction, once named, takes over (decision 399), and
        // the next sync decides the rest.
        (Held { ki, hold: DevicePool { .. } }, PoolSpend { tx }) => {
            Held { ki, hold: InPool { tx } }
        }
        (Held { hold: DevicePool { .. }, .. }, PoolLeft { .. }) => coin,
        (Held { ki, hold: DevicePool { .. } }, DeviceSaid { answer: Said::Unspent, .. }) => {
            Free { ki }
        }
        (Held { ki, hold: DevicePool { .. } }, DeviceSaid { answer: Said::OnChain, at }) => {
            Held { ki, hold: DeviceChain { asked: at } }
        }
        (Held { hold: DevicePool { .. }, .. }, DeviceSaid { answer: Said::InPool, .. }) => coin,
        (Held { ki, hold: DevicePool { .. } }, SentByUs { tx }) => {
            Held { ki, hold: OurPayment { tx } }
        }
        (Held { hold: DevicePool { .. }, .. }, Released { .. }) => coin,
        (Held { hold: DevicePool { .. }, .. }, Rewind { .. }) => coin,

        // Spent: only a rewind below the spend's block undoes it.
        (Spent { .. }, PoolSpend { .. }) => coin,
        (Spent { .. }, PoolLeft { .. }) => coin,
        (Spent { .. }, DeviceSaid { .. }) => coin,
        (Spent { .. }, SentByUs { .. }) => coin,
        (Spent { .. }, Released { .. }) => coin,
        (Spent { ki, height, .. }, Rewind { from }) => {
            if from <= height {
                Free { ki }
            } else {
                coin
            }
        }
    }
}

/// Proof that a coin may be spent now: made only from a free coin with a
/// known key image and enough confirmations, by [`Coin::spendable`]. A
/// payment is built from these, so a device's coin the device hasn't been
/// asked about can't be chosen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Spendable {
    ki: [u8; 32],
}

impl Spendable {
    /// The coin's key image, which the proof was made from.
    pub fn key_image(&self) -> [u8; 32] {
        self.ki
    }
}

impl Default for Coin {
    /// A coin just found, nothing known of it: free, its key image not yet
    /// worked out or asked for.
    fn default() -> Self {
        Coin::Free { ki: KeyImage::Unknown }
    }
}

impl Coin {
    /// The coin's key image, as far as it's known.
    pub fn ki(&self) -> KeyImage {
        match self {
            Coin::Free { ki } | Coin::Held { ki, .. } | Coin::Spent { ki, .. } => *ki,
        }
    }

    /// The key image, if known.
    pub fn key_image(&self) -> Option<[u8; 32]> {
        match self.ki() {
            KeyImage::Known(image) => Some(image),
            KeyImage::Unknown => None,
        }
    }

    /// Free: not spent, and nothing holds it.
    pub fn is_free(&self) -> bool {
        matches!(self, Coin::Free { .. })
    }

    /// The transaction the state names as spending or holding the coin, if
    /// it names one. A device hold names none.
    pub fn tx(&self) -> Option<TxHash> {
        match self {
            Coin::Spent { tx, .. } => Some(*tx),
            Coin::Held { hold: Hold::InPool { tx } | Hold::OurPayment { tx }, .. } => Some(*tx),
            Coin::Held { .. } | Coin::Free { .. } => None,
        }
    }

    /// Whether the state names `tx`: see [`tx`](Coin::tx).
    pub fn names(&self, tx: &TxHash) -> bool {
        self.tx().as_ref() == Some(tx)
    }

    /// Proof the coin may be spent now, with `confirmations` of the
    /// `needed`: only a free coin whose key image is known gives one.
    pub fn spendable(&self, confirmations: u64, needed: u64) -> Option<Spendable> {
        match self {
            Coin::Free { ki: KeyImage::Known(ki) } if confirmations >= needed => {
                Some(Spendable { ki: *ki })
            }
            _ => None,
        }
    }
}

impl std::fmt::Display for Coin {
    /// Short, for the transition log: `Free`, `Held(InPool 1a2b…)`,
    /// `Spent(1a2b… at 3000000)`; a question mark for a key image not known.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ki = |ki: &KeyImage| if matches!(ki, KeyImage::Unknown) { "?" } else { "" };
        match self {
            Coin::Free { ki: k } => write!(f, "Free{}", ki(k)),
            Coin::Held { ki: k, hold } => write!(f, "Held{}({hold})", ki(k)),
            Coin::Spent { ki: k, tx, height } => {
                write!(f, "Spent{}({} at {height})", ki(k), short(tx))
            }
        }
    }
}

impl std::fmt::Display for Hold {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Hold::InPool { tx } => write!(f, "InPool {}", short(tx)),
            Hold::OurPayment { tx } => write!(f, "OurPayment {}", short(tx)),
            Hold::DeviceChain { asked } => write!(f, "DeviceChain asked at {asked}"),
            Hold::DevicePool { asked } => write!(f, "DevicePool asked at {asked}"),
        }
    }
}

impl std::fmt::Display for Event {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Event::LearnedKeyImage(_) => write!(f, "LearnedKeyImage"),
            Event::MinedSpend { tx, height } => write!(f, "MinedSpend {} at {height}", short(tx)),
            Event::PoolSpend { tx } => write!(f, "PoolSpend {}", short(tx)),
            Event::PoolLeft { tx } => write!(f, "PoolLeft {}", short(tx)),
            Event::DeviceSaid { answer, at } => write!(f, "DeviceSaid {answer:?} at {at}"),
            Event::SentByUs { tx } => write!(f, "SentByUs {}", short(tx)),
            Event::Released { tx } => write!(f, "Released {}", short(tx)),
            Event::Rewind { from } => write!(f, "Rewind from {from}"),
        }
    }
}

/// The first four bytes of a hash, in hex, with an ellipsis.
fn short(hash: &TxHash) -> String {
    format!("{}…", hex::encode(&hash[..4]))
}

/// A 32-byte hash as hex in the file, as the rest of the wallet keeps them.
mod hex32 {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8; 32], s: S) -> Result<S::Ok, S::Error> {
        hex::encode(bytes).serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 32], D::Error> {
        let text = String::deserialize(d)?;
        let bytes = hex::decode(&text).map_err(serde::de::Error::custom)?;
        <[u8; 32]>::try_from(bytes).map_err(|_| serde::de::Error::custom("a hash is 32 bytes"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const T: TxHash = [0x11; 32];
    const OTHER: TxHash = [0x22; 32];
    const IMAGE: [u8; 32] = [0x33; 32];
    const KNOWN: KeyImage = KeyImage::Known(IMAGE);

    fn free() -> Coin {
        Coin::Free { ki: KNOWN }
    }
    fn held(hold: Hold) -> Coin {
        Coin::Held { ki: KNOWN, hold }
    }
    fn spent(height: u64) -> Coin {
        Coin::Spent { ki: KNOWN, tx: T, height }
    }

    /// Every event there is, with `T` as the transaction named.
    fn events() -> Vec<Event> {
        vec![
            Event::LearnedKeyImage(IMAGE),
            Event::MinedSpend { tx: OTHER, height: 50 },
            Event::PoolSpend { tx: OTHER },
            Event::PoolLeft { tx: T },
            Event::PoolLeft { tx: OTHER },
            Event::DeviceSaid { answer: Said::Unspent, at: 70 },
            Event::DeviceSaid { answer: Said::OnChain, at: 70 },
            Event::DeviceSaid { answer: Said::InPool, at: 70 },
            Event::SentByUs { tx: OTHER },
            Event::Released { tx: T },
            Event::Released { tx: OTHER },
            Event::Rewind { from: 40 },
        ]
    }

    /// `from`, under every event in [`events`]' order, becomes `expect`.
    fn row(from: Coin, expect: [Coin; 12]) {
        for (event, after) in events().into_iter().zip(expect) {
            assert_eq!(step(from, &event), after, "{from} under {event}");
        }
    }

    #[test]
    fn a_free_coin_with_no_key_image() {
        let unknown = Coin::Free { ki: KeyImage::Unknown };
        let hold = |hold| Coin::Held { ki: KeyImage::Unknown, hold };
        row(
            unknown,
            [
                free(),
                Coin::Spent { ki: KeyImage::Unknown, tx: OTHER, height: 50 },
                hold(Hold::InPool { tx: OTHER }),
                unknown,
                unknown,
                unknown,
                hold(Hold::DeviceChain { asked: 70 }),
                hold(Hold::DevicePool { asked: 70 }),
                hold(Hold::OurPayment { tx: OTHER }),
                unknown,
                unknown,
                unknown,
            ],
        );
    }

    #[test]
    fn a_free_coin() {
        row(
            free(),
            [
                free(),
                Coin::Spent { ki: KNOWN, tx: OTHER, height: 50 },
                held(Hold::InPool { tx: OTHER }),
                free(),
                free(),
                free(),
                held(Hold::DeviceChain { asked: 70 }),
                held(Hold::DevicePool { asked: 70 }),
                held(Hold::OurPayment { tx: OTHER }),
                free(),
                free(),
                free(),
            ],
        );
    }

    #[test]
    fn a_coin_a_pool_transaction_holds() {
        let from = held(Hold::InPool { tx: T });
        row(
            from,
            [
                from,
                Coin::Spent { ki: KNOWN, tx: OTHER, height: 50 },
                from,
                free(),
                from,
                from,
                held(Hold::DeviceChain { asked: 70 }),
                from,
                held(Hold::OurPayment { tx: OTHER }),
                from,
                from,
                from,
            ],
        );
    }

    #[test]
    fn a_coin_our_payment_holds() {
        let from = held(Hold::OurPayment { tx: T });
        row(
            from,
            [
                from,
                Coin::Spent { ki: KNOWN, tx: OTHER, height: 50 },
                from,
                from,
                from,
                from,
                from,
                from,
                from,
                free(),
                from,
                from,
            ],
        );
    }

    #[test]
    fn a_coin_a_device_sync_found_spent_on_the_chain() {
        let from = held(Hold::DeviceChain { asked: 60 });
        row(
            from,
            [
                from,
                Coin::Spent { ki: KNOWN, tx: OTHER, height: 50 },
                from,
                from,
                from,
                free(),
                from,
                held(Hold::DevicePool { asked: 70 }),
                held(Hold::OurPayment { tx: OTHER }),
                from,
                from,
                from,
            ],
        );
    }

    #[test]
    fn a_coin_a_device_sync_found_in_the_pool() {
        let from = held(Hold::DevicePool { asked: 60 });
        row(
            from,
            [
                from,
                Coin::Spent { ki: KNOWN, tx: OTHER, height: 50 },
                held(Hold::InPool { tx: OTHER }),
                from,
                from,
                free(),
                held(Hold::DeviceChain { asked: 70 }),
                from,
                held(Hold::OurPayment { tx: OTHER }),
                from,
                from,
                from,
            ],
        );
    }

    #[test]
    fn a_spent_coin() {
        let from = spent(45);
        row(from, [from, from, from, from, from, from, from, from, from, from, from, free()]);
        assert_eq!(
            step(spent(39), &Event::Rewind { from: 40 }),
            spent(39),
            "spent below the rewind"
        );
        assert_eq!(
            step(spent(40), &Event::Rewind { from: 40 }),
            free(),
            "spent in the first block forgotten"
        );
    }

    #[test]
    fn learning_the_key_image_keeps_the_rest() {
        let unknown = KeyImage::Unknown;
        let learn = Event::LearnedKeyImage(IMAGE);
        assert_eq!(
            step(Coin::Held { ki: unknown, hold: Hold::InPool { tx: T } }, &learn),
            held(Hold::InPool { tx: T })
        );
        assert_eq!(step(Coin::Spent { ki: unknown, tx: T, height: 9 }, &learn), spent(9));
        let again = Event::LearnedKeyImage([0x44; 32]);
        assert_eq!(
            step(free(), &again),
            Coin::Free { ki: KeyImage::Known([0x44; 32]) },
            "the device's word is taken"
        );
    }

    /// Decision 399, as a sequence: a device sync found a coin spent on the
    /// chain, and the pool's bookkeeping then ran. The coin stays held.
    #[test]
    fn the_pool_leaving_does_not_free_what_a_device_sync_found() {
        let coin = step(free(), &Event::DeviceSaid { answer: Said::OnChain, at: 100 });
        let coin = step(coin, &Event::PoolLeft { tx: OTHER });
        assert_eq!(coin, held(Hold::DeviceChain { asked: 100 }));
        // The pool-only mark takes the pool transaction's name, and is then
        // freed by its leaving like any other.
        let coin = step(free(), &Event::DeviceSaid { answer: Said::InPool, at: 100 });
        let coin = step(coin, &Event::PoolLeft { tx: OTHER });
        assert_eq!(
            coin,
            held(Hold::DevicePool { asked: 100 }),
            "an empty pool frees nothing the device found"
        );
        let coin = step(coin, &Event::PoolSpend { tx: T });
        assert_eq!(coin, held(Hold::InPool { tx: T }));
        assert_eq!(step(coin, &Event::PoolLeft { tx: T }), free());
    }

    #[test]
    fn only_a_free_coin_with_a_known_key_image_is_spendable() {
        assert_eq!(free().spendable(10, 10).map(|s| s.key_image()), Some(IMAGE));
        assert!(free().spendable(9, 10).is_none(), "not confirmed enough");
        assert!(Coin::Free { ki: KeyImage::Unknown }.spendable(100, 10).is_none());
        assert!(held(Hold::InPool { tx: T }).spendable(100, 10).is_none());
        assert!(spent(1).spendable(100, 10).is_none());
    }

    #[test]
    fn a_coin_is_kept_as_tagged_json_with_hashes_in_hex() {
        let json = serde_json::to_value(held(Hold::InPool { tx: T })).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "Held": {
                    "ki": { "Known": hex::encode(IMAGE) },
                    "hold": { "InPool": { "tx": hex::encode(T) } },
                },
            })
        );
        let json =
            serde_json::to_value(Coin::Spent { ki: KeyImage::Unknown, tx: T, height: 7 }).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "Spent": { "ki": "Unknown", "tx": hex::encode(T), "height": 7 } })
        );
        for coin in [
            free(),
            held(Hold::DeviceChain { asked: 3 }),
            held(Hold::OurPayment { tx: T }),
            spent(5),
            Coin::default(),
        ] {
            let back: Coin = serde_json::from_value(serde_json::to_value(coin).unwrap()).unwrap();
            assert_eq!(back, coin);
        }
        let word = serde_json::json!({
            "Spent": { "ki": "Unknown", "tx": "spent-before-sync", "height": 7 },
        });
        assert!(serde_json::from_value::<Coin>(word).is_err(), "a word isn't a hash");
    }

    fn any_hash() -> impl Strategy<Value = TxHash> {
        prop_oneof![Just(T), Just(OTHER), any::<[u8; 32]>()]
    }

    fn any_ki() -> impl Strategy<Value = KeyImage> {
        prop_oneof![Just(KeyImage::Unknown), any::<[u8; 32]>().prop_map(KeyImage::Known)]
    }

    fn any_hold() -> impl Strategy<Value = Hold> {
        prop_oneof![
            any_hash().prop_map(|tx| Hold::InPool { tx }),
            any_hash().prop_map(|tx| Hold::OurPayment { tx }),
            any::<u64>().prop_map(|asked| Hold::DeviceChain { asked }),
            any::<u64>().prop_map(|asked| Hold::DevicePool { asked }),
        ]
    }

    fn any_coin() -> impl Strategy<Value = Coin> {
        prop_oneof![
            any_ki().prop_map(|ki| Coin::Free { ki }),
            (any_ki(), any_hold()).prop_map(|(ki, hold)| Coin::Held { ki, hold }),
            (any_ki(), any_hash(), any::<u64>()).prop_map(|(ki, tx, height)| Coin::Spent {
                ki,
                tx,
                height
            }),
        ]
    }

    fn any_said() -> impl Strategy<Value = Said> {
        prop_oneof![Just(Said::Unspent), Just(Said::OnChain), Just(Said::InPool)]
    }

    fn any_event() -> impl Strategy<Value = Event> {
        prop_oneof![
            any::<[u8; 32]>().prop_map(Event::LearnedKeyImage),
            (any_hash(), any::<u64>()).prop_map(|(tx, height)| Event::MinedSpend { tx, height }),
            any_hash().prop_map(|tx| Event::PoolSpend { tx }),
            any_hash().prop_map(|tx| Event::PoolLeft { tx }),
            (any_said(), any::<u64>()).prop_map(|(answer, at)| Event::DeviceSaid { answer, at }),
            any_hash().prop_map(|tx| Event::SentByUs { tx }),
            any_hash().prop_map(|tx| Event::Released { tx }),
            any::<u64>().prop_map(|from| Event::Rewind { from }),
        ]
    }

    proptest! {
        /// A spent coin is freed by nothing but a rewind to or below its
        /// spend's block, whatever happens to it in whatever order.
        #[test]
        fn spent_stays_spent_but_for_a_rewind(
            start in any_coin(),
            events in prop::collection::vec(any_event(), 0..40),
        ) {
            let mut coin = start;
            for event in &events {
                let after = step(coin, event);
                if let Coin::Spent { height, .. } = coin && after.is_free() {
                    let rewound = matches!(event, Event::Rewind { from } if *from <= height);
                    prop_assert!(rewound, "{coin} freed by {event}");
                }
                coin = after;
            }
        }

        /// What a device sync found is never let go by the pool's leaving.
        #[test]
        fn the_pool_leaving_never_lifts_a_device_hold(coin in any_coin(), tx in any_hash()) {
            let by_device = matches!(
                coin,
                Coin::Held { hold: Hold::DeviceChain { .. }, .. }
                    | Coin::Held { hold: Hold::DevicePool { .. }, .. }
            );
            if by_device {
                prop_assert_eq!(step(coin, &Event::PoolLeft { tx }), coin);
            }
        }

        #[test]
        fn nothing_spendable_comes_from_an_unknown_key_image(
            coin in any_coin(),
            confirmations in any::<u64>(),
            needed in any::<u64>(),
        ) {
            if let Some(spendable) = coin.spendable(confirmations, needed) {
                prop_assert_eq!(coin.key_image(), Some(spendable.key_image()));
                prop_assert!(coin.is_free() && confirmations >= needed);
            }
        }

        #[test]
        fn learning_a_key_image_changes_nothing_else(
            coin in any_coin(),
            image in any::<[u8; 32]>(),
        ) {
            let after = step(coin, &Event::LearnedKeyImage(image));
            prop_assert_eq!(after.key_image(), Some(image));
            let same_but_ki = match (coin, after) {
                (Coin::Free { .. }, Coin::Free { .. }) => true,
                (Coin::Held { hold: a, .. }, Coin::Held { hold: b, .. }) => a == b,
                (Coin::Spent { tx: a, height: h, .. }, Coin::Spent { tx: b, height: k, .. }) => {
                    a == b && h == k
                }
                _ => false,
            };
            prop_assert!(same_but_ki, "{coin} became {after}");
        }

        /// `step` is total: no pair panics, and the result can be written and read.
        #[test]
        fn every_pair_steps(coin in any_coin(), event in any_event()) {
            let after = step(coin, &event);
            let back: Coin = serde_json::from_value(serde_json::to_value(after).unwrap()).unwrap();
            prop_assert_eq!(back, after);
        }
    }
}
