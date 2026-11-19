//! Demo wallets for screenshots (`crates/fob-scenes`), as the records the
//! apps already draw: an app launched with `-FobScene <name>` draws its real
//! screens from [`scene`] with no node, no clock and no randomness.

use crate::{
    Activity, ActivityDay, ActivityRow, AmountEntry, AmountUnit, Balance, Coin, CoinSegment, Coins,
    ContactRow, FobError, Network, NotReady, NotReadyPiece, PaidBefore, PaymentStatus, Prepared,
    Receipt, ReceiptLine, ReceiveCode, ScanOption, SplitView, SyncStatus, TypedAmount,
};
use fob_wallet::{amount, catch_up, fiat, manual, price, split, uri};

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
    /// The sync chip and its card: synced, or catching up ("4 min left")
    /// for home-syncing and catchup.
    pub sync: SyncStatus,
    pub send: SceneSend,
    pub profile: SceneProfile,
    pub settings: SceneSettings,
    /// Back up your wallet: 16 fixed demo words, never a real wallet's.
    pub seed_words: Vec<String>,
    pub welcome: SceneWelcome,
    /// The Scan sheet.
    pub catch_up: SceneCatchUp,
    /// Split up as the board opens it: A$5 × 2, A$10 × 3, A$20 × 4.
    pub split: SplitView,
}

/// Send's screens, filled in.
#[derive(uniffi::Record)]
pub struct SceneSend {
    /// send-form: A$20.00 to Joel.
    pub form: SceneSendForm,
    /// confirm: paying Corner Bakery, with its receipt.
    pub confirm: SceneConfirm,
    /// confirm-joel: paying Joel, with a note.
    pub confirm_joel: SceneConfirm,
    /// send and pay-who: the contacts to pay, "Paid yesterday · Rent share".
    pub people: Vec<ContactRow>,
    /// send and pay-who: "Paid before".
    pub paid_before: Vec<PaidBefore>,
}

/// The send form as the boards draw it.
#[derive(uniffi::Record)]
pub struct SceneSendForm {
    /// The contact it pays.
    pub contact_id: u64,
    /// "Joel"
    pub to_name: String,
    pub to_address: String,
    /// "46Qm TxVk … 9bRe"
    pub to_short: String,
    /// What's typed: "20.00".
    pub amount_text: String,
    /// The amount field for it, as the real form reads it: "A$", "AUD",
    /// "≈ 0.0564 XMR · 1 XMR = A$354.60".
    pub amount: AmountEntry,
    /// Empty: "Note (optional)" shows.
    pub note: String,
}

/// A confirm screen: the payment as `prepare` gives it, and who it's to.
#[derive(uniffi::Record)]
pub struct SceneConfirm {
    /// "Corner Bakery", "Joel"
    pub name: String,
    /// A contact's letter; none for a shop.
    pub initial: Option<String>,
    /// "86Ux 3m9P … 7kQa"
    pub to_short: String,
    pub note: Option<String>,
    pub prepared: Prepared,
}

/// Settings' profile row: "Henry" and " · share as a code".
#[derive(uniffi::Record)]
pub struct SceneProfile {
    pub initial: String,
    pub name: String,
    /// "Your profile"
    pub title: String,
    /// " · share as a code"
    pub suffix: String,
}

/// Settings' values.
#[derive(uniffi::Record)]
pub struct SceneSettings {
    /// Wallets: "Spending".
    pub wallet: String,
    /// Backup and keys: "backed up".
    pub backup: String,
    /// Appearance: "OLED".
    pub appearance: String,
    /// Default currency: "AUD"; the Mac writes it "Australian dollar (AUD)".
    pub currency: String,
    pub currency_name: String,
    /// Price from: "CoinGecko".
    pub price_from: String,
    /// Sync through: "Node".
    pub sync_through: String,
    /// "node.monokulo.net"
    pub node: String,
    /// Scanning: "GPU · Apple A18".
    pub scanning: String,
    /// Mac: "After 15 minutes".
    pub lock_after: String,
    /// Mac: "Touch ID".
    pub unlock_with: String,
}

/// Welcome, the first run.
#[derive(uniffi::Record)]
pub struct SceneWelcome {
    pub network: Network,
    /// "On stagenet, with test coins"
    pub network_line: String,
}

/// The Scan sheet (PhoneCatchUp).
#[derive(uniffi::Record)]
pub struct SceneCatchUp {
    /// "Quick", "Dates", "Block"; the first is shown.
    pub modes: Vec<String>,
    /// From last scan, Today, Past week, Past month, The whole wallet: their
    /// words from fob-words, with the board's costs ("about 1 min").
    pub options: Vec<ScanOption>,
    /// The one ticked: Past week.
    pub selected: u32,
    /// "Scan the past week"
    pub action: String,
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

/// One of each sync state (synced, catching up, offline), for the sync
/// chip's gallery.
#[uniffi::export]
pub fn scene_sync_states() -> Vec<SyncStatus> {
    fob_scenes::sync_states()
        .into_iter()
        .map(sync_status)
        .collect()
}

/// The amount field as scene `name`'s wallet reads `amount`: A$ at the
/// scene's fixed price, never aged, as `FobWallet::amount_entry` does with a
/// price. None for a name that isn't a scene.
#[uniffi::export]
pub fn scene_amount_entry(name: String, amount: TypedAmount) -> Option<AmountEntry> {
    let w = fob_scenes::scene(&name)?.wallet;
    let aud = fiat::currency(w.currency);
    Some(price::amount_field(amount.typed(), price::FieldPrice::At(&aud, w.price, None)).into())
}

/// An amount field's text switched to `to` at scene `name`'s price, as
/// `FobWallet::convert_amount`: blank for blank, or what isn't an amount.
#[uniffi::export]
pub fn scene_convert_amount(name: String, text: String, to: AmountUnit) -> String {
    let Some(w) = fob_scenes::scene(&name).map(|s| s.wallet) else {
        return String::new();
    };
    let aud = fiat::currency(w.currency);
    match to {
        AmountUnit::Fiat => amount::parse(&text)
            .map(|a| fiat::plain(fiat::to_minor(a, w.price, &aud), &aud))
            .unwrap_or_default(),
        AmountUnit::Xmr => fiat::parse(&text, &aud)
            .ok()
            .and_then(|m| fiat::to_atomic(m, w.price, &aud))
            .map(amount::format)
            .unwrap_or_default(),
    }
}

/// Receive's payment code for scene `name`'s receive address, as
/// `FobWallet::receive_code` makes it: the amount (XMR, or A$ converted at
/// the scene's price) and a note for the payer. None for a name that isn't
/// a scene; throws when the amount isn't one.
#[uniffi::export]
pub fn scene_receive_code(
    name: String,
    amount: Option<TypedAmount>,
    note: Option<String>,
) -> Result<Option<ReceiveCode>, FobError> {
    let Some(w) = fob_scenes::scene(&name).map(|s| s.wallet) else {
        return Ok(None);
    };
    let aud = fiat::currency(w.currency);
    let typed = amount.as_ref().map(TypedAmount::typed);
    let (xmr, money) = match typed.and_then(amount::Typed::trimmed) {
        None => (None, None),
        Some(amount::Typed::Xmr(text)) => (Some(amount::parse(text.trim())?), None),
        Some(amount::Typed::Fiat(text)) => {
            let minor = fiat::parse(text.trim(), &aud)?;
            let atomic =
                fiat::to_atomic(minor, w.price, &aud).ok_or_else(|| FobError::Invalid {
                    reason: "That isn't an amount".into(),
                })?;
            (Some(atomic), Some(fiat::format(minor, &aud)))
        }
    };
    let xmr = xmr.filter(|a| *a > 0);
    let xmr_text = xmr.map(amount::format);
    let code = uri::receive(w.receive_address, xmr_text.as_deref(), note.as_deref())?;
    Ok(Some(match (money.filter(|_| xmr.is_some()), xmr_text) {
        (Some(money), Some(xmr)) => ReceiveCode {
            uri: code.uri,
            headline: Some(money),
            detail: Some(
                [Some(format!("{xmr} XMR")), code.detail]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join(" · "),
            ),
        },
        _ => ReceiveCode {
            uri: code.uri,
            headline: code.headline,
            detail: code.detail,
        },
    }))
}

fn sync_status(state: fob_scenes::SyncState) -> SyncStatus {
    let status = match state {
        fob_scenes::SyncState::Synced { height } => catch_up::SyncStatus {
            scanned: height,
            tip: height,
            start: fob_scenes::FIRST_BLOCK,
            synced: true,
            connected: true,
            progress: 1.0,
            ..Default::default()
        },
        fob_scenes::SyncState::CatchingUp {
            scanned,
            tip,
            progress,
            seconds_left,
            blocks_per_second,
        } => catch_up::SyncStatus {
            scanned,
            tip,
            start: fob_scenes::FIRST_BLOCK,
            connected: true,
            progress,
            seconds_left: Some(seconds_left),
            blocks_per_second: Some(blocks_per_second),
            ..Default::default()
        },
        fob_scenes::SyncState::Offline { scanned, tip } => catch_up::SyncStatus {
            scanned,
            tip,
            start: fob_scenes::FIRST_BLOCK,
            progress: (scanned - fob_scenes::FIRST_BLOCK) as f32
                / (tip - fob_scenes::FIRST_BLOCK) as f32,
            error: Some("Connection refused".into()),
            ..Default::default()
        },
    };
    status.into()
}

fn confirm(c: &fob_scenes::Confirm) -> SceneConfirm {
    let receipt = c.receipt.as_ref().map(|r| Receipt {
        merchant: r.merchant.into(),
        issued: r.issued.into(),
        lines: r
            .lines
            .iter()
            .map(|l| ReceiptLine {
                label: l.label.into(),
                amount: l.amount.into(),
            })
            .collect(),
        item_count: r.lines.len() as u32,
        tax: r.tax.map(Into::into),
        total: r.total.into(),
        currency: r.currency.into(),
        other_currency: None,
        converted_label: None,
        converted: None,
        mismatch: None,
        no_price: None,
    });
    SceneConfirm {
        name: c.to_name.into(),
        initial: c.initial.map(Into::into),
        to_short: c.to_short.into(),
        note: c.note.map(Into::into),
        prepared: Prepared {
            id: 0,
            to: c.to_address.into(),
            amount_text: crate::exact(c.atomic),
            fee_text: amount::format(c.fee),
            total_text: amount::format(c.atomic + c.fee),
            amount_fiat: Some(c.amount_fiat.into()),
            fee_fiat: Some(c.fee_fiat.into()),
            total_fiat: None,
            fiat_age: Some(c.price_age.into()),
            receipt,
            split: None,
        },
    }
}

fn send(w: &fob_scenes::Wallet) -> SceneSend {
    let s = &w.send;
    let aud = fiat::currency(w.currency);
    let field = price::amount_field(
        amount::Typed::Fiat(s.form.amount_text),
        price::FieldPrice::At(&aud, w.price, None),
    );
    let contact = |id: u64| w.contacts.iter().find(|c| c.id == id);
    SceneSend {
        form: SceneSendForm {
            contact_id: s.form.contact_id,
            to_name: s.form.to_name.into(),
            to_address: s.form.to_address.into(),
            to_short: s.form.to_short.into(),
            amount_text: s.form.amount_text.into(),
            amount: field.into(),
            note: s.form.note.into(),
        },
        confirm: confirm(&s.confirm),
        confirm_joel: confirm(&s.confirm_joel),
        people: s
            .people
            .iter()
            .map(|p| ContactRow {
                id: p.contact_id,
                name: p.name.into(),
                initial: p.initial.into(),
                photo_key: None,
                send_address: contact(p.contact_id)
                    .and_then(|c| c.send_address)
                    .map(Into::into),
                line: p.line.into(),
            })
            .collect(),
        paid_before: s
            .paid_before
            .iter()
            .map(|u| PaidBefore {
                address: u.address.into(),
                name: u.name.into(),
                line: u.line.into(),
            })
            .collect(),
    }
}

fn catch_up_sheet(w: &fob_scenes::Wallet) -> SceneCatchUp {
    let c = &w.catch_up;
    let settings = manual::ManualSettings {
        on: false,
        pull_mode: manual::PullMode::Auto,
        incoming_scan_blocks: 5,
        settled_confirmations: 10,
    };
    let tip = fob_scenes::TIP;
    let options = manual::scan_options(settings, 0, tip - c.behind, fob_scenes::FIRST_BLOCK, tip)
        .into_iter()
        .filter(|o| o.label != manual::ScanLabel::Unsettled)
        .zip(c.costs)
        .map(|(o, cost)| {
            let said = fob_wallet::words::manual::option(&o);
            ScanOption {
                request: o.request.into(),
                label: said.label,
                note: said.note,
                cost: cost.into(),
                action: said.action,
            }
        })
        .collect();
    SceneCatchUp {
        modes: c.modes.iter().map(|m| m.to_string()).collect(),
        options,
        selected: c.selected as u32,
        action: c.action.into(),
    }
}

fn split_view(w: &fob_scenes::Wallet) -> SplitView {
    split_view_of(w, &w.split.counts)
}

/// Split up for scene wallet `w` with `counts` of each note, as
/// `FobWallet::split_view` reads it: its notes at the scene's price, its
/// ready coins to spend, Auto learned from its payments, a fresh price.
fn split_view_of(w: &fob_scenes::Wallet, counts: &[u32]) -> SplitView {
    let aud = fiat::currency(w.currency);
    let money = Some((&aud, w.price));
    let notes = split::notes(money);
    split::view(
        &notes,
        counts,
        &w.split.payments,
        &split_budget(w),
        money,
        None,
    )
    .into()
}

fn split_budget(w: &fob_scenes::Wallet) -> split::Budget {
    split::Budget {
        outputs: w.coins.ready.iter().map(|c| c.atomic).collect(),
        fee_rate: None,
    }
}

fn split_notes(w: &fob_scenes::Wallet) -> Vec<split::Note> {
    let aud = fiat::currency(w.currency);
    split::notes(Some((&aud, w.price)))
}

/// Split up for scene `name` with `counts` of each note (in the order of
/// `SplitView::tiles`), as the live wallet's `split_view` gives it. None for
/// a name that isn't a scene.
#[uniffi::export]
pub fn scene_split_view(name: String, counts: Vec<u32>) -> Option<SplitView> {
    let w = fob_scenes::scene(&name)?.wallet;
    Some(split_view_of(&w, &counts))
}

/// One more piece of note `index` in scene `name`, as `split_add`: throws
/// why not (15 pieces at most, or not enough ready to spend with the fee).
/// Unchanged counts for a name that isn't a scene.
#[uniffi::export]
pub fn scene_split_add(name: String, counts: Vec<u32>, index: u32) -> Result<Vec<u32>, FobError> {
    let Some(w) = fob_scenes::scene(&name).map(|s| s.wallet) else {
        return Ok(counts);
    };
    Ok(split::add(
        &split_notes(&w),
        &counts,
        index as usize,
        &split_budget(&w),
    )?)
}

/// One fewer piece of note `index` in scene `name`, as `split_remove`.
#[uniffi::export]
pub fn scene_split_remove(name: String, counts: Vec<u32>, index: u32) -> Vec<u32> {
    match fob_scenes::scene(&name).map(|s| s.wallet) {
        Some(w) => split::remove(&split_notes(&w), &counts, index as usize),
        None => counts,
    }
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
        let profile_row =
            fob_wallet::words::profile::settings_row(fob_wallet::profile::SettingsRow {
                name: Some(w.profile.name.into()),
            });
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
            sync: sync_status(w.sync),
            send: send(&w),
            profile: SceneProfile {
                initial: w.profile.initial.into(),
                name: profile_row.name.unwrap_or_default(),
                title: profile_row.title,
                suffix: profile_row.suffix.unwrap_or_default(),
            },
            settings: SceneSettings {
                wallet: w.settings.wallet.into(),
                backup: w.settings.backup.into(),
                appearance: w.settings.appearance.into(),
                currency: w.settings.currency.into(),
                currency_name: fiat::currency(w.settings.currency).text(),
                price_from: w.settings.price_from.into(),
                sync_through: w.settings.sync_through.into(),
                node: w.settings.node.into(),
                scanning: w.settings.scanning.into(),
                lock_after: w.settings.lock_after.into(),
                unlock_with: w.settings.unlock_with.into(),
            },
            seed_words: w.seed_words.iter().map(|w| w.to_string()).collect(),
            welcome: SceneWelcome {
                network: match w.welcome.network {
                    fob_scenes::Network::Mainnet => Network::Mainnet,
                    fob_scenes::Network::Stagenet => Network::Stagenet,
                },
                network_line: w.welcome.network_line.into(),
            },
            catch_up: catch_up_sheet(&w),
            split: split_view(&w),
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

    #[test]
    fn send_is_the_boards() {
        let s = scene("send-form".into()).unwrap().send;
        let f = &s.form;
        assert_eq!(
            (
                f.to_name.as_str(),
                f.to_short.as_str(),
                f.amount_text.as_str()
            ),
            ("Joel", "46Qm TxVk … 9bRe", "20.00")
        );
        assert_eq!(f.amount.sign.as_deref(), Some("A$"));
        assert_eq!(f.amount.unit, "AUD");
        assert_eq!(f.amount.conversion.as_deref(), Some("≈ 0.0564 XMR"));
        assert_eq!(
            f.amount.line.as_deref(),
            Some("≈ 0.0564 XMR · 1 XMR = A$354.60")
        );
        assert!(f.amount.sendable);
        assert_eq!(f.note, "");

        let c = &s.confirm;
        assert_eq!(
            (c.name.as_str(), c.initial.as_deref(), c.to_short.as_str()),
            ("Corner Bakery", None, "86Ux 3m9P … 7kQa")
        );
        let p = &c.prepared;
        assert_eq!(
            (
                p.amount_text.as_str(),
                p.fee_text.as_str(),
                p.total_text.as_str()
            ),
            ("0.0312", "0.00006", "0.03126")
        );
        assert_eq!(
            (
                p.amount_fiat.as_deref(),
                p.fee_fiat.as_deref(),
                p.fiat_age.as_deref()
            ),
            (Some("A$11.05"), Some("A$0.02"), Some("2 min old"))
        );
        let r = p.receipt.as_ref().unwrap();
        assert_eq!(
            (r.item_count, r.tax.as_deref(), r.total.as_str()),
            (2, Some("A$1.00"), "A$11.05")
        );
        assert_eq!(
            (r.lines[0].label.as_str(), r.lines[1].amount.as_str()),
            ("Flat white", "A$5.55")
        );

        let j = &s.confirm_joel;
        assert_eq!(
            (j.name.as_str(), j.initial.as_deref(), j.note.as_deref()),
            ("Joel", Some("J"), Some("Dinner, Friday"))
        );
        assert_eq!(
            (
                j.prepared.amount_text.as_str(),
                j.prepared.total_text.as_str()
            ),
            ("0.0564", "0.05646")
        );
        assert_eq!(j.prepared.amount_fiat.as_deref(), Some("A$20.00"));
        assert!(j.prepared.receipt.is_none());

        let people: Vec<_> = s
            .people
            .iter()
            .map(|p| (p.name.as_str(), p.line.as_str()))
            .collect();
        assert_eq!(
            people,
            [
                ("Joel", "Paid yesterday · Rent share"),
                ("Mara Okafor", "Paid 3 Nov · Concert tickets"),
                ("Sam Whitlock", "Not paid yet")
            ]
        );
        assert!(s.people.iter().all(|p| p.send_address.is_some()));
        assert_eq!(
            s.people[0].send_address.as_deref(),
            Some(f.to_address.as_str())
        );
        assert_eq!(
            (
                s.paid_before[0].name.as_str(),
                s.paid_before[0].line.as_str()
            ),
            ("Corner Bakery", "86Ux 3m9P … 7kQa · today")
        );
    }

    #[test]
    fn settings_profile_welcome_backup_and_scan() {
        let s = scene("settings".into()).unwrap();
        assert_eq!(
            (s.profile.initial.as_str(), s.profile.name.as_str()),
            ("H", "Henry")
        );
        assert_eq!(
            format!("{}{}", s.profile.title, s.profile.suffix),
            "Your profile · share as a code"
        );
        let t = &s.settings;
        assert_eq!(
            [
                &t.wallet,
                &t.backup,
                &t.appearance,
                &t.currency,
                &t.price_from,
                &t.sync_through,
                &t.node,
                &t.scanning,
                &t.lock_after,
                &t.unlock_with
            ]
            .map(|s| s.as_str()),
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
        assert_eq!(
            format!("{} ({})", t.currency_name, t.currency),
            "Australian dollar (AUD)"
        );
        assert_eq!(s.seed_words.len(), 16);
        assert_eq!(s.seed_words[0], "timber");
        assert_eq!(
            (s.welcome.network, s.welcome.network_line.as_str()),
            (Network::Stagenet, "On stagenet, with test coins")
        );

        let c = scene("catchup".into()).unwrap();
        assert_eq!(c.sync.chip_text, "4 min left");
        let sheet = &c.catch_up;
        assert_eq!(sheet.modes, ["Quick", "Dates", "Block"]);
        let options: Vec<_> = sheet
            .options
            .iter()
            .map(|o| (o.label.as_str(), o.cost.as_str()))
            .collect();
        assert_eq!(
            options,
            [
                ("From last scan", "seconds"),
                ("Today", "seconds"),
                ("Past week", "about 1 min"),
                ("Past month", "about 4 min"),
                ("The whole wallet", "about 20 min")
            ]
        );
        assert_eq!(
            (sheet.selected, sheet.action.as_str()),
            (2, "Scan the past week")
        );
        assert!(matches!(
            sheet.options[2].request,
            crate::ScanRequest::Back { blocks: 5040 }
        ));
    }

    #[test]
    fn sync_chip_words_come_from_fob_words() {
        for name in scene_names() {
            let s = scene(name.clone()).unwrap();
            assert_eq!(s.sync.chip_text, s.sync_text, "{name}");
        }
        let home = scene("home-syncing".into()).unwrap();
        assert_eq!(home.sync.chip_look, crate::ChipLook::Ring);
        assert_eq!(home.sync.chip_text, "4 min left");
        assert_eq!(home.balance.total_text, "0.4182");
        let states = scene_sync_states();
        let looks: Vec<_> = states
            .iter()
            .map(|s| (s.chip_look, s.chip_text.as_str()))
            .collect();
        assert_eq!(
            looks,
            [
                (crate::ChipLook::Synced, "Synced"),
                (crate::ChipLook::Ring, "4 min left"),
                (crate::ChipLook::Offline, "Can't reach node")
            ]
        );
    }

    #[test]
    fn split_taps_go_through_split_add() {
        let name = "split".to_string();
        let opened = scene(name.clone()).unwrap().split;
        // A$120 of A$137 is in coins: no room for another A$20.
        assert!(scene_split_add(name.clone(), opened.counts.clone(), 2).is_err());
        let cleared = scene_split_view(name.clone(), vec![0; 6]).unwrap();
        assert_eq!(cleared.pieces, 0);
        assert!(!cleared.ready);
        let once = scene_split_add(name.clone(), cleared.counts.clone(), 2).unwrap();
        let twice = scene_split_add(name.clone(), once, 2).unwrap();
        assert_eq!(twice, [0, 0, 2, 0, 0, 0]);
        let v = scene_split_view(name.clone(), twice.clone()).unwrap();
        assert_eq!(v.summary.as_deref(), Some("2 coins · A$40.00"));
        assert_eq!(v.button, "Split into 2 coins");
        assert!(v.ready);
        let one = scene_split_remove(name.clone(), twice, 2);
        assert_eq!(one, [0, 0, 1, 0, 0, 0]);
        let v = scene_split_view(name.clone(), one).unwrap();
        assert_eq!(v.summary.as_deref(), Some("1 coin · A$20.00"));
        if let Some(auto) = &opened.auto {
            let v = scene_split_view(name.clone(), auto.counts.clone()).unwrap();
            assert!(v.is_auto);
        }
        let same = scene_split_view(name.clone(), opened.counts.clone()).unwrap();
        assert_eq!(same.summary, opened.summary);
        assert!(scene_split_view("nope".into(), vec![]).is_none());
    }

    #[test]
    fn split_is_computed_by_split_view() {
        let v = scene("split".into()).unwrap().split;
        let tiles: Vec<_> = v
            .tiles
            .iter()
            .map(|t| (t.label.as_str(), t.count_text.as_str()))
            .collect();
        assert_eq!(
            tiles,
            [
                ("A$5", "× 2"),
                ("A$10", "× 3"),
                ("A$20", "× 4"),
                ("A$50", "tap to add"),
                ("A$100", "tap to add"),
                ("A$200", "tap to add")
            ]
        );
        assert_eq!(v.summary.as_deref(), Some("9 coins · A$120.00"));
        assert_eq!(v.button, "Split into 9 coins");
        assert!(v.ready);
        assert!(
            v.left_over
                .as_deref()
                .is_some_and(|l| l.starts_with("+ A$17.") && l.ends_with(" left as one coin")),
            "{:?}",
            v.left_over
        );
        assert!(
            v.footer.starts_with("Network fee about A$0.0"),
            "{}",
            v.footer
        );
        let auto = v.auto.as_ref().unwrap();
        assert_eq!(auto.label, "Auto");
        assert_eq!(auto.spoken, "Auto, fill coins like your last 30 days");
        assert_eq!(v.bar.len(), 10);
        assert!(v.bar.last().unwrap().left_over);
    }

    #[test]
    fn home_has_yesterdays_profile_payment() {
        let s = scene("home".into()).unwrap();
        let row = &s.activity[1].rows[1];
        assert_eq!(
            (
                row.title.as_str(),
                row.line.as_str(),
                row.amount_text.as_str()
            ),
            ("Via your profile", "14:02", "+0.2000")
        );
        assert_eq!(row.fiat_text.as_deref(), Some("A$70.86"));
        assert!(!row.activity.as_ref().unwrap().outgoing);
    }

    #[test]
    fn receive_is_the_boards() {
        let s = scene("receive".into()).unwrap();
        let groups: Vec<_> = crate::address_groups(s.receive_address.clone())
            .into_iter()
            .map(|g| g.text)
            .collect();
        assert_eq!(groups.len(), 19);
        assert_eq!(groups[..3], ["5AqHt", "JB65J", "3Lu7M"]);
        assert_eq!(groups[18], "cUDet");

        let blank =
            scene_amount_entry("receive".into(), TypedAmount::Fiat { text: "".into() }).unwrap();
        assert_eq!(
            (blank.sign.as_deref(), blank.unit.as_str()),
            (Some("A$"), "AUD")
        );
        assert!(blank.can_switch && !blank.sendable);
        let twenty = scene_amount_entry(
            "receive".into(),
            TypedAmount::Fiat {
                text: "20.00".into(),
            },
        )
        .unwrap();
        assert_eq!(
            twenty.line.as_deref(),
            Some("≈ 0.0564 XMR · 1 XMR = A$354.60")
        );
        assert!(scene_amount_entry("nope".into(), TypedAmount::Xmr { text: "1".into() }).is_none());
        assert_eq!(
            scene_convert_amount("receive".into(), "20.00".into(), AmountUnit::Xmr),
            "0.0564"
        );
        assert_eq!(
            scene_convert_amount("receive".into(), "1".into(), AmountUnit::Fiat),
            "354.60"
        );

        let plain = scene_receive_code("receive".into(), None, None)
            .unwrap()
            .unwrap();
        assert_eq!(plain.uri, format!("monero:{}", s.receive_address));
        assert_eq!((plain.headline, plain.detail), (None, None));
        let asked = scene_receive_code(
            "receive".into(),
            Some(TypedAmount::Fiat {
                text: "20.00".into(),
            }),
            Some("Joel".into()),
        )
        .unwrap()
        .unwrap();
        assert!(
            asked
                .uri
                .starts_with(&format!("monero:{}?tx_amount=0.0564", s.receive_address)),
            "{}",
            asked.uri
        );
        assert!(asked.uri.contains("Joel"), "{}", asked.uri);
        assert_eq!(asked.headline.as_deref(), Some("A$20.00"));
        assert!(
            asked
                .detail
                .as_deref()
                .is_some_and(|d| d.starts_with("0.0564") && d.ends_with(" XMR · Joel")),
            "{:?}",
            asked.detail
        );
        assert!(
            scene_receive_code(
                "receive".into(),
                Some(TypedAmount::Xmr { text: "abc".into() }),
                None
            )
            .is_err()
        );
        assert!(
            scene_receive_code("nope".into(), None, None)
                .unwrap()
                .is_none()
        );
    }
}
