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
//!
//! Each scene opens the screen one or more boards draw
//! (`docs/apple/design/<board>.dc.html`):
//!
//! | scene | screen | boards |
//! |---|---|---|
//! | `home` | Wallet | Main, PhoneHomeLight, PadHome, MacHome |
//! | `home-syncing` | Wallet while catching up (sync chip) | Main, PadHome, MacHome |
//! | `receive` | Receive | PhoneReceive, PhoneReceiveLight |
//! | `send` | Send: camera and the people to pay | PhoneSend |
//! | `send-form` | Send: the form, to Joel | PhoneSendForm, PadSend |
//! | `confirm` | Confirm paying Corner Bakery (Face ID) | PhoneConfirm |
//! | `confirm-joel` | Confirm paying Joel (Touch ID) | MacSend |
//! | `pay-who` | "Pay who?" pushed from the form's To | PadSendTo |
//! | `payment` | Payment and its receipt | PhonePayment |
//! | `contacts` | Contacts | PhoneContacts |
//! | `settings` | Settings | PhoneSettings, MacSettings |
//! | `welcome` | Welcome, first run | PhoneWelcome |
//! | `backup` | Back up your wallet (seed words) | PhoneBackup |
//! | `catchup` | Catch up (scan) | PhoneCatchUp |
//! | `coins` | Coins | PhoneCoins |
//! | `split` | Split up | PhoneSplit |
//!
//! Component galleries (`gallery-<component>`) are not scenes: the apps'
//! FobUI gallery draws them without a wallet.

/// One XMR in atomic units.
pub const XMR: u64 = 1_000_000_000_000;

/// Every scene's name, matching the GTK app's `FOB_SCENE` names where a screen
/// exists in both. Each names the screen the app opens on; they share one wallet.
pub const NAMES: &[&str] = &[
    "home",
    "home-syncing",
    "receive",
    "send",
    "send-form",
    "confirm",
    "confirm-joel",
    "pay-who",
    "payment",
    "contacts",
    "settings",
    "welcome",
    "backup",
    "catchup",
    "coins",
    "split",
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
    /// The sync chip as the boards write it: "Synced", "4 min left".
    pub sync_text: &'static str,
    /// Where syncing stands, for the sync chip and its card.
    pub sync: SyncState,
    pub currency: &'static str,
    /// The price money is shown at: millionths of an Australian dollar per
    /// XMR (A$354.60).
    pub price: u64,
    pub balance: Balance,
    pub activity: Vec<Day>,
    pub contacts: Vec<Contact>,
    pub unsaved: Vec<Unsaved>,
    pub coins: Coins,
    /// The receipt of the payment the Payment scene opens.
    pub payment: Payment,
    pub receive_address: &'static str,
    /// Send: the form, the two confirm screens and the people to pay.
    pub send: Send,
    /// Settings' profile row.
    pub profile: Profile,
    pub settings: Settings,
    /// Back up your wallet: fixed demo words, never a real wallet's.
    pub seed_words: [&'static str; 16],
    pub welcome: Welcome,
    /// The Scan sheet (PhoneCatchUp).
    pub catch_up: CatchUp,
    /// Split up as the board opens it.
    pub split: Split,
}

/// Where syncing stands. The words come from fob-words, through fob-ffi.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SyncState {
    Synced {
        height: u64,
    },
    CatchingUp {
        scanned: u64,
        tip: u64,
        /// 0 to 1.
        progress: f32,
        seconds_left: u64,
        blocks_per_second: f64,
    },
    /// The node doesn't answer.
    Offline {
        scanned: u64,
        tip: u64,
    },
}

/// The chain's tip in every scene.
pub const TIP: u64 = 2_034_560;
/// The scene wallet's first block (its restore height).
pub const FIRST_BLOCK: u64 = 2_000_000;

/// One of each sync state, for the sync chip's gallery.
pub fn sync_states() -> [SyncState; 3] {
    [
        SyncState::Synced { height: TIP },
        catching_up(),
        SyncState::Offline {
            scanned: TIP - 120,
            tip: TIP,
        },
    ]
}

fn catching_up() -> SyncState {
    SyncState::CatchingUp {
        scanned: 2_023_000,
        tip: TIP,
        progress: 0.66,
        seconds_left: 240,
        blocks_per_second: 28.0,
    }
}

/// Send: everything its screens draw.
#[derive(Clone, Debug, PartialEq)]
pub struct Send {
    /// send-form: A$20.00 to Joel (PhoneSendForm, PadSend).
    pub form: SendForm,
    /// confirm: paying Corner Bakery (PhoneConfirm).
    pub confirm: Confirm,
    /// confirm-joel: paying Joel (MacSend).
    pub confirm_joel: Confirm,
    /// send / pay-who: the contacts to pay, most recently paid first.
    pub people: Vec<Person>,
    /// send / pay-who: "Paid before".
    pub paid_before: Vec<Unsaved>,
}

/// The send form, filled in.
#[derive(Clone, Debug, PartialEq)]
pub struct SendForm {
    /// The contact it pays: Joel.
    pub contact_id: u64,
    pub to_name: &'static str,
    pub to_address: &'static str,
    /// "46Qm TxVk … 9bRe"
    pub to_short: &'static str,
    /// Typed in money: "20.00", in [`Wallet::currency`].
    pub amount_text: &'static str,
    /// What that comes to: "0.0564".
    pub xmr_text: &'static str,
    /// Empty: "Note (optional)" shows.
    pub note: &'static str,
}

/// A confirm screen: the payment, its fee and what it comes to.
#[derive(Clone, Debug, PartialEq)]
pub struct Confirm {
    pub to_name: &'static str,
    /// A contact's letter; none for a shop.
    pub initial: Option<&'static str>,
    pub to_address: &'static str,
    /// "86Ux 3m9P … 7kQa"
    pub to_short: &'static str,
    pub atomic: u64,
    /// "0.0312"
    pub amount_text: &'static str,
    /// "A$11.05"
    pub amount_fiat: &'static str,
    /// The price's age beside it: "2 min old".
    pub price_age: &'static str,
    pub fee: u64,
    /// "0.00006"
    pub fee_text: &'static str,
    /// "A$0.02"
    pub fee_fiat: &'static str,
    /// With the fee: "0.03126"
    pub total_text: &'static str,
    pub note: Option<&'static str>,
    pub receipt: Option<Receipt>,
}

/// A receipt as Confirm shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct Receipt {
    pub merchant: &'static str,
    pub issued: &'static str,
    pub lines: Vec<ReceiptLine>,
    /// "A$1.00", included in the total.
    pub tax: Option<&'static str>,
    pub total: &'static str,
    pub currency: &'static str,
}

/// Someone Send offers to pay.
#[derive(Clone, Debug, PartialEq)]
pub struct Person {
    pub contact_id: u64,
    pub name: &'static str,
    pub initial: &'static str,
    /// "Paid yesterday · Rent share", "Not paid yet".
    pub line: &'static str,
}

/// Settings' profile row: "H", "Henry", "Your profile · share as a code".
#[derive(Clone, Debug, PartialEq)]
pub struct Profile {
    pub initial: &'static str,
    pub name: &'static str,
}

/// Settings' values, as the boards write them.
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    /// Wallets: "Spending".
    pub wallet: &'static str,
    /// Backup and keys: "backed up".
    pub backup: &'static str,
    /// Appearance: "OLED".
    pub appearance: &'static str,
    /// Default currency: "AUD".
    pub currency: &'static str,
    /// Price from: "CoinGecko".
    pub price_from: &'static str,
    /// Sync through: "Node" (or "Server").
    pub sync_through: &'static str,
    /// Node: "node.monokulo.net".
    pub node: &'static str,
    /// Scanning: "GPU · Apple A18".
    pub scanning: &'static str,
    /// Mac: Lock when left alone: "After 15 minutes".
    pub lock_after: &'static str,
    /// Mac: Unlock with: "Touch ID".
    pub unlock_with: &'static str,
}

/// Welcome, the first run.
#[derive(Clone, Debug, PartialEq)]
pub struct Welcome {
    pub network: Network,
    /// "On stagenet, with test coins"
    pub network_line: &'static str,
}

/// The Scan sheet: its modes and the Quick tab's choices.
#[derive(Clone, Debug, PartialEq)]
pub struct CatchUp {
    /// "Quick", "Dates", "Block"; Quick shown.
    pub modes: [&'static str; 3],
    /// What each Quick choice costs, as the board says it, in the order
    /// From last scan, Today, Past week, Past month, The whole wallet.
    pub costs: [&'static str; 5],
    /// The choice ticked: Past week.
    pub selected: usize,
    /// The button: "Scan the past week".
    pub action: &'static str,
    /// Blocks since the last scan.
    pub behind: u64,
}

/// Split up: how many of each note (A$5, 10, 20, 50, 100, 200) the board
/// opens with, and the payments of the last 30 days that teach Auto.
#[derive(Clone, Debug, PartialEq)]
pub struct Split {
    pub counts: [u32; 6],
    pub payments: Vec<u64>,
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
    let mut wallet = spending();
    // Catching up: the chip says how long is left, the balance stays.
    if matches!(name, "home-syncing" | "catchup") {
        wallet.sync = catching_up();
        wallet.sync_text = "4 min left";
    }
    Some(Scene { name, wallet })
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
        sync: SyncState::Synced { height: TIP },
        currency: "AUD",
        price: 354_600_000,
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
                rows: vec![
                    Row {
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
                    },
                    // PadHome shows it; on iPhone it's below the fold.
                    Row {
                        kind: RowKind::Payment {
                            tx: TX_PROFILE,
                            outgoing: false,
                            atomic: 2_000 * XMR / 10_000,
                            confirmations: 410,
                            needed: 10,
                        },
                        title: "Via your profile",
                        line: "14:02",
                        amount_text: "+0.2000",
                        fiat_text: Some("A$70.86"),
                    },
                ],
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
        send: send(),
        profile: Profile {
            initial: "H",
            name: "Henry",
        },
        settings: Settings {
            wallet: "Spending",
            backup: "backed up",
            appearance: "OLED",
            currency: "AUD",
            price_from: "CoinGecko",
            sync_through: "Node",
            node: "node.monokulo.net",
            scanning: "GPU · Apple A18",
            lock_after: "After 15 minutes",
            unlock_with: "Touch ID",
        },
        seed_words: [
            "timber", "orbit", "velvet", "harbor", "lemon", "sketch", "plunge", "meadow", "cactus",
            "ribbon", "frozen", "wander", "pillow", "summit", "glide", "anchor",
        ],
        welcome: Welcome {
            network: Network::Stagenet,
            network_line: "On stagenet, with test coins",
        },
        catch_up: CatchUp {
            modes: ["Quick", "Dates", "Block"],
            costs: [
                "seconds",
                "seconds",
                "about 1 min",
                "about 4 min",
                "about 20 min",
            ],
            selected: 2,
            action: "Scan the past week",
            behind: 120,
        },
        split: Split {
            counts: [2, 3, 4, 0, 0, 0],
            // A$4 twice, A$9 three times, A$18 four times: Auto learns
            // A$5, A$10 and A$20 coins in the board's 2 : 3 : 4.
            payments: vec![
                aud(4),
                aud(4),
                aud(9),
                aud(9),
                aud(9),
                aud(18),
                aud(18),
                aud(18),
                aud(18),
            ],
        },
    }
}

/// `dollars` Australian in XMR at the scene's price, to the atomic unit.
const fn aud(dollars: u64) -> u64 {
    (dollars as u128 * XMR as u128 * 1_000_000 / 354_600_000) as u64
}

fn send() -> Send {
    const FEE: u64 = 60_000_000;
    Send {
        form: SendForm {
            contact_id: 2,
            to_name: "Joel",
            to_address: ADDRESS_JOEL,
            to_short: "46Qm TxVk … 9bRe",
            amount_text: "20.00",
            xmr_text: "0.0564",
            note: "",
        },
        confirm: Confirm {
            to_name: "Corner Bakery",
            initial: None,
            to_address: ADDRESS_BAKERY,
            to_short: "86Ux 3m9P … 7kQa",
            atomic: 312 * XMR / 10_000,
            amount_text: "0.0312",
            amount_fiat: "A$11.05",
            price_age: "2 min old",
            fee: FEE,
            fee_text: "0.00006",
            fee_fiat: "A$0.02",
            total_text: "0.03126",
            note: None,
            receipt: Some(Receipt {
                merchant: "Corner Bakery",
                issued: "16 Nov 2026, 09:41",
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
                tax: Some("A$1.00"),
                total: "A$11.05",
                currency: "AUD",
            }),
        },
        confirm_joel: Confirm {
            to_name: "Joel",
            initial: Some("J"),
            to_address: ADDRESS_JOEL,
            to_short: "46Qm TxVk … 9bRe",
            atomic: 564 * XMR / 10_000,
            amount_text: "0.0564",
            amount_fiat: "A$20.00",
            price_age: "2 min old",
            fee: FEE,
            fee_text: "0.00006",
            fee_fiat: "A$0.02",
            total_text: "0.05646",
            note: Some("Dinner, Friday"),
            receipt: None,
        },
        people: vec![
            Person {
                contact_id: 2,
                name: "Joel",
                initial: "J",
                line: "Paid yesterday · Rent share",
            },
            Person {
                contact_id: 1,
                name: "Mara Okafor",
                initial: "M",
                line: "Paid 3 Nov · Concert tickets",
            },
            Person {
                contact_id: 4,
                name: "Sam Whitlock",
                initial: "S",
                line: "Not paid yet",
            },
        ],
        paid_before: vec![Unsaved {
            address: ADDRESS_BAKERY,
            name: "Corner Bakery",
            line: "86Ux 3m9P … 7kQa · today",
        }],
    }
}

// Fixed, made-up stagenet-shaped strings: shown shortened, never paid.
const TX_BAKERY: &str = "c9e1f00dfeedfacecafebeef0123456789abcdef0123456789abcdef000004af";
const TX_MARA: &str = "7b2a00112233445566778899aabbccddeeff00112233445566778899aabb0c01";
const TX_JOEL: &str = "e4d3c2b1a0998877665544332211000ffeeddccbbaa99887766554433221100a";
const TX_PROFILE: &str = "5a5a00c0ffee00112233445566778899aabbccddeeff0011223344556677d14a";
// Receive's address as PhoneReceive draws it, 19 groups of five.
const ADDRESS_RECEIVE: &str = "5AqHtJB65J3Lu7MZNBt3oj8eDqKVeiVFsRwNfnvdNvgLiH21DJZWwE1UpyhySdmjheGZJAeWQbWvqc9STx2MP8M63KcUDet";
const ADDRESS_MARA: &str = "74Fob1SceneMaraOkaforStagenetxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxM01";
const ADDRESS_JOEL: &str = "46QmTxVkSceneJoelStagenetxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx9bRe";
const ADDRESS_BAKERY: &str = "86Ux3m9PSceneCornerBakeryStagenetxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx7kQa";
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
    fn every_board_has_its_scene() {
        for name in [
            "home",
            "home-syncing",
            "receive",
            "send",
            "send-form",
            "confirm",
            "confirm-joel",
            "pay-who",
            "payment",
            "contacts",
            "settings",
            "welcome",
            "backup",
            "catchup",
            "coins",
            "split",
        ] {
            assert!(NAMES.contains(&name), "{name} is missing");
        }
        assert_eq!(NAMES.len(), 16);
        let mut sorted = NAMES.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), NAMES.len(), "names are unique");
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

    /// "46Qm TxVk … 9bRe": two groups of four, then the last four.
    fn grouped_short(address: &str) -> String {
        let n = address.len();
        format!(
            "{} {} … {}",
            &address[..4],
            &address[4..8],
            &address[n - 4..]
        )
    }

    #[test]
    fn send_shows_the_boards() {
        let s = spending().send;
        let f = &s.form;
        assert_eq!(
            (f.to_name, f.to_short, f.amount_text, f.xmr_text, f.note),
            ("Joel", "46Qm TxVk … 9bRe", "20.00", "0.0564", "")
        );
        assert_eq!(grouped_short(f.to_address), f.to_short);
        let joel = spending()
            .contacts
            .into_iter()
            .find(|c| c.id == f.contact_id)
            .unwrap();
        assert_eq!(joel.name, f.to_name);
        assert_eq!(joel.send_address, Some(f.to_address));

        let c = &s.confirm;
        assert_eq!(
            (c.to_name, c.amount_text, c.amount_fiat, c.price_age),
            ("Corner Bakery", "0.0312", "A$11.05", "2 min old")
        );
        assert_eq!(
            (c.to_short, c.fee_text, c.fee_fiat, c.total_text),
            ("86Ux 3m9P … 7kQa", "0.00006", "A$0.02", "0.03126")
        );
        assert_eq!(grouped_short(c.to_address), c.to_short);
        assert_eq!(c.atomic + c.fee, 31_260_000_000);
        let r = c.receipt.as_ref().unwrap();
        let lines: Vec<_> = r.lines.iter().map(|l| (l.label, l.amount)).collect();
        assert_eq!(lines, [("Flat white", "A$5.50"), ("Croissant", "A$5.55")]);
        assert_eq!((r.tax, r.total), (Some("A$1.00"), "A$11.05"));

        let j = &s.confirm_joel;
        assert_eq!(
            (j.to_name, j.initial, j.amount_text, j.amount_fiat),
            ("Joel", Some("J"), "0.0564", "A$20.00")
        );
        assert_eq!(
            (j.note, j.total_text, j.receipt.is_none()),
            (Some("Dinner, Friday"), "0.05646", true)
        );
        assert_eq!(j.atomic + j.fee, 56_460_000_000);

        let people: Vec<_> = s
            .people
            .iter()
            .map(|p| (p.initial, p.name, p.line))
            .collect();
        assert_eq!(
            people,
            [
                ("J", "Joel", "Paid yesterday · Rent share"),
                ("M", "Mara Okafor", "Paid 3 Nov · Concert tickets"),
                ("S", "Sam Whitlock", "Not paid yet")
            ]
        );
        for p in &s.people {
            assert!(
                spending()
                    .contacts
                    .iter()
                    .any(|c| c.id == p.contact_id && c.name == p.name)
            );
        }
        assert_eq!(
            (s.paid_before[0].name, s.paid_before[0].line),
            ("Corner Bakery", "86Ux 3m9P … 7kQa · today")
        );
    }

    #[test]
    fn settings_welcome_backup_and_scan_are_the_boards() {
        let w = spending();
        assert_eq!((w.profile.initial, w.profile.name), ("H", "Henry"));
        let s = &w.settings;
        assert_eq!(
            [
                s.wallet,
                s.backup,
                s.appearance,
                s.currency,
                s.price_from,
                s.sync_through,
                s.node,
                s.scanning,
                s.lock_after,
                s.unlock_with
            ],
            [
                "Spending",
                "backed up",
                "OLED",
                "AUD",
                "CoinGecko",
                "Node",
                "node.monokulo.net",
                "GPU · Apple A18",
                "After 15 minutes",
                "Touch ID"
            ]
        );
        assert_eq!(w.seed_words.len(), 16);
        assert_eq!((w.seed_words[0], w.seed_words[15]), ("timber", "anchor"));
        assert_eq!(
            (w.welcome.network, w.welcome.network_line),
            (Network::Stagenet, "On stagenet, with test coins")
        );
        let c = &w.catch_up;
        assert_eq!(c.modes, ["Quick", "Dates", "Block"]);
        assert_eq!(
            c.costs,
            [
                "seconds",
                "seconds",
                "about 1 min",
                "about 4 min",
                "about 20 min"
            ]
        );
        assert_eq!((c.selected, c.action), (2, "Scan the past week"));
    }

    #[test]
    fn catching_up_only_where_the_boards_show_it() {
        for name in NAMES {
            let w = scene(name).unwrap().wallet;
            let syncing = matches!(*name, "home-syncing" | "catchup");
            assert_eq!(
                matches!(w.sync, SyncState::CatchingUp { .. }),
                syncing,
                "{name}"
            );
            assert_eq!(w.sync_text, if syncing { "4 min left" } else { "Synced" });
            assert_eq!(w.balance.total_text, "0.4182");
        }
        let states = sync_states();
        assert!(matches!(states[0], SyncState::Synced { .. }));
        assert!(matches!(
            states[1],
            SyncState::CatchingUp {
                seconds_left: 240,
                ..
            }
        ));
        assert!(matches!(states[2], SyncState::Offline { .. }));
    }

    #[test]
    fn yesterday_has_the_profile_payment() {
        let w = spending();
        let yesterday = &w.activity[1];
        assert_eq!(yesterday.label, "YESTERDAY");
        let r = &yesterday.rows[1];
        assert_eq!(
            (r.title, r.line, r.amount_text, r.fiat_text),
            ("Via your profile", "14:02", "+0.2000", Some("A$70.86"))
        );
        assert_eq!(w.activity[0].rows.len(), 3);
    }

    #[test]
    fn split_opens_as_the_board() {
        let w = spending();
        assert_eq!(w.split.counts, [2, 3, 4, 0, 0, 0]);
        assert_eq!(w.split.payments.len(), 9);
        assert!(w.split.payments.iter().all(|p| *p < 20 * XMR / 354));
    }
}
