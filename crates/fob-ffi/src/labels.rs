//! The words a screen shows around the wallet's own (tabs, titles, buttons,
//! headings), from fob-words, so the apps write none of them.

use fob_wallet::words::labels::{self as l, Biometry as B};

/// How the device checks it's you, for the buttons that pay.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum Biometry {
    FaceId,
    TouchId,
    OpticId,
}

impl From<Biometry> for B {
    fn from(b: Biometry) -> Self {
        match b {
            Biometry::FaceId => B::FaceId,
            Biometry::TouchId => B::TouchId,
            Biometry::OpticId => B::OpticId,
        }
    }
}

/// Every screen's own words.
#[derive(uniffi::Record)]
pub struct Labels {
    pub tabs: TabLabels,
    pub chrome: ChromeLabels,
    pub home: HomeLabels,
    pub receive: ReceiveLabels,
    pub send: SendLabels,
    pub confirm: ConfirmLabels,
    pub payment: PaymentLabels,
    pub contacts: ContactsLabels,
    pub settings: SettingsLabels,
    pub welcome: WelcomeLabels,
    pub backup: BackupLabels,
    pub coins: CoinsLabels,
    pub scan: ScanLabels,
}

/// Wallet, Contacts, Settings.
#[derive(uniffi::Record)]
pub struct TabLabels {
    pub wallet: String,
    pub contacts: String,
    pub settings: String,
}

/// The toolbar's icon buttons as VoiceOver names them, and the wallet
/// switcher's.
#[derive(uniffi::Record)]
pub struct ChromeLabels {
    pub scan_a_payment_code: String,
    pub switch_wallet: String,
    /// The tab bar, read as one group.
    pub sections: String,
    pub contacts: String,
    pub refresh: String,
    pub settings: String,
    /// The Mac's wallet menu.
    pub wallets: String,
}

#[derive(uniffi::Record)]
pub struct HomeLabels {
    pub receive: String,
    pub send: String,
    pub activity: String,
    pub coins: String,
    /// "ready", after the amount.
    pub ready: String,
    /// "not yet", after the amount.
    pub not_yet: String,
    /// "ready to spend", after the amount where there's room.
    pub ready_to_spend: String,
    /// "asked", under a request's amount.
    pub asked: String,
    /// "Stagenet", the chip beside the wallet's name.
    pub stagenet: String,
}

#[derive(uniffi::Record)]
pub struct ReceiveLabels {
    pub title: String,
    /// "Who is it for?"
    pub who_is_it_for: String,
}

#[derive(uniffi::Record)]
pub struct SendLabels {
    pub title: String,
    /// "Pay who?"
    pub pay_who: String,
    /// "Point at a payment code", over the camera.
    pub point_at_code: String,
    pub scan_a_code: String,
    pub paste_an_address: String,
    pub contacts: String,
    pub paid_before: String,
    pub to: String,
    pub paste: String,
    pub amount: String,
    /// "Note (optional)"
    pub note: String,
    pub review: String,
    pub cancel: String,
}

#[derive(uniffi::Record)]
pub struct ConfirmLabels {
    /// "Confirm"; paying someone, the title is [`label_pay`]'s.
    pub title: String,
    pub to: String,
    pub note: String,
    pub add_a_note: String,
    pub network_fee: String,
    pub with_the_fee: String,
    pub includes_tax: String,
    pub total: String,
    /// "Pay with Face ID", a phone's button.
    pub pay_with: String,
    /// "Send with Touch ID", a computer's button.
    pub send_with: String,
    pub use_passphrase: String,
}

#[derive(uniffi::Record)]
pub struct PaymentLabels {
    pub worth_now: String,
    pub status: String,
    pub network_fee: String,
    pub transaction: String,
    pub copy: String,
    pub receipt: String,
    pub total: String,
    pub paid_in_monero: String,
}

#[derive(uniffi::Record)]
pub struct ContactsLabels {
    pub title: String,
    /// "Paid, not saved"
    pub paid_not_saved: String,
    pub save: String,
}

#[derive(uniffi::Record)]
pub struct SettingsLabels {
    pub title: String,
    /// The section "Wallet".
    pub wallet: String,
    pub wallets: String,
    pub backup_and_keys: String,
    pub display_and_money: String,
    pub appearance: String,
    pub default_currency: String,
    pub price_from: String,
    pub syncing: String,
    pub sync_through: String,
    pub server: String,
    pub node: String,
    pub scanning: String,
    pub manual_sync: String,
    pub rescan: String,
    /// A computer's Settings window: "General", "Money", "This Mac".
    pub general: String,
    pub money: String,
    pub this_device: String,
    /// "Same as Mac", "Light", "Dark", "OLED".
    pub same_as_device: String,
    pub light: String,
    pub dark: String,
    pub oled: String,
    /// "OLED draws on true black, …"
    pub oled_line: String,
    pub show_amounts_first_in: String,
    pub lock_when_left_alone: String,
    pub unlock_with: String,
    pub passphrase_always_works: String,
    pub change_passphrase: String,
    pub lock_now: String,
}

#[derive(uniffi::Record)]
pub struct WelcomeLabels {
    /// "Fob"
    pub title: String,
    pub tagline: String,
    /// Ready in seconds, Keeps your receipts, Yours alone.
    pub points: Vec<WelcomePoint>,
    pub use_mainnet: String,
    /// "Create a wallet"
    pub create: String,
    /// "Restore a wallet"
    pub restore: String,
}

#[derive(uniffi::Record)]
pub struct WelcomePoint {
    pub title: String,
    pub line: String,
}

#[derive(uniffi::Record)]
pub struct BackupLabels {
    pub title: String,
    /// "I've written them down"
    pub written_down: String,
    pub back: String,
}

#[derive(uniffi::Record)]
pub struct CoinsLabels {
    pub title: String,
    pub split_up: String,
    pub clear: String,
}

#[derive(uniffi::Record)]
pub struct ScanLabels {
    pub title: String,
}

/// Every screen's own words, on `device` ("iPhone", "iPad", "Mac"), which
/// checks it's you with `biometry`.
#[uniffi::export]
pub fn labels(device: String, biometry: Biometry) -> Labels {
    let s = |text: &str| text.to_owned();
    Labels {
        tabs: TabLabels {
            wallet: s(l::tabs::WALLET),
            contacts: s(l::tabs::CONTACTS),
            settings: s(l::tabs::SETTINGS),
        },
        chrome: ChromeLabels {
            scan_a_payment_code: s(l::chrome::SCAN_A_PAYMENT_CODE),
            switch_wallet: s(l::chrome::SWITCH_WALLET),
            sections: s(l::chrome::SECTIONS),
            contacts: s(l::chrome::CONTACTS),
            refresh: s(l::chrome::REFRESH),
            settings: s(l::chrome::SETTINGS),
            wallets: s(l::chrome::WALLETS),
        },
        home: HomeLabels {
            receive: s(l::home::RECEIVE),
            send: s(l::home::SEND),
            activity: s(l::home::ACTIVITY),
            coins: s(l::home::COINS),
            ready: s(l::home::READY),
            not_yet: s(l::home::NOT_YET),
            ready_to_spend: s(l::home::READY_TO_SPEND),
            asked: s(l::home::ASKED),
            stagenet: s(l::home::STAGENET),
        },
        receive: ReceiveLabels {
            title: s(l::receive::TITLE),
            who_is_it_for: s(l::receive::WHO_IS_IT_FOR),
        },
        send: SendLabels {
            title: s(l::send::TITLE),
            pay_who: s(l::send::PAY_WHO),
            point_at_code: s(l::send::POINT_AT_CODE),
            scan_a_code: s(l::send::SCAN_A_CODE),
            paste_an_address: s(l::send::PASTE_AN_ADDRESS),
            contacts: s(l::send::CONTACTS),
            paid_before: s(l::send::PAID_BEFORE),
            to: s(l::send::TO),
            paste: s(l::send::PASTE),
            amount: s(l::send::AMOUNT),
            note: s(l::send::NOTE),
            review: s(l::send::REVIEW),
            cancel: s(l::send::CANCEL),
        },
        confirm: ConfirmLabels {
            title: s(l::confirm::TITLE),
            to: s(l::confirm::TO),
            note: s(l::confirm::NOTE),
            add_a_note: s(l::confirm::ADD_A_NOTE),
            network_fee: s(l::confirm::NETWORK_FEE),
            with_the_fee: s(l::confirm::WITH_THE_FEE),
            includes_tax: s(l::confirm::INCLUDES_TAX),
            total: s(l::confirm::TOTAL),
            pay_with: l::confirm::pay_with(biometry.into()),
            send_with: l::confirm::send_with(biometry.into()),
            use_passphrase: s(l::confirm::USE_PASSPHRASE),
        },
        payment: PaymentLabels {
            worth_now: s(l::payment::WORTH_NOW),
            status: s(l::payment::STATUS),
            network_fee: s(l::payment::NETWORK_FEE),
            transaction: s(l::payment::TRANSACTION),
            copy: s(l::payment::COPY),
            receipt: s(l::payment::RECEIPT),
            total: s(l::payment::TOTAL),
            paid_in_monero: s(l::payment::PAID_IN_MONERO),
        },
        contacts: ContactsLabels {
            title: s(l::contacts::TITLE),
            paid_not_saved: s(l::contacts::PAID_NOT_SAVED),
            save: s(l::contacts::SAVE),
        },
        settings: SettingsLabels {
            title: s(l::settings::TITLE),
            wallet: s(l::settings::WALLET),
            wallets: s(l::settings::WALLETS),
            backup_and_keys: s(l::settings::BACKUP_AND_KEYS),
            display_and_money: s(l::settings::DISPLAY_AND_MONEY),
            appearance: s(l::settings::APPEARANCE),
            default_currency: s(l::settings::DEFAULT_CURRENCY),
            price_from: s(l::settings::PRICE_FROM),
            syncing: s(l::settings::SYNCING),
            sync_through: s(l::settings::SYNC_THROUGH),
            server: s(l::settings::SERVER),
            node: s(l::settings::NODE),
            scanning: s(l::settings::SCANNING),
            manual_sync: s(l::settings::MANUAL_SYNC),
            rescan: s(l::settings::RESCAN),
            general: s(l::settings::GENERAL),
            money: s(l::settings::MONEY),
            this_device: l::settings::this_device(&device),
            same_as_device: l::settings::same_as_device(&device),
            light: s(l::settings::LIGHT),
            dark: s(l::settings::DARK),
            oled: s(l::settings::OLED),
            oled_line: s(l::settings::OLED_LINE),
            show_amounts_first_in: s(l::settings::SHOW_AMOUNTS_FIRST_IN),
            lock_when_left_alone: s(l::settings::LOCK_WHEN_LEFT_ALONE),
            unlock_with: s(l::settings::UNLOCK_WITH),
            passphrase_always_works: s(l::settings::PASSPHRASE_ALWAYS_WORKS),
            change_passphrase: s(l::settings::CHANGE_PASSPHRASE),
            lock_now: s(l::settings::LOCK_NOW),
        },
        welcome: WelcomeLabels {
            title: s(l::welcome::TITLE),
            tagline: s(l::welcome::TAGLINE),
            points: l::welcome::points(&device)
                .into_iter()
                .map(|(title, line)| WelcomePoint {
                    title: s(title),
                    line,
                })
                .collect(),
            use_mainnet: s(l::welcome::USE_MAINNET),
            create: s(l::welcome::CREATE),
            restore: s(l::welcome::RESTORE),
        },
        backup: BackupLabels {
            title: s(l::backup::TITLE),
            written_down: s(l::backup::WRITTEN_DOWN),
            back: s(l::backup::BACK),
        },
        coins: CoinsLabels {
            title: s(l::coins::TITLE),
            split_up: s(l::coins::SPLIT_UP),
            clear: s(l::coins::CLEAR),
        },
        scan: ScanLabels {
            title: s(l::scan::TITLE),
        },
    }
}

/// Confirm's title paying someone: "Pay Corner Bakery".
#[uniffi::export]
pub fn label_pay(name: String) -> String {
    l::confirm::pay(&name)
}

/// The receipt's heading on Confirm: "Receipt · 2 items".
#[uniffi::export]
pub fn label_receipt_items(items: u32) -> String {
    l::confirm::receipt_items(items)
}

/// The sync chip read as a button: "Synced, tap for details", from the
/// chip's own VoiceOver words (`SyncStatus.chip_label`).
#[uniffi::export]
pub fn label_sync_chip(label: String) -> String {
    l::chrome::sync_chip(&label)
}

/// A confirmation ring read aloud (Coins' not-ready row, an Activity line):
/// "3 of 10 confirmations".
#[uniffi::export]
pub fn label_confirmations(done: u64, needed: u64) -> String {
    l::coins::confirmations(done, needed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_board_word_is_here() {
        let phone = labels("iPhone".into(), Biometry::FaceId);
        assert_eq!(phone.home.activity, "Activity");
        assert_eq!(phone.home.coins, "Coins");
        assert_eq!(phone.home.stagenet, "Stagenet");
        assert_eq!(phone.confirm.pay_with, "Pay with Face ID");
        assert_eq!(phone.receive.who_is_it_for, "Who is it for?");
        assert_eq!(phone.send.review, "Review");
        assert_eq!(phone.backup.written_down, "I've written them down");
        assert_eq!(phone.welcome.create, "Create a wallet");
        assert_eq!(phone.welcome.points.len(), 3);
        assert!(phone.welcome.points[2].line.ends_with("on this iPhone."));
        let mac = labels("Mac".into(), Biometry::TouchId);
        assert_eq!(mac.confirm.send_with, "Send with Touch ID");
        assert_eq!(mac.confirm.use_passphrase, "Use passphrase");
        assert_eq!(mac.settings.this_device, "This Mac");
        assert_eq!(label_pay("Corner Bakery".into()), "Pay Corner Bakery");
        assert_eq!(label_receipt_items(2), "Receipt · 2 items");
        assert_eq!(phone.chrome.scan_a_payment_code, "Scan a payment code");
        assert_eq!(phone.chrome.switch_wallet, "Switch wallet");
        assert_eq!(label_sync_chip("Synced".into()), "Synced, tap for details");
        assert_eq!(label_confirmations(3, 10), "3 of 10 confirmations");
    }
}
