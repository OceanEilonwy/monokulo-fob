//! Backup and keys as the screens show them: Home's reminder, the backup hub
//! and every key written out. The keys themselves are kept elsewhere.

use zeroize::Zeroizing;

use crate::{Device, device};

/// What Keys' page writes out.
#[derive(Clone, PartialEq)]
pub struct KeyValues {
    /// The secret spend key; none for a view-only wallet.
    pub spend: Option<Zeroizing<String>>,
    pub view: Zeroizing<String>,
    pub address: String,
    pub public_view: String,
    pub public_spend: String,
    pub restore_height: u64,
    /// When the restore height was, when that can be named.
    pub date: Option<String>,
}

// Zeroizing's Debug prints what it holds, and spend and view are the secret
// keys, so printing the values (as KeyRow does) must not show them. Whether
// there is a spend key is kept, since that alone says nothing secret.
impl std::fmt::Debug for KeyValues {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeyValues")
            .field("has_spend", &self.spend.is_some())
            .field("address", &self.address)
            .field("public_view", &self.public_view)
            .field("public_spend", &self.public_spend)
            .field("restore_height", &self.restore_height)
            .field("date", &self.date)
            .finish_non_exhaustive()
    }
}

/// What a key's badge says it lets someone do, which also sets its colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyBadge {
    SpendsMoney,
    SeesPayments,
    Public,
}

impl KeyBadge {
    fn text(self) -> &'static str {
        match self {
            KeyBadge::SpendsMoney => "Spends money",
            KeyBadge::SeesPayments => "Sees payments",
            KeyBadge::Public => "Public",
        }
    }
}

/// One key on Keys' page, ready to draw.
#[derive(Clone, PartialEq)]
pub struct KeyRow {
    /// "Secret spend key", "Main address"
    pub title: String,
    pub badge: KeyBadge,
    /// "Spends money", "Sees payments", "Public"
    pub badge_text: String,
    /// What it's for; none for the public keys.
    pub line: Option<String>,
    /// In full: 64 hex digits, the whole address, "1690400 · 2 Sep 2026".
    pub value: Zeroizing<String>,
    /// What Copy puts on the clipboard: the value, but the restore height
    /// without its date, as other wallets take it.
    pub clip: Zeroizing<String>,
    /// The Copy button's label: "Copy the secret spend key".
    pub copy: String,
}

// Zeroizing's Debug prints what it holds, and value and clip are secret keys
// on the secret card, so printing a row must not show them.
impl std::fmt::Debug for KeyRow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeyRow")
            .field("title", &self.title)
            .field("badge", &self.badge)
            .field("badge_text", &self.badge_text)
            .field("line", &self.line)
            .field("copy", &self.copy)
            .finish_non_exhaustive()
    }
}

/// A card of keys under its heading: "Secret: keep to yourself", "Public".
#[derive(Clone, Debug, PartialEq)]
pub struct KeyGroup {
    pub title: String,
    pub keys: Vec<KeyRow>,
}

/// Where a wallet's seed phrase stands, for Backup and keys' Seed phrase
/// row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backed {
    /// Made here and said to be written down.
    Up,
    /// Made here and not written down yet: Home reminds.
    Pending,
    /// Restored: the words were already somewhere, so nothing is said.
    Restored,
}

/// What a row of Backup and keys opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HubItem {
    SeedPhrase,
    WalletCode,
    Keys,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HubRow {
    pub item: HubItem,
    pub title: String,
    pub line: String,
}

/// Backup and keys (board KeysHub), every word ready to draw, and the line
/// for its row in Settings.
#[derive(Clone, Debug, PartialEq)]
pub struct Hub {
    pub warning: String,
    /// Under "Back up": Seed phrase (not on a view-only wallet), Wallet code.
    pub backup: Vec<HubRow>,
    /// Under "For other wallets": Keys.
    pub other: Vec<HubRow>,
    pub footer: String,
    /// Settings' Backup and keys row: "Seed phrase, wallet code, keys".
    pub settings_line: String,
}

/// Backup and keys for a wallet whose seed phrase has `words` words (none
/// for a view-only wallet), written down or not as `backed` says, on a
/// phone with a screen lock set or not (`phone_lock`: without one, Backup
/// and keys opens straight away, so the warning doesn't promise a lock).
pub fn hub(words: Option<usize>, backed: Backed, phone_lock: bool) -> Hub {
    hub_on(device(), words, backed, phone_lock)
}

fn hub_on(device: Device, words: Option<usize>, backed: Backed, phone_lock: bool) -> Hub {
    let row = |item, title: &str, line: String| HubRow { item, title: title.to_owned(), line };
    let mut backup = Vec::new();
    if let Some(n) = words {
        let state = match backed {
            Backed::Up => " · backed up",
            Backed::Pending => " · not written down yet",
            Backed::Restored => "",
        };
        backup.push(row(HubItem::SeedPhrase, "Seed phrase", format!("{n} words that restore everything{state}")));
    }
    backup.push(row(HubItem::WalletCode, "Wallet code", format!("A QR code another {} can scan", device.noun())));
    let other = vec![row(HubItem::Keys, "Keys", "View and spend keys, address, restore height".into())];
    let spends = words.is_some();
    let who = if spends { "Anyone who sees these can take or watch your money." } else { "Anyone who sees these can watch your money." };
    let hides = match device {
        Device::Phone if phone_lock => "Fob hides them from screenshots and asks for your phone's lock first.",
        Device::Phone => "Fob hides them from screenshots.",
        // No desktop can promise to keep them out of a screenshot.
        Device::Computer => "Fob asks for your passphrase first, but can't keep them out of a screenshot.",
    };
    Hub {
        warning: format!("{who} {hides}"),
        backup,
        other,
        footer: if spends {
            "The seed phrase is all you need. The code and keys are for moving this wallet to another app."
        } else {
            "This wallet only watches. The code and keys are for moving it to another app."
        }
        .into(),
        settings_line: if spends { "Seed phrase, wallet code, keys" } else { "Wallet code, keys" }.into(),
    }
}

/// Home's card while a wallet made here hasn't had its seed phrase of
/// `words` words written down, and the warning over the words wherever
/// they're shown meanwhile (after making the wallet, from the card, or from
/// Backup and keys).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reminder {
    pub title: String,
    pub line: String,
    pub seed_warning: String,
}

pub fn reminder(words: usize) -> Reminder {
    Reminder {
        title: "Back up your seed phrase".into(),
        line: format!("Your {words} words aren't written down yet. Without them, this wallet can't be restored."),
        seed_warning: format!(
            "These {words} words are your wallet. Write them down in order and keep them somewhere safe: anyone with them can spend your money, and without them it can't be restored."
        ),
    }
}

/// Keys' page (board KeysDetail): the secret keys first, each saying
/// what it lets someone do, then the public ones. The restore height
/// shows its `date` when there is one ("1690400 · 2 Sep 2026"), and
/// copies as the number alone. A view-only wallet has no secret spend
/// key row.
pub fn page(values: KeyValues) -> Vec<KeyGroup> {
    let KeyValues { spend, view, address, public_view, public_spend, restore_height, date } = values;
    let row = |title: &str, badge: KeyBadge, line: Option<&str>, value: Zeroizing<String>| KeyRow {
        title: title.to_owned(),
        badge,
        badge_text: badge.text().to_owned(),
        line: line.map(str::to_owned),
        clip: value.clone(),
        value,
        copy: format!("Copy the {}", title.to_lowercase()),
    };
    let restore = match date {
        Some(date) => format!("{restore_height} · {date}"),
        None => restore_height.to_string(),
    };
    let mut height = row("Restore height", KeyBadge::Public, Some("Where a restore starts looking."), Zeroizing::new(restore));
    height.clip = Zeroizing::new(restore_height.to_string());
    let mut secret = Vec::new();
    if let Some(spend) = spend {
        secret.push(row("Secret spend key", KeyBadge::SpendsMoney, Some("With the view key, restores this wallet anywhere."), spend));
    }
    secret.push(row("Secret view key", KeyBadge::SeesPayments, Some("Lets a watch-only wallet see what arrives."), view));
    let public = vec![
        row("Main address", KeyBadge::Public, None, Zeroizing::new(address)),
        height,
        row("Public view key", KeyBadge::Public, None, Zeroizing::new(public_view)),
        row("Public spend key", KeyBadge::Public, None, Zeroizing::new(public_spend)),
    ];
    vec![KeyGroup { title: "Secret: keep to yourself".into(), keys: secret }, KeyGroup { title: "Public".into(), keys: public }]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_reminder_and_the_seed_warning_say_how_many_words() {
        let r = reminder(16);
        assert_eq!(r.title, "Back up your seed phrase");
        assert_eq!(r.line, "Your 16 words aren't written down yet. Without them, this wallet can't be restored.");
        assert_eq!(
            reminder(25).seed_warning,
            "These 25 words are your wallet. Write them down in order and keep them somewhere safe: anyone with them can spend your money, and without them it can't be restored."
        );
    }

    #[test]
    fn the_hub_says_how_many_words_and_whether_they_are_written_down() {
        let made = hub(Some(16), Backed::Up, true);
        assert_eq!(made.backup.iter().map(|r| r.item).collect::<Vec<_>>(), [HubItem::SeedPhrase, HubItem::WalletCode]);
        assert_eq!(made.backup[0].title, "Seed phrase");
        assert_eq!(made.backup[0].line, "16 words that restore everything · backed up");
        assert_eq!(made.backup[1].line, "A QR code another phone can scan");
        assert_eq!(made.other[0].item, HubItem::Keys);
        assert_eq!(made.other[0].line, "View and spend keys, address, restore height");
        assert_eq!(made.footer, "The seed phrase is all you need. The code and keys are for moving this wallet to another app.");
        assert_eq!(made.settings_line, "Seed phrase, wallet code, keys");
        assert_eq!(hub(Some(16), Backed::Pending, true).backup[0].line, "16 words that restore everything · not written down yet");
        assert_eq!(hub(Some(25), Backed::Restored, true).backup[0].line, "25 words that restore everything");
    }

    #[test]
    fn the_hub_s_warning_promises_the_phone_s_lock_only_when_one_is_set() {
        // Any lock (PIN, pattern, fingerprint, face) is asked for, so the
        // words name none in particular.
        assert_eq!(
            hub(Some(16), Backed::Up, true).warning,
            "Anyone who sees these can take or watch your money. Fob hides them from screenshots and asks for your phone's lock first."
        );
        assert_eq!(
            hub(None, Backed::Restored, true).warning,
            "Anyone who sees these can watch your money. Fob hides them from screenshots and asks for your phone's lock first."
        );
        // No lock set: Backup and keys opens straight away, so nothing is promised.
        assert_eq!(
            hub(Some(16), Backed::Up, false).warning,
            "Anyone who sees these can take or watch your money. Fob hides them from screenshots."
        );
        assert_eq!(hub(None, Backed::Restored, false).warning, "Anyone who sees these can watch your money. Fob hides them from screenshots.");
    }

    #[test]
    fn a_view_only_wallet_s_hub_has_no_seed_phrase() {
        let watch = hub(None, Backed::Restored, true);
        assert_eq!(watch.backup.iter().map(|r| r.item).collect::<Vec<_>>(), [HubItem::WalletCode]);
        assert_eq!(watch.other.len(), 1);
        assert!(watch.warning.starts_with("Anyone who sees these can watch your money."));
        assert_eq!(watch.footer, "This wallet only watches. The code and keys are for moving it to another app.");
        assert_eq!(watch.settings_line, "Wallet code, keys");
    }

    const SPEND: &str = "c0af65c0dd837e666b9d0dfed62745f4df35aed7ea619b2798a709f0fe545403";
    const VIEW: &str = "513ba91c538a5a9069e0094de90e927c0cd147fa10428ce3ac1afd49f63e3b01";

    fn values(spend: bool, date: Option<&str>) -> KeyValues {
        KeyValues {
            spend: spend.then(|| Zeroizing::new(SPEND.to_owned())),
            view: Zeroizing::new(VIEW.to_owned()),
            address: "4".repeat(95),
            public_view: "a".repeat(64),
            public_spend: "b".repeat(64),
            restore_height: 1_690_400,
            date: date.map(str::to_owned),
        }
    }

    #[test]
    fn printing_the_values_or_a_row_never_shows_a_secret_key() {
        let printed = format!("{:?} {:#?}", values(true, Some("2 Sep 2026")), values(true, None));
        assert!(!printed.contains(SPEND) && !printed.contains(VIEW), "{printed}");
        assert!(printed.contains("has_spend: true"));
        let rows = format!("{:?}", page(values(true, None)));
        assert!(!rows.contains(SPEND) && !rows.contains(VIEW), "{rows}");
    }

    #[test]
    fn keys_page_shows_every_key_in_full() {
        let page = page(values(true, Some("2 Sep 2026")));
        let titles: Vec<Vec<&str>> = page.iter().map(|g| g.keys.iter().map(|k| k.title.as_str()).collect()).collect();
        assert_eq!(page[0].title, "Secret: keep to yourself");
        assert_eq!(page[1].title, "Public");
        assert_eq!(
            titles,
            [vec!["Secret spend key", "Secret view key"], vec!["Main address", "Restore height", "Public view key", "Public spend key"]]
        );
        let value = |g: usize, k: usize| page[g].keys[k].value.to_string();
        assert_eq!(value(0, 0), SPEND);
        assert_eq!(value(0, 1), VIEW);
        assert_eq!(value(1, 0), "4".repeat(95), "the whole address, not cut");
        assert_eq!(value(1, 2), "a".repeat(64));
        assert_eq!(value(1, 3), "b".repeat(64));
        assert_eq!(value(1, 1), "1690400 · 2 Sep 2026");
        assert_eq!(*page[1].keys[1].clip, "1690400", "copied as other wallets take it");
        for (g, k) in [(0, 0), (0, 1), (1, 0), (1, 2), (1, 3)] {
            assert_eq!(page[g].keys[k].clip, page[g].keys[k].value);
        }
        assert_eq!(*super::page(values(true, None))[1].keys[1].value, "1690400");
    }

    #[test]
    fn keys_page_words() {
        let page = page(values(true, None));
        let spend = &page[0].keys[0];
        assert_eq!((spend.badge, spend.badge_text.as_str()), (KeyBadge::SpendsMoney, "Spends money"));
        assert_eq!(spend.line.as_deref(), Some("With the view key, restores this wallet anywhere."));
        assert_eq!(spend.copy, "Copy the secret spend key");
        let view = &page[0].keys[1];
        assert_eq!((view.badge, view.badge_text.as_str()), (KeyBadge::SeesPayments, "Sees payments"));
        assert_eq!(view.line.as_deref(), Some("Lets a watch-only wallet see what arrives."));
        let main = &page[1].keys[0];
        assert_eq!((main.badge, main.badge_text.as_str()), (KeyBadge::Public, "Public"));
        assert_eq!(main.line, None);
        assert_eq!(main.copy, "Copy the main address");
        assert_eq!(page[1].keys[1].line.as_deref(), Some("Where a restore starts looking."));
        assert_eq!(page[1].keys[2].line, None);
        assert_eq!(page[1].keys[3].copy, "Copy the public spend key");
    }

    #[test]
    fn a_view_only_wallet_s_page_has_no_secret_spend_key() {
        let watch = page(values(false, None));
        assert_eq!(watch[0].keys.len(), 1);
        assert_eq!(watch[0].keys[0].title, "Secret view key");
        assert_eq!(*watch[0].keys[0].value, VIEW);
        // It still shows both public keys, from the address it was given.
        assert_eq!(watch[1], page(values(true, None))[1]);
    }

    #[test]
    fn on_a_computer_the_hub_promises_only_what_a_computer_can_do() {
        let hub = hub_on(Device::Computer, Some(16), Backed::Up, false);
        assert_eq!(hub.backup[1].line, "A QR code another computer can scan");
        assert_eq!(hub.warning, "Anyone who sees these can take or watch your money. Fob asks for your passphrase first, but can't keep them out of a screenshot.");
    }
}
