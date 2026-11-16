//! The words a screen shows around the wallet's own: tabs, page titles,
//! buttons and headings. The wallet's data (amounts, names, lines) is said
//! by the other modules; these are the words that never change with it, so
//! every app (the Apple ones first) takes them from here instead of writing
//! them itself.

/// How a device checks it's you, for the buttons that pay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Biometry {
    FaceId,
    TouchId,
    OpticId,
}

impl Biometry {
    /// "Face ID", "Touch ID", "Optic ID".
    pub fn name(self) -> &'static str {
        match self {
            Biometry::FaceId => "Face ID",
            Biometry::TouchId => "Touch ID",
            Biometry::OpticId => "Optic ID",
        }
    }
}

/// The tab bar.
pub mod tabs {
    pub const WALLET: &str = "Wallet";
    pub const CONTACTS: &str = "Contacts";
    pub const SETTINGS: &str = "Settings";
}

/// Home: the balance card, its buttons and Activity.
pub mod home {
    pub const RECEIVE: &str = "Receive";
    pub const SEND: &str = "Send";
    pub const ACTIVITY: &str = "Activity";
    pub const COINS: &str = "Coins";
    /// After the amount ready: "0.3870 ready".
    pub const READY: &str = "ready";
    /// After the amount not ready: "0.0312 not yet".
    pub const NOT_YET: &str = "not yet";
    /// After the amount ready, where there's room: "0.3870 ready to spend".
    pub const READY_TO_SPEND: &str = "ready to spend";
    /// Under a request's amount.
    pub const ASKED: &str = "asked";
    /// The chip beside the wallet's name on the test network.
    pub const STAGENET: &str = "Stagenet";
}

/// Receive.
pub mod receive {
    pub const TITLE: &str = "Receive";
    /// The field that names who a request is for.
    pub const WHO_IS_IT_FOR: &str = "Who is it for?";
}

/// Send: who to pay, then the form.
pub mod send {
    pub const TITLE: &str = "Send";
    /// Choosing who, where there's no camera page (iPad's sheet).
    pub const PAY_WHO: &str = "Pay who?";
    /// Over the camera.
    pub const POINT_AT_CODE: &str = "Point at a payment code";
    pub const SCAN_A_CODE: &str = "Scan a code";
    pub const PASTE_AN_ADDRESS: &str = "Paste an address";
    pub const CONTACTS: &str = "Contacts";
    pub const PAID_BEFORE: &str = "Paid before";
    pub const TO: &str = "To";
    pub const PASTE: &str = "Paste";
    pub const AMOUNT: &str = "Amount";
    pub const NOTE: &str = "Note (optional)";
    pub const REVIEW: &str = "Review";
    pub const CANCEL: &str = crate::connect::CANCEL;
}

/// Confirming a payment.
pub mod confirm {
    use super::Biometry;

    pub const TITLE: &str = "Confirm";
    pub const TO: &str = "To";
    pub const NOTE: &str = "Note";
    pub const ADD_A_NOTE: &str = "Add a note";
    pub const NETWORK_FEE: &str = "Network fee";
    pub const WITH_THE_FEE: &str = "With the fee";
    pub const INCLUDES_TAX: &str = "Includes tax";
    pub const TOTAL: &str = "Total";
    pub const USE_PASSPHRASE: &str = "Use passphrase";

    /// The title paying someone: "Pay Corner Bakery".
    pub fn pay(name: &str) -> String {
        format!("Pay {name}")
    }

    /// The receipt's heading: "Receipt · 2 items".
    pub fn receipt_items(items: u32) -> String {
        format!("Receipt · {items} item{}", if items == 1 { "" } else { "s" })
    }

    /// The button on a phone: "Pay with Face ID".
    pub fn pay_with(biometry: Biometry) -> String {
        format!("Pay with {}", biometry.name())
    }

    /// The button on a computer: "Send with Touch ID".
    pub fn send_with(biometry: Biometry) -> String {
        format!("Send with {}", biometry.name())
    }
}

/// A payment's own screen.
pub mod payment {
    pub const WORTH_NOW: &str = "Worth now";
    pub const STATUS: &str = "Status";
    pub const NETWORK_FEE: &str = "Network fee";
    pub const TRANSACTION: &str = "Transaction";
    pub const COPY: &str = "Copy";
    pub const RECEIPT: &str = "Receipt";
    pub const TOTAL: &str = "Total";
    pub const PAID_IN_MONERO: &str = "Paid in Monero";
}

/// The Contacts tab.
pub mod contacts {
    pub const TITLE: &str = "Contacts";
    pub const PAID_NOT_SAVED: &str = "Paid, not saved";
    pub const SAVE: &str = "Save";
}

/// Settings, on a phone and in a computer's Settings window.
pub mod settings {
    pub const TITLE: &str = "Settings";
    pub const WALLET: &str = "Wallet";
    pub const WALLETS: &str = "Wallets";
    pub const BACKUP_AND_KEYS: &str = "Backup and keys";
    pub const DISPLAY_AND_MONEY: &str = "Display and money";
    pub const APPEARANCE: &str = "Appearance";
    pub const DEFAULT_CURRENCY: &str = "Default currency";
    pub const PRICE_FROM: &str = "Price from";
    pub const SYNCING: &str = "Syncing";
    pub const SYNC_THROUGH: &str = crate::light::SYNC_THROUGH;
    pub const SERVER: &str = "Server";
    pub const NODE: &str = crate::light::THROUGH_NODE;
    pub const SCANNING: &str = "Scanning";
    pub const MANUAL_SYNC: &str = "Manual Sync";
    pub const RESCAN: &str = "Rescan";
    // A computer's Settings window.
    pub const GENERAL: &str = "General";
    pub const MONEY: &str = "Money";
    pub const LIGHT: &str = "Light";
    pub const DARK: &str = "Dark";
    pub const OLED: &str = "OLED";
    /// Under the appearance choice while OLED is chosen.
    pub const OLED_LINE: &str =
        "OLED draws on true black, where an OLED or mini-LED pixel is off. Cards are held by their outlines.";
    pub const SHOW_AMOUNTS_FIRST_IN: &str = "Show amounts first in";
    pub const LOCK_WHEN_LEFT_ALONE: &str = "Lock when left alone";
    pub const UNLOCK_WITH: &str = "Unlock with";
    pub const PASSPHRASE_ALWAYS_WORKS: &str = "Your passphrase always works too.";
    pub const CHANGE_PASSPHRASE: &str = "Change passphrase…";
    pub const LOCK_NOW: &str = "Lock now";

    /// The settings about the device itself: "This Mac".
    pub fn this_device(device: &str) -> String {
        format!("This {device}")
    }

    /// The appearance that follows the device's: "Same as Mac".
    pub fn same_as_device(device: &str) -> String {
        format!("Same as {device}")
    }
}

/// Welcome, the first run.
pub mod welcome {
    pub const TITLE: &str = "Fob";
    pub const TAGLINE: &str = "A Monero wallet for everyday paying. Quick to catch up, and it keeps your receipts.";
    pub const USE_MAINNET: &str = "Use mainnet";
    pub const CREATE: &str = "Create a wallet";
    pub const RESTORE: &str = "Restore a wallet";

    /// Its three points, title and line, on `device` ("iPhone").
    pub fn points(device: &str) -> [(&'static str, String); 3] {
        [
            ("Ready in seconds", "Use it while it catches up. Money in shows straight away.".into()),
            ("Keeps your receipts", "Shops that include one in the payment code send it with the bill.".into()),
            ("Yours alone", format!("Talks only to the node you pick. Receipts stay on this {device}.")),
        ]
    }
}

/// Back up your wallet.
pub mod backup {
    pub const TITLE: &str = "Back up your wallet";
    pub const WRITTEN_DOWN: &str = "I've written them down";
    pub const BACK: &str = "Back";
}

/// Coins and Split up.
pub mod coins {
    pub const TITLE: &str = "Coins";
    pub const SPLIT_UP: &str = crate::split::TITLE;
    pub const CLEAR: &str = "Clear";
}

/// The Scan sheet.
pub mod scan {
    pub const TITLE: &str = "Scan";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buttons_name_the_check() {
        assert_eq!(confirm::pay_with(Biometry::FaceId), "Pay with Face ID");
        assert_eq!(confirm::send_with(Biometry::TouchId), "Send with Touch ID");
        assert_eq!(confirm::pay("Corner Bakery"), "Pay Corner Bakery");
        assert_eq!(confirm::receipt_items(2), "Receipt · 2 items");
        assert_eq!(confirm::receipt_items(1), "Receipt · 1 item");
    }

    #[test]
    fn device_words() {
        assert_eq!(settings::this_device("Mac"), "This Mac");
        assert_eq!(settings::same_as_device("Mac"), "Same as Mac");
        assert_eq!(welcome::points("iPhone")[2].1, "Talks only to the node you pick. Receipts stay on this iPhone.");
        assert_eq!((send::CANCEL, settings::NODE, coins::SPLIT_UP), ("Cancel", "Node", "Split up"));
    }
}
