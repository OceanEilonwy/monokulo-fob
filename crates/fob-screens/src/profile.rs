//! Your profile: a name and a photo to share as a code, so another Fob can
//! save you in one scan. There's no fixed profile address: every press of
//! "Share my profile" is a share, on a new address used for nothing else,
//! kept however briefly its code was shown. Payments to a share's address
//! come "Via your profile". The name also goes in requests' codes while
//! "Name in payment requests" is on.
//!
//! A scanned profile is matched against the contacts here too: one that
//! would change a contact (the same name, or the same address) is never
//! taken without asking.

use serde::{Deserialize, Serialize};

use crate::{activity::{self, Activity}, clock::{self, Clock}, contacts::{self, Contact}, error::ScreenError, error::Result};
use fob_monero::{amount, fiat};
use fob_codes::uri;

/// Your name and photo, kept in the wallet file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    /// 1 to [`contacts::NAME_MAX`] characters, cleaned as a contact's.
    pub name: String,
    /// A WebP of at most [`contacts::PHOTO_MAX`] bytes (hex in the file).
    #[serde(default, with = "contacts::photo_hex")]
    pub photo: Option<Vec<u8>>,
    /// Requests' codes carry the name ("Name in payment requests"); on
    /// once there's a name.
    #[serde(default = "on")]
    pub name_in_requests: bool,
}

fn on() -> bool {
    true
}

impl Profile {
    pub fn initial(&self) -> String {
        contacts::initial(&self.name)
    }
}

/// How a share went out. Each kind keeps its name in the file (serde's
/// external tags), so kinds added later read beside these.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShareKind {
    /// The code was on screen: the kind every share from Share my profile
    /// starts as.
    CodeShown,
    /// Copy link was pressed on its screen.
    LinkCopied,
    /// Share was pressed on its screen.
    LinkShared,
    /// Swapped by touching phones (NFC), with whom; no name when ours went
    /// out whole but theirs didn't come.
    TouchedPhones { name: Option<String> },
    /// Met by two codes (Meet's QR code tab): theirs was scanned or theirs
    /// scanned back, and they're saved.
    Met { name: String },
}

impl ShareKind {
    /// Whether someone's profile came back for it: the only shares Activity lists.
    pub fn completed(&self) -> bool {
        matches!(self, ShareKind::TouchedPhones { name: Some(_) } | ShareKind::Met { .. })
    }

}

/// One press of "Share my profile".
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Share {
    pub id: u64,
    /// Its address's index: handed out for this share only.
    pub subaddress: u32,
    pub address: String,
    pub kind: ShareKind,
    /// When it was made, seconds since the epoch.
    pub created: u64,
}

/// The profile and every share of it, kept in the wallet file.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Book {
    #[serde(default)]
    profile: Option<Profile>,
    #[serde(default)]
    shares: Vec<Share>,
    #[serde(default)]
    next_id: u64,
}

/// A typed name for the profile, checked as a contact's: 1 to
/// [`contacts::NAME_MAX`] characters once cleaned.
pub fn check_name(text: &str) -> Result<String> {
    let kept = contacts::clean(text);
    match kept.trim().chars().count() {
        0 => Err(ScreenError::ProfileNameEmpty),
        n if n > contacts::NAME_MAX => Err(ScreenError::ProfileNameTooLong { max: contacts::NAME_MAX }),
        _ => Ok(kept.trim().to_owned()),
    }
}

impl Book {
    pub fn profile(&self) -> Option<&Profile> {
        self.profile.as_ref()
    }

    /// Sets the name and photo ("Make it yours", or Edit). "Name in payment
    /// requests" stays as it was, on for a first profile.
    pub fn set(&mut self, name: &str, photo: Option<Vec<u8>>) -> Result<Profile> {
        let name = check_name(name)?;
        if let Some(photo) = &photo {
            contacts::check_photo(photo).map_err(|_| ScreenError::ProfilePhotoBad { max: contacts::PHOTO_MAX })?;
        }
        let name_in_requests = self.profile.as_ref().is_none_or(|p| p.name_in_requests);
        let profile = Profile { name, photo, name_in_requests };
        self.profile = Some(profile.clone());
        Ok(profile)
    }

    /// Drops the photo when [`contacts::check_photo`] refuses it (one saved
    /// before it read headers). Returns whether it did.
    pub fn drop_bad_photo(&mut self) -> bool {
        match self.profile.as_mut() {
            Some(p) if p.photo.as_deref().is_some_and(|photo| contacts::check_photo(photo).is_err()) => {
                p.photo = None;
                true
            }
            _ => false,
        }
    }

    /// "Name in payment requests", on or off.
    pub fn set_name_in_requests(&mut self, on: bool) -> Result<()> {
        let profile = self.profile.as_mut().ok_or(ScreenError::ProfileNotSetUp)?;
        profile.name_in_requests = on;
        Ok(())
    }

    /// The name requests' codes carry: the profile's, while the switch is on.
    pub fn name_in_requests(&self) -> Option<&str> {
        self.profile.as_ref().filter(|p| p.name_in_requests).map(|p| p.name.as_str())
    }

    /// Records a share on `subaddress`, which must be one no share has had. It
    /// needs no profile: Meet goes without a name.
    pub fn share(&mut self, subaddress: u32, address: String, kind: ShareKind, now: u64) -> Result<Share> {
        if self.at(subaddress).is_some() {
            return Err(ScreenError::ShareAddressUsed);
        }
        let id = self.next_id.max(self.shares.iter().map(|s| s.id + 1).max().unwrap_or(1)).max(1);
        self.next_id = id + 1;
        let share = Share { id, subaddress, address, kind, created: now };
        self.shares.push(share.clone());
        Ok(share)
    }

    pub fn get(&self, id: u64) -> Option<&Share> {
        self.shares.iter().find(|s| s.id == id)
    }

    /// The share made on the address at `subaddress`.
    pub fn at(&self, subaddress: u32) -> Option<&Share> {
        self.shares.iter().find(|s| s.subaddress == subaddress)
    }

    /// Copy link or Share pressed on a share's screen: it's now that kind,
    /// the last pressed. A share made by touching phones stays one: who it
    /// went to is what it's listed by. Returns whether it changed.
    pub fn went_out(&mut self, id: u64, kind: ShareKind) -> bool {
        let link = matches!(kind, ShareKind::LinkCopied | ShareKind::LinkShared);
        match self.shares.iter_mut().find(|s| s.id == id) {
            Some(s) if link && s.kind != kind && !matches!(s.kind, ShareKind::TouchedPhones { .. }) => {
                s.kind = kind;
                true
            }
            _ => false,
        }
    }

    /// Touched phones on share `id`'s screen: it's now kept as that,
    /// naming whom once theirs came (a name, once there, isn't taken away).
    /// Returns the share when it changed.
    pub fn touched(&mut self, id: u64, name: Option<String>) -> Option<Share> {
        let share = self.shares.iter_mut().find(|s| s.id == id)?;
        let name = match (&share.kind, name) {
            (ShareKind::TouchedPhones { name: Some(had) }, _) => Some(had.clone()),
            (_, name) => name,
        };
        let kind = ShareKind::TouchedPhones { name };
        if share.kind == kind {
            return None;
        }
        share.kind = kind;
        Some(share.clone())
    }

    /// Met by two codes, with whom: kept as that unless it already shows a meeting.
    pub fn met(&mut self, id: u64, name: String) -> Option<Share> {
        let share = self.shares.iter_mut().find(|s| s.id == id)?;
        if share.kind.completed() {
            return None;
        }
        share.kind = ShareKind::Met { name };
        Some(share.clone())
    }

    /// Every share, newest first.
    pub fn newest_first(&self) -> Vec<&Share> {
        let mut all: Vec<&Share> = self.shares.iter().collect();
        all.sort_by_key(|s| std::cmp::Reverse((s.created, s.id)));
        all
    }
}

/// A share's code, shown or sent as a link: its address and the profile's
/// name as they are now, or only the address without a profile. No photo:
/// photos go only by touching phones, so the code stays small and quick to scan.
pub fn code(profile: Option<&Profile>, share: &Share) -> String {
    match profile {
        Some(profile) => uri::profile_uri(&share.address, &profile.name, None),
        None => uri::nameless_uri(&share.address, false),
    }
}

/// A share's code as Meet's second one, after the other phone's was
/// scanned: the same, marked as a reply.
pub fn reply_code(profile: Option<&Profile>, share: &Share) -> String {
    match profile {
        Some(profile) => uri::profile_uri_with(&share.address, &profile.name, None, true),
        None => uri::nameless_uri(&share.address, true),
    }
}

/// A share's code with the photo, which touching phones swap.
pub fn swap_code(profile: Option<&Profile>, share: &Share) -> String {
    match profile {
        Some(profile) => uri::profile_uri(&share.address, &profile.name, profile.photo.as_deref()),
        None => uri::nameless_uri(&share.address, false),
    }
}

/// What `share`'s address received, "+$4.98" in the money prices are shown
/// in when every payment has a price, else "+0.03 XMR"; `None` while
/// nothing has.
pub fn received(share: &Share, rows: &[Activity]) -> Option<String> {
    let paid: Vec<Activity> = rows.iter().filter_map(|a| a.part(share.subaddress)).collect();
    let first = paid.first()?;
    let atomic: u64 = paid.iter().map(|a| a.amount).sum();
    let unit = first.price.as_ref().map(|p| p.currency.clone());
    let fiat = unit.and_then(|unit| {
        let currency = fiat::currency(&unit);
        let minor: Option<u64> = paid.iter().map(|a| a.price.as_ref().filter(|p| p.currency == unit).map(|p| fiat::to_minor(a.amount, p.price, &currency))).sum();
        minor.map(|m| fiat::format(m, &currency))
    });
    Some(format!("+{}", fiat.unwrap_or_else(|| format!("{} XMR", amount::format_short(atomic)))))
}

/// A share on Your profile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub id: u64,
    pub address: String,
    pub kind: ShareKind,
    pub when: clock::When,
    /// "+$4.98"; `None` while nothing has come.
    pub received: Option<String>,
}

/// Your shares, newest first ("Your shares · each its own address").
pub fn lines(book: &Book, rows: &[Activity], clock: Clock) -> Vec<Line> {
    book.newest_first()
        .into_iter()
        .map(|s| Line { id: s.id, address: s.address.clone(), kind: s.kind.clone(), when: clock::when(s.created, clock), received: received(s, rows) })
        .collect()
}

/// Each completed share's line in Activity, newest first, no amount. A share
/// nobody took (a code shown, a touch that gave nothing) isn't listed there;
/// Your profile still lists every one.
pub fn activity_rows(book: &Book, clock: Clock) -> Vec<activity::Row> {
    book.newest_first()
        .into_iter()
        .filter(|s| s.kind.completed())
        .map(|s| activity::Row {
            activity: None,
            request: None,
            share: Some(s.id),
            time: s.created,
            title: match &s.kind {
                ShareKind::TouchedPhones { name: Some(name) } | ShareKind::Met { name } => activity::RowTitle::Met(name.clone()),
                _ => activity::RowTitle::SharedProfile,
            },
            line: activity::RowLine::Share { time: clock::time_of_day(clock::local(s.created, clock).1, clock) },
            amount: String::new(),
            fiat: None,
        })
        .collect()
}

/// Settings' row for the profile: there's only the name to say, and none
/// while it's blank (a photo alone doesn't make a contact others can save).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingsRow {
    pub name: Option<String>,
}

pub fn settings_row(profile: Option<&Profile>) -> SettingsRow {
    SettingsRow { name: profile.map(|p| p.name.trim()).filter(|n| !n.is_empty()).map(str::to_owned) }
}

/// A change a scanned profile would make to a contact: what it is now and
/// what it would be.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub kind: ChangeKind,
    pub now: ChangeValue,
    pub new: ChangeValue,
}

/// One side of a change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChangeValue {
    /// A name, or an address cut to three parts.
    Text(String),
    /// No send address.
    Nothing,
    NoPhoto,
    TheirPhoto,
    ProfilePhoto,
}

/// What a change is to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeKind {
    Name,
    Photo,
    SendAddress,
}

/// Why a scanned profile matches a contact.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClashWhy {
    SendAddress,
    PaidAt,
    SameName,
    NameLooksLike,
}

/// A contact a scanned profile matches (the same address, else the same
/// name), and what taking the profile would change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Clash {
    pub contact: Contact,
    pub why: ClashWhy,
    /// Empty when the profile is what the contact has already.
    pub changes: Vec<Change>,
    /// The photo would change: draw both.
    pub photo_changes: bool,
    /// It matched by name: an address that's a contact's already is never
    /// given a second one.
    pub can_add_new: bool,
}

/// A scanned profile ("Another Fob scans the code").
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scanned {
    pub code: uri::ProfileCode,
    pub initial: String,
    /// "89Hn2 … Tq4Lp … 6wKeR"
    pub address_short: String,
    /// A contact it would change: asked about before anything is.
    pub clash: Option<Clash>,
}

fn changes(contact: &Contact, code: &uri::ProfileCode) -> (Vec<Change>, bool) {
    let mut changes = vec![];
    if contact.name != code.name {
        changes.push(Change { kind: ChangeKind::Name, now: ChangeValue::Text(contact.name.clone()), new: ChangeValue::Text(code.name.clone()) });
    }
    // A profile without a photo leaves the contact's.
    let photo_changes = code.photo.is_some() && code.photo != contact.photo;
    if photo_changes {
        let now = if contact.photo.is_some() { ChangeValue::TheirPhoto } else { ChangeValue::NoPhoto };
        changes.push(Change { kind: ChangeKind::Photo, now, new: ChangeValue::ProfilePhoto });
    }
    if contact.send_address.as_deref() != Some(code.address.as_str()) {
        let now = contact.send_address.as_deref().map_or(ChangeValue::Nothing, |a| ChangeValue::Text(contacts::three_part(a)));
        changes.push(Change { kind: ChangeKind::SendAddress, now, new: ChangeValue::Text(contacts::three_part(&code.address)) });
    }
    (changes, photo_changes)
}

/// What scanning `code` shows: the profile, and the contact it would change
/// (the one with its address, else the oldest whose name is its name or
/// looks like it).
pub fn scanned(book: &contacts::Book, code: uri::ProfileCode) -> Scanned {
    let clash = book
        .owner(&code.address)
        .map(|c| (c, if c.send_address.as_deref() == Some(code.address.as_str()) { ClashWhy::SendAddress } else { ClashWhy::PaidAt }, false))
        .or_else(|| {
            book.named(&code.name).filter(|_| !code.name.is_empty()).map(|c| (c, if c.name.to_lowercase() == code.name.to_lowercase() { ClashWhy::SameName } else { ClashWhy::NameLooksLike }, true))
        })
        .map(|(contact, why, by_name)| {
            let (changes, photo_changes) = changes(contact, &code);
            Clash { can_add_new: by_name, contact: contact.clone(), why, changes, photo_changes }
        });
    Scanned { initial: contacts::initial(&code.name), address_short: contacts::three_part(&code.address), clash, code }
}

/// A new contact from a scanned profile: its name, photo and address as
/// the send address.
pub fn new_contact(code: &uri::ProfileCode) -> contacts::Draft {
    contacts::Draft { name: code.name.clone(), photo: code.photo.clone(), send_address: Some(code.address.clone()), ..Default::default() }
}

/// `contact` taking a scanned profile, once asked: its name, its photo
/// (when it has one) and its address as the send address; the note and
/// the addresses it was paid at stay.
pub fn updated(contact: &Contact, code: &uri::ProfileCode) -> contacts::Draft {
    contacts::Draft {
        name: code.name.clone(),
        note: contact.note.clone(),
        photo: code.photo.clone().or_else(|| contact.photo.clone()),
        send_address: Some(code.address.clone()),
        also_paid_at: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fob_vault::keys::Keys;
    use fob_monero::network::Network;

    /// 2026-10-31 12:00 UTC.
    const NOW: u64 = 1_793_448_000;
    const DAY: u64 = 86_400;

    fn webp(len: usize) -> Vec<u8> {
        fob_codes::names::test_webp(len, 96)
    }

    fn code(name: &str, address: &str, photo: Option<Vec<u8>>) -> uri::ProfileCode {
        uri::ProfileCode { address: address.into(), name: name.into(), photo, reply: false }
    }

    #[test]
    fn a_profile_s_name_is_checked_and_the_switch_starts_on_and_stays_as_set() {
        let mut book = Book::default();
        assert_eq!(book.name_in_requests(), None);
        assert!(book.set_name_in_requests(true).is_err(), "no profile yet");
        assert!(book.set(" \u{202e}\n ", None).is_err());
        assert!(book.set(&"x".repeat(41), None).is_err());
        assert!(book.set("Henry", Some(b"PNG".to_vec())).is_err());
        assert!(book.set("Henry", Some(webp(contacts::PHOTO_MAX + 1))).is_err());
        let p = book.set(" Hen\u{200f}ry\n", Some(webp(900))).unwrap();
        assert_eq!((p.name.as_str(), p.name_in_requests), ("Henry", true));
        assert_eq!(book.name_in_requests(), Some("Henry"));
        book.set_name_in_requests(false).unwrap();
        assert_eq!(book.name_in_requests(), None);
        // Editing the name keeps the switch off.
        assert!(!book.set("Hen", None).unwrap().name_in_requests);
        assert_eq!(book.profile().unwrap().photo, None);
    }

    #[test]
    fn a_share_without_a_profile_gives_only_its_address() {
        let keys = Keys::generate(Network::Stagenet).keys;
        let mut book = Book::default();
        let share = book.share(4, keys.address(4), ShareKind::CodeShown, NOW).unwrap();
        for text in [super::code(None, &share), swap_code(None, &share), reply_code(None, &share)] {
            assert!(!text.contains("recipient_name"));
            let read = uri::parse_meet_code(Network::Stagenet, &text).unwrap();
            assert_eq!((read.name.as_str(), read.address.as_str()), ("", share.address.as_str()));
        }
        assert!(uri::parse_meet_code(Network::Stagenet, &reply_code(None, &share)).unwrap().reply);
    }

    #[test]
    fn a_share_needs_an_address_of_its_own_and_goes_out_as_pressed() {
        let keys = Keys::generate(Network::Stagenet).keys;
        let mut book = Book::default();
        book.set("Henry", None).unwrap();
        let first = book.share(5, keys.address(5), ShareKind::CodeShown, NOW - DAY * 3).unwrap();
        assert!(book.share(5, keys.address(5), ShareKind::CodeShown, NOW).is_err(), "that address is the first share's");
        let second = book.share(6, keys.address(6), ShareKind::CodeShown, NOW).unwrap();
        assert_eq!((first.id, second.id, second.kind.clone()), (1, 2, ShareKind::CodeShown));
        assert!(book.went_out(second.id, ShareKind::LinkCopied));
        assert!(book.went_out(second.id, ShareKind::LinkShared));
        assert!(!book.went_out(second.id, ShareKind::LinkShared));
        assert!(!book.went_out(second.id, ShareKind::CodeShown), "a link sent stays sent");
        assert!(!book.went_out(99, ShareKind::LinkShared));
        assert_eq!(book.at(6).map(|s| s.kind.clone()), Some(ShareKind::LinkShared));
        assert_eq!(book.newest_first().iter().map(|s| s.id).collect::<Vec<_>>(), [2, 1]);
        // Kept in the file by name, as step 10's kind will be.
        let json = serde_json::to_string(&book).unwrap();
        assert!(json.contains(r#""kind":"link_shared""#) && json.contains(r#""kind":"code_shown""#), "{json}");
        assert_eq!(serde_json::from_str::<Book>(&json).unwrap(), book);
        // A file from before the switch: on.
        let old: Profile = serde_json::from_str(r#"{"name":"Henry"}"#).unwrap();
        assert!(old.name_in_requests);
    }

    #[test]
    fn a_scanned_profile_says_what_it_clashes_with_without_words() {
        let network = Network::Stagenet;
        let keys = Keys::generate(network).keys;
        let mut book = contacts::Book::default();
        let sam = book.add(network, contacts::Draft { name: "Sam".into(), send_address: Some(keys.address(1)), ..Default::default() }, NOW).unwrap();
        let by_address = scanned(&book, code("Henry", &keys.address(1), None));
        let clash = by_address.clash.unwrap();
        assert_eq!((clash.contact.id, clash.why, clash.can_add_new), (sam.id, ClashWhy::SendAddress, false));
        assert_eq!(clash.changes, [Change { kind: ChangeKind::Name, now: ChangeValue::Text("Sam".into()), new: ChangeValue::Text("Henry".into()) }]);
        let by_name = scanned(&book, code("sam", &keys.address(2), None)).clash.unwrap();
        assert_eq!((by_name.why, by_name.can_add_new), (ClashWhy::SameName, true));
    }
}
