//! Deterministic demo wallets for screenshots.
//!
//! A scene is a wallet that never touches a node, a clock or a random number:
//! the apps draw their real screens from it, so iPhone, iPad and macOS (and
//! later Android and GTK) show identical data in every screenshot. It plays
//! the role `crates/fob-gtk/src/scene.rs` plays for the GTK app, with fixed
//! data in place of a fresh wallet. The data is the one the design boards in
//! `docs/apple/design` draw: wallet "Spending" on stagenet holding
//! 0.4182 XMR (A$148.20), at 09:41 on 16 Nov 2026.
//!
//! This crate is plain data; `fob-ffi` turns it into the records the apps
//! already use (`scene(name)`).

/// One XMR in atomic units.
pub const XMR: u64 = 1_000_000_000_000;

/// Every scene's name, matching the GTK app's `FOB_SCENE` names where a screen
/// exists in both. Each names the screen the app opens on; they share one wallet.
pub const NAMES: &[&str] = &[
    "home", "receive", "send", "contacts", "coins", "settings", "payment", "split",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Network {
    Mainnet,
    Stagenet,
}

/// A scene: the screen to open and the wallet to draw it from.
#[derive(Clone, Debug, PartialEq)]
pub struct Scene {
    pub name: &'static str,
    pub wallet: Wallet,
}

/// What a scene wallet holds, with every amount both in atomic units and as
/// the screens write it.
#[derive(Clone, Debug, PartialEq)]
pub struct Wallet {
    pub name: &'static str,
    pub network: Network,
    /// The fixed clock, seconds since the Unix epoch (16 Nov 2026, 09:41 AEDT).
    pub now: u64,
    /// The sync chip: "Synced".
    pub sync_text: &'static str,
    pub currency: &'static str,
    pub balance: Balance,
    pub activity: Vec<Day>,
    pub contacts: Vec<Contact>,
    pub unsaved: Vec<Unsaved>,
    pub coins: Coins,
    /// The receipt of the payment the Payment scene opens.
    pub payment: Payment,
    pub receive_address: &'static str,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Balance {
    pub total: u64,
    pub ready: u64,
    pub not_ready: u64,
    /// "0.4182"
    pub total_text: &'static str,
    /// "A$148.20"
    pub fiat_text: &'static str,
    /// "0.3870"
    pub ready_text: &'static str,
    /// "0.0312"
    pub not_ready_text: &'static str,
    /// When the money not ready yet will be: "09:32".
    pub all_ready: &'static str,
}

/// A day of Activity under its label.
#[derive(Clone, Debug, PartialEq)]
pub struct Day {
    pub label: &'static str,
    pub rows: Vec<Row>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RowKind {
    Payment {
        tx: &'static str,
        outgoing: bool,
        atomic: u64,
        confirmations: u64,
        needed: u64,
    },
    Request {
        id: u64,
    },
}

/// One row of Activity, as Home draws it.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub kind: RowKind,
    pub title: &'static str,
    pub line: &'static str,
    /// With its sign: "−0.0312"; a request's ask.
    pub amount_text: &'static str,
    pub fiat_text: Option<&'static str>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Contact {
    pub id: u64,
    pub name: &'static str,
    pub initial: &'static str,
    pub send_address: Option<&'static str>,
    pub line: &'static str,
}

/// Paid, not saved.
#[derive(Clone, Debug, PartialEq)]
pub struct Unsaved {
    pub address: &'static str,
    pub name: &'static str,
    pub line: &'static str,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Coin {
    pub atomic: u64,
    pub amount_text: &'static str,
    pub fiat_text: &'static str,
    pub source: &'static str,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NotReadyCoin {
    pub atomic: u64,
    pub amount_text: &'static str,
    pub fiat_text: &'static str,
    pub source: &'static str,
    pub confirmations: u64,
    pub needed: u64,
    pub ready: &'static str,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Coins {
    pub intro: &'static str,
    pub ready_title: &'static str,
    pub ready_sum: &'static str,
    pub ready: Vec<Coin>,
    pub not_ready_title: &'static str,
    pub not_ready_sum: &'static str,
    pub not_ready: Vec<NotReadyCoin>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReceiptLine {
    pub label: &'static str,
    pub amount: &'static str,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Payment {
    pub tx: &'static str,
    pub merchant: &'static str,
    pub issued: &'static str,
    pub atomic: u64,
    pub amount_text: &'static str,
    pub lines: Vec<ReceiptLine>,
    pub total: &'static str,
    pub currency: &'static str,
}

/// The scene called `name`, or none for a name not in [`NAMES`].
pub fn scene(name: &str) -> Option<Scene> {
    let name = *NAMES.iter().find(|n| **n == name)?;
    Some(Scene {
        name,
        wallet: spending(),
    })
}

/// 0.4182 XMR in "Spending", as every board draws it.
pub fn spending() -> Wallet {
    const READY: u64 = 3_870 * XMR / 10_000;
    const NOT_READY: u64 = 312 * XMR / 10_000;
    Wallet {
        name: "Spending",
        network: Network::Stagenet,
        // 2026-11-16 09:41 in Sydney (UTC+11).
        now: 1_794_782_460,
        sync_text: "Synced",
        currency: "AUD",
        balance: Balance {
            total: READY + NOT_READY,
            ready: READY,
            not_ready: NOT_READY,
            total_text: "0.4182",
            fiat_text: "A$148.20",
            ready_text: "0.3870",
            not_ready_text: "0.0312",
            all_ready: "09:32",
        },
        activity: vec![
            Day {
                label: "TODAY",
                rows: vec![
                    Row {
                        kind: RowKind::Payment {
                            tx: TX_BAKERY,
                            outgoing: true,
                            atomic: NOT_READY,
                            confirmations: 14,
                            needed: 10,
                        },
                        title: "To Corner Bakery",
                        line: "Receipt, 2 items · 09:12",
                        amount_text: "−0.0312",
                        fiat_text: Some("A$11.05"),
                    },
                    Row {
                        kind: RowKind::Payment {
                            tx: TX_MARA,
                            outgoing: false,
                            atomic: 1_250 * XMR / 10_000,
                            confirmations: 3,
                            needed: 10,
                        },
                        title: "From Mara",
                        line: "Concert tickets · not ready · 3 of 10 · 08:40",
                        amount_text: "+0.1250",
                        fiat_text: Some("A$44.30"),
                    },
                    Row {
                        kind: RowKind::Request { id: 7 },
                        title: "Request to Joel",
                        line: "Dinner, Friday · waiting · 08:05",
                        amount_text: "A$20.00",
                        fiat_text: Some("asked"),
                    },
                ],
            },
            Day {
                label: "YESTERDAY",
                rows: vec![Row {
                    kind: RowKind::Payment {
                        tx: TX_JOEL,
                        outgoing: true,
                        atomic: 5_600 * XMR / 10_000,
                        confirmations: 160,
                        needed: 10,
                    },
                    title: "To Joel",
                    line: "Rent share · 19:31",
                    amount_text: "−0.5600",
                    fiat_text: Some("A$198.40"),
                }],
            },
        ],
        contacts: vec![
            Contact {
                id: 1,
                name: "Mara Okafor",
                initial: "M",
                send_address: Some(ADDRESS_MARA),
                line: "Concerts, coffee · paid 6 times, last today",
            },
            Contact {
                id: 2,
                name: "Joel",
                initial: "J",
                send_address: Some(ADDRESS_JOEL),
                line: "Flatmate · paid 14 times, last yesterday",
            },
            Contact {
                id: 3,
                name: "Priya",
                initial: "P",
                send_address: None,
                line: "Half set up",
            },
            Contact {
                id: 4,
                name: "Sam Whitlock",
                initial: "S",
                send_address: Some(ADDRESS_SAM),
                line: "Not paid yet",
            },
        ],
        unsaved: vec![Unsaved {
            address: ADDRESS_UNSAVED,
            name: "4AdU nnb4 … q2Ws",
            line: "paid 2 times, last 3 Nov",
        }],
        coins: Coins {
            intro: "Your 0.4182 XMR is held as 4 coins. A payment uses whole coins; what's left comes back as a new coin, not ready for about 20 minutes.",
            ready_title: "Ready to spend",
            ready_sum: "0.3870 · A$137.23",
            ready: vec![
                Coin {
                    atomic: 2_000 * XMR / 10_000,
                    amount_text: "0.2000",
                    fiat_text: "A$70.92",
                    source: "Received · 15 Nov",
                },
                Coin {
                    atomic: 1_250 * XMR / 10_000,
                    amount_text: "0.1250",
                    fiat_text: "A$44.33",
                    source: "Split up · 14 Nov",
                },
                Coin {
                    atomic: 620 * XMR / 10_000,
                    amount_text: "0.0620",
                    fiat_text: "A$21.99",
                    source: "Change · Joel",
                },
            ],
            not_ready_title: "Not ready · 1 coin",
            not_ready_sum: "0.0312 · A$11.06",
            not_ready: vec![NotReadyCoin {
                atomic: NOT_READY,
                amount_text: "0.0312",
                fiat_text: "A$11.06",
                source: "Change · Corner Bakery",
                confirmations: 3,
                needed: 10,
                ready: "09:32",
            }],
        },
        payment: Payment {
            tx: TX_BAKERY,
            merchant: "Corner Bakery",
            issued: "16 Nov 2026, 09:12",
            atomic: NOT_READY,
            amount_text: "0.0312",
            lines: vec![
                ReceiptLine {
                    label: "Flat white",
                    amount: "A$5.50",
                },
                ReceiptLine {
                    label: "Croissant",
                    amount: "A$5.55",
                },
            ],
            total: "A$11.05",
            currency: "AUD",
        },
        receive_address: ADDRESS_RECEIVE,
    }
}

// Fixed, made-up stagenet-shaped strings: shown shortened, never paid.
const TX_BAKERY: &str = "c9e1f00dfeedfacecafebeef0123456789abcdef0123456789abcdef000004af";
const TX_MARA: &str = "7b2a00112233445566778899aabbccddeeff00112233445566778899aabb0c01";
const TX_JOEL: &str = "e4d3c2b1a0998877665544332211000ffeeddccbbaa99887766554433221100a";
const ADDRESS_RECEIVE: &str = "74Fob1SceneReceiveAddressStagenetxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx9Qa";
const ADDRESS_MARA: &str = "74Fob1SceneMaraOkaforStagenetxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxM01";
const ADDRESS_JOEL: &str = "74Fob1SceneJoelStagenetxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxJ02";
const ADDRESS_SAM: &str = "74Fob1SceneSamWhitlockStagenetxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxS04";
const ADDRESS_UNSAVED: &str = "4AdUnnb4SceneUnsavedStagenetxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxq2Ws";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_name_is_a_scene_and_nothing_else_is() {
        for name in NAMES {
            assert_eq!(scene(name).unwrap().name, *name);
        }
        assert!(scene("nope").is_none());
    }

    #[test]
    fn scenes_are_deterministic() {
        assert_eq!(scene("home"), scene("home"));
        assert_eq!(
            scene("home").unwrap().wallet,
            scene("coins").unwrap().wallet
        );
    }

    #[test]
    fn the_balance_is_the_boards() {
        let w = spending();
        assert_eq!(w.name, "Spending");
        assert_eq!(w.network, Network::Stagenet);
        assert_eq!(w.balance.total, 418_200_000_000);
        assert_eq!(w.balance.ready + w.balance.not_ready, w.balance.total);
        assert_eq!(w.balance.total_text, "0.4182");
        assert_eq!(w.balance.fiat_text, "A$148.20");
    }

    #[test]
    fn the_coins_add_up_to_the_balance() {
        let w = spending();
        let ready: u64 = w.coins.ready.iter().map(|c| c.atomic).sum();
        let not_ready: u64 = w.coins.not_ready.iter().map(|c| c.atomic).sum();
        assert_eq!(ready, w.balance.ready);
        assert_eq!(not_ready, w.balance.not_ready);
    }

    #[test]
    fn the_receipt_matches_its_activity_row() {
        let w = spending();
        let row = &w.activity[0].rows[0];
        assert_eq!(row.title, format!("To {}", w.payment.merchant));
        assert_eq!(row.fiat_text, Some(w.payment.total));
        assert!(
            matches!(row.kind, RowKind::Payment { tx, atomic, .. } if tx == w.payment.tx && atomic == w.payment.atomic)
        );
    }
}
