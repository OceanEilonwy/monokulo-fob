//! Demo wallets for screenshots (`crates/fob-scenes`), as the records the
//! apps already draw: an app launched with `-FobScene <name>` draws its real
//! screens from [`scene`] with no node, no clock and no randomness.

use crate::{
    Activity, ActivityDay, ActivityRow, Balance, Coin, CoinSegment, Coins, ContactRow, Network,
    NotReady, NotReadyPiece, PaidBefore, PaymentStatus, Receipt, ReceiptLine,
};

/// A demo wallet, opened on the screen its name says.
#[derive(uniffi::Record)]
pub struct SceneWallet {
    /// The scene: "home", "receive", "coins"…, the screen to open on.
    pub scene: String,
    /// "Spending"
    pub wallet_name: String,
    pub network: Network,
    /// The fixed clock, seconds since the Unix epoch.
    pub now: u64,
    /// The sync chip's words: "Synced".
    pub sync_text: String,
    pub balance: Balance,
    pub not_ready: Option<NotReady>,
    pub activity: Vec<ActivityDay>,
    pub contacts: Vec<ContactRow>,
    pub unsaved: Vec<PaidBefore>,
    pub coins: Coins,
    /// The payment the Payment scene opens, with its receipt.
    pub payment: Activity,
    pub receive_address: String,
}

/// Every scene's name.
#[uniffi::export]
pub fn scene_names() -> Vec<String> {
    fob_scenes::NAMES.iter().map(|n| n.to_string()).collect()
}

/// The demo wallet called `name`, or none for a name that isn't a scene.
#[uniffi::export]
pub fn scene(name: String) -> Option<SceneWallet> {
    fob_scenes::scene(&name).map(Into::into)
}

fn status(confirmations: u64, needed: u64) -> PaymentStatus {
    if confirmations < needed {
        PaymentStatus::NotReady {
            done: confirmations,
            needed,
        }
    } else {
        PaymentStatus::Confirmed { confirmations }
    }
}

fn progress(confirmations: u64, needed: u64) -> f32 {
    (confirmations as f32 / needed as f32).min(1.0)
}

impl From<fob_scenes::Scene> for SceneWallet {
    fn from(s: fob_scenes::Scene) -> Self {
        let w = s.wallet;
        let b = &w.balance;
        let receipt = Receipt {
            merchant: w.payment.merchant.into(),
            issued: w.payment.issued.into(),
            lines: w
                .payment
                .lines
                .iter()
                .map(|l| ReceiptLine {
                    label: l.label.into(),
                    amount: l.amount.into(),
                })
                .collect(),
            item_count: w.payment.lines.len() as u32,
            tax: None,
            total: w.payment.total.into(),
            currency: w.payment.currency.into(),
            other_currency: None,
            converted_label: None,
            converted: None,
            mismatch: None,
            no_price: None,
        };
        let payment = Activity {
            tx: w.payment.tx.into(),
            outgoing: true,
            amount_text: w.payment.amount_text.into(),
            receipt: Some(receipt),
            status: PaymentStatus::Confirmed { confirmations: 14 },
        };
        let segments: Vec<CoinSegment> = w
            .coins
            .ready
            .iter()
            .map(|c| CoinSegment {
                atomic: c.atomic,
                progress: None,
                coins: 1,
            })
            .chain(w.coins.not_ready.iter().map(|c| CoinSegment {
                atomic: c.atomic,
                progress: Some(progress(c.confirmations, c.needed)),
                coins: 1,
            }))
            .collect();
        let not_ready_progress = w
            .coins
            .not_ready
            .iter()
            .map(|c| progress(c.confirmations, c.needed) * c.atomic as f32)
            .sum::<f32>()
            / b.not_ready.max(1) as f32;
        let fiat_ready = w
            .coins
            .ready_sum
            .split(" · ")
            .nth(1)
            .unwrap_or_default()
            .to_string();
        let fiat_not_ready = w
            .coins
            .not_ready_sum
            .split(" · ")
            .nth(1)
            .unwrap_or_default()
            .to_string();
        SceneWallet {
            scene: s.name.into(),
            wallet_name: w.name.into(),
            network: match w.network {
                fob_scenes::Network::Mainnet => Network::Mainnet,
                fob_scenes::Network::Stagenet => Network::Stagenet,
            },
            now: w.now,
            sync_text: w.sync_text.into(),
            balance: Balance {
                total: b.total,
                unlocked: b.ready,
                unsynced: 0,
                total_text: b.total_text.into(),
                unlocked_text: b.ready_text.into(),
                unsynced_text: "0".into(),
                fiat_text: Some(b.fiat_text.into()),
                fiat_age: None,
                fiat_line: Some(b.fiat_text.into()),
                can_swap: true,
                fiat_first: false,
                ready_line: format!("{} XMR ready to spend · {fiat_ready}", b.ready_text),
            },
            not_ready: (b.not_ready > 0).then(|| NotReady {
                ready: b.ready,
                not_ready: b.not_ready,
                ready_xmr: format!("{} XMR", b.ready_text),
                incoming_xmr: format!("{} XMR", b.not_ready_text),
                ready_money: vec![fiat_ready.clone()],
                incoming_money: vec![fiat_not_ready.clone()],
                all_ready: b.all_ready.into(),
                description: format!(
                    "{} XMR ready to spend, {} XMR not yet, all ready about {}",
                    b.ready_text, b.not_ready_text, b.all_ready
                ),
                segments: segments
                    .iter()
                    .map(|s| CoinSegment {
                        atomic: s.atomic,
                        progress: s.progress,
                        coins: s.coins,
                    })
                    .collect(),
            }),
            activity: w
                .activity
                .iter()
                .map(|d| ActivityDay {
                    label: d.label.into(),
                    rows: d
                        .rows
                        .iter()
                        .map(|r| {
                            let (activity, request) = match r.kind {
                                fob_scenes::RowKind::Payment {
                                    tx,
                                    outgoing,
                                    atomic,
                                    confirmations,
                                    needed,
                                } => (
                                    Some(Activity {
                                        tx: tx.into(),
                                        outgoing,
                                        amount_text: crate::xmr(atomic),
                                        receipt: None,
                                        status: status(confirmations, needed),
                                    }),
                                    None,
                                ),
                                fob_scenes::RowKind::Request { id } => (None, Some(id)),
                            };
                            ActivityRow {
                                activity,
                                request,
                                share: None,
                                title: r.title.into(),
                                line: r.line.into(),
                                amount_text: r.amount_text.into(),
                                fiat_text: r.fiat_text.map(Into::into),
                            }
                        })
                        .collect(),
                })
                .collect(),
            contacts: w
                .contacts
                .iter()
                .map(|c| ContactRow {
                    id: c.id,
                    name: c.name.into(),
                    initial: c.initial.into(),
                    photo_key: None,
                    send_address: c.send_address.map(Into::into),
                    line: c.line.into(),
                })
                .collect(),
            unsaved: w
                .unsaved
                .iter()
                .map(|u| PaidBefore {
                    address: u.address.into(),
                    name: u.name.into(),
                    line: u.line.into(),
                })
                .collect(),
            coins: Coins {
                intro: w.coins.intro.into(),
                ready: b.ready,
                not_ready: b.not_ready,
                progress: not_ready_progress,
                bar_description: format!(
                    "{} XMR ready, {} XMR not yet, {}% of the way",
                    b.ready_text,
                    b.not_ready_text,
                    (not_ready_progress * 100.0).round() as u32
                ),
                ready_title: w.coins.ready_title.into(),
                ready_sum: w.coins.ready_sum.into(),
                ready_coins: w
                    .coins
                    .ready
                    .iter()
                    .map(|c| Coin {
                        amount_text: c.amount_text.into(),
                        fiat_text: Some(c.fiat_text.into()),
                        source: c.source.into(),
                    })
                    .collect(),
                not_ready_title: w.coins.not_ready_title.into(),
                not_ready_sum: w.coins.not_ready_sum.into(),
                not_ready_coins: w
                    .coins
                    .not_ready
                    .iter()
                    .map(|c| NotReadyPiece {
                        amount_text: c.amount_text.into(),
                        fiat_text: Some(c.fiat_text.into()),
                        confirmations: c.confirmations,
                        needed: c.needed,
                        progress: progress(c.confirmations, c.needed),
                        source: c.source.into(),
                        confirmations_text: format!("{} of {}", c.confirmations, c.needed),
                        ready: c.ready.into(),
                    })
                    .collect(),
                can_split: true,
                segments,
            },
            payment,
            receive_address: w.receive_address.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_scene_converts() {
        for name in scene_names() {
            let s = scene(name.clone()).unwrap();
            assert_eq!(s.scene, name);
            assert_eq!(s.wallet_name, "Spending");
            assert_eq!(s.network, Network::Stagenet);
        }
        assert!(scene("nope".into()).is_none());
    }

    #[test]
    fn every_board_scene_is_listed_and_deterministic() {
        let names = scene_names();
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
            assert!(names.iter().any(|n| n == name), "{name} is missing");
            let (a, b) = (scene(name.into()).unwrap(), scene(name.into()).unwrap());
            assert_eq!(a.now, b.now);
            assert_eq!(a.balance.total_text, b.balance.total_text);
            assert_eq!(a.activity.len(), b.activity.len());
            assert_eq!(a.receive_address, b.receive_address);
        }
    }

    #[test]
    fn home_shows_the_boards_balance() {
        let s = scene("home".into()).unwrap();
        assert_eq!(s.balance.total_text, "0.4182");
        assert_eq!(s.balance.fiat_text.as_deref(), Some("A$148.20"));
        assert_eq!(s.activity[0].label, "TODAY");
        assert_eq!(s.activity[0].rows[0].title, "To Corner Bakery");
        assert_eq!(s.contacts.len(), 4);
        assert_eq!(s.coins.ready_coins.len(), 3);
        assert_eq!(s.payment.receipt.as_ref().unwrap().total, "A$11.05");
    }
}
