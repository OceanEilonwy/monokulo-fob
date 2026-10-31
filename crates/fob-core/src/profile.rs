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

use crate::{
    activity::{self, Clock},
    amount,
    contacts::{self, Contact},
    error::{FobError, Result},
    price, uri,
    wallet::Activity,
};

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
}

impl ShareKind {
    /// "Code shown", "Link copied", "Link shared", "Touched phones with Sam".
    pub fn title(&self) -> String {
        match self {
            ShareKind::CodeShown => "Code shown".into(),
            ShareKind::LinkCopied => "Link copied".into(),
            ShareKind::LinkShared => "Link shared".into(),
            ShareKind::TouchedPhones { name: Some(name) } => format!("Touched phones with {name}"),
            ShareKind::TouchedPhones { name: None } => "Touched phones".into(),
        }
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
        0 => Err(FobError::Profile("Your profile needs a name.".into())),
        n if n > contacts::NAME_MAX => Err(FobError::Profile(format!("A name can be up to {} characters.", contacts::NAME_MAX))),
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
            contacts::check_photo(photo).map_err(|_| FobError::Profile(format!("Your photo must be a WebP of at most {} bytes.", contacts::PHOTO_MAX)))?;
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
        let profile = self.profile.as_mut().ok_or_else(|| FobError::Profile("Set up your profile first.".into()))?;
        profile.name_in_requests = on;
        Ok(())
    }

    /// The name requests' codes carry: the profile's, while the switch is on.
    pub fn name_in_requests(&self) -> Option<&str> {
        self.profile.as_ref().filter(|p| p.name_in_requests).map(|p| p.name.as_str())
    }

    /// Records a share on `subaddress`, which must be one no share has had.
    pub fn share(&mut self, subaddress: u32, address: String, kind: ShareKind, now: u64) -> Result<Share> {
        if self.profile.is_none() {
            return Err(FobError::Profile("Set up your profile first.".into()));
        }
        if self.at(subaddress).is_some() {
            return Err(FobError::Profile("That address was already given to a share.".into()));
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

    /// Every share, newest first.
    pub fn newest_first(&self) -> Vec<&Share> {
        let mut all: Vec<&Share> = self.shares.iter().collect();
        all.sort_by_key(|s| std::cmp::Reverse((s.created, s.id)));
        all
    }
}

/// A share's code: its address, the profile's name and photo as they are
/// now.
pub fn code(profile: &Profile, share: &Share) -> String {
    uri::profile_uri(&share.address, &profile.name, profile.photo.as_deref())
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
        let currency = price::currency(&unit);
        let minor: Option<u64> = paid.iter().map(|a| a.price.as_ref().filter(|p| p.currency == unit).map(|p| price::to_minor(a.amount, p.price, &currency))).sum();
        minor.map(|m| price::format(m, &currency))
    });
    Some(format!("+{}", fiat.unwrap_or_else(|| format!("{} XMR", amount::format_short(atomic)))))
}

/// A share on Your profile, ready to draw.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub id: u64,
    pub address: String,
    pub kind: ShareKind,
    /// "Code shown", "Link copied"
    pub title: String,
    /// "Today 09:15", "Yesterday 18:02", "28 Oct"
    pub when: String,
    /// "+$4.98", or "nothing yet".
    pub received: String,
    /// Something came: drawn green.
    pub any: bool,
}

/// "Today 09:15", "Yesterday 18:02", "28 Oct": a share's time on Your profile.
fn when(time: u64, clock: Clock) -> String {
    match activity::day_name(time, clock).as_str() {
        "Today" | "Yesterday" => activity::when(time, clock),
        day => day.to_owned(),
    }
}

/// Your shares, newest first ("Your shares · each its own address").
pub fn lines(book: &Book, rows: &[Activity], clock: Clock) -> Vec<Line> {
    book.newest_first()
        .into_iter()
        .map(|s| {
            let got = received(s, rows);
            Line {
                id: s.id,
                address: s.address.clone(),
                kind: s.kind.clone(),
                title: s.kind.title(),
                when: when(s.created, clock),
                any: got.is_some(),
                received: got.unwrap_or_else(|| "nothing yet".into()),
            }
        })
        .collect()
}

/// Each share's line in Activity, newest first: "Shared your profile",
/// "A new address for this share · 09:15", no amount.
pub fn activity_rows(book: &Book, clock: Clock) -> Vec<activity::Row> {
    book.newest_first()
        .into_iter()
        .map(|s| activity::Row {
            activity: None,
            request: None,
            share: Some(s.id),
            time: s.created,
            title: "Shared your profile".into(),
            line: format!("A new address for this share · {}", activity::time_of_day(activity::local(s.created, clock).1, clock)),
            amount: String::new(),
            fiat: None,
        })
        .collect()
}

/// What leads a payment's line when it came to a share's address: "Shared
/// 09:15" (the share's time, or its day before today).
pub fn shared_text(created: u64, clock: Clock) -> String {
    format!("Shared {}", activity::short_when(created, clock))
}

/// The title of a payment that came to a share's address.
pub const VIA: &str = "Via your profile";

/// What Swapped says once phones have touched: "Swapped with Sam" and
/// "Sam got your profile, and you got Sam's." when Sam's phone said it took
/// ours (`sure`); otherwise only what this phone knows, since its last
/// answer may not have reached Sam's.
pub fn swapped_words(name: &str, sure: bool) -> (String, String) {
    let line = if sure {
        format!("{name} got your profile, and you got {name}'s.")
    } else {
        format!("You got {name}'s profile and sent yours. If {name}'s phone doesn't say Swapped, touch again.")
    };
    (format!("Swapped with {name}"), line)
}

/// Settings' row for the profile, every word ready to draw.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingsRow {
    /// "Your profile", or "Add a shareable Fob contact" before there's a name.
    pub title: String,
    /// The line under it, in two parts so the app cuts only the name:
    /// "Henry" and " · share as a code". None without a name.
    pub name: Option<String>,
    pub suffix: Option<String>,
}

/// Settings' row: "Your profile" over "Henry · share as a code", or only
/// "Add a shareable Fob contact" while there's no name (a photo alone
/// doesn't make a contact others can save).
pub fn settings_row(profile: Option<&Profile>) -> SettingsRow {
    match profile.map(|p| p.name.trim()).filter(|n| !n.is_empty()) {
        Some(name) => SettingsRow { title: "Your profile".into(), name: Some(name.to_owned()), suffix: Some(" · share as a code".into()) },
        None => SettingsRow { title: "Add a shareable Fob contact".into(), name: None, suffix: None },
    }
}

/// A change a scanned profile would make to a contact: what it is now and
/// what it would be.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    /// "Name", "Photo", "Send address"
    pub label: String,
    /// "Sam", "No photo", "84Bv9 … 2kLmQ … 99fQ2", "None"
    pub now: String,
    /// "Henry", "Their new photo", "89Hn2 … Tq4Lp … 6wKeR"
    pub new: String,
}

/// A contact a scanned profile matches (the same address, else the same
/// name), and what taking the profile would change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Clash {
    pub contact: Contact,
    /// "You have a contact named Henry.", "This address is Sam's send
    /// address.", "Sam was paid at this address."
    pub why: String,
    /// Empty when the profile is what the contact has already.
    pub changes: Vec<Change>,
    /// The photo would change: draw both.
    pub photo_changes: bool,
    /// "Update Sam: their name, photo and send address change to the
    /// profile's." or "Nothing would change."
    pub ask: String,
    /// "Update Sam", or "Open Sam" when nothing would change.
    pub update: String,
    pub keep: String,
    /// "Add as new contact", for a name only: an address that's a
    /// contact's already is never given a second one.
    pub add_new: Option<String>,
}

/// A scanned profile, every word ready to draw ("Another Fob scans the code").
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scanned {
    pub code: uri::ProfileCode,
    pub initial: String,
    /// "89Hn2 … Tq4Lp … 6wKeR"
    pub address_short: String,
    /// "A Fob profile. The name and photo are Henry's own, not checked by Fob."
    pub note: String,
    /// "Pay Henry"
    pub pay: String,
    /// "Add contact"
    pub add: String,
    /// A contact it would change: asked about before anything is.
    pub clash: Option<Clash>,
}

fn changes(contact: &Contact, code: &uri::ProfileCode) -> (Vec<Change>, bool) {
    let mut changes = vec![];
    if contact.name != code.name {
        changes.push(Change { label: "Name".into(), now: contact.name.clone(), new: code.name.clone() });
    }
    // A profile without a photo leaves the contact's.
    let photo_changes = code.photo.is_some() && code.photo != contact.photo;
    if photo_changes {
        let now = if contact.photo.is_some() { "Their photo now" } else { "No photo" };
        changes.push(Change { label: "Photo".into(), now: now.into(), new: "The profile's photo".into() });
    }
    if contact.send_address.as_deref() != Some(code.address.as_str()) {
        let now = contact.send_address.as_deref().map(contacts::three_part).unwrap_or_else(|| "None".into());
        changes.push(Change { label: "Send address".into(), now, new: contacts::three_part(&code.address) });
    }
    (changes, photo_changes)
}

/// What scanning `code` shows: the profile, and the contact it would change
/// (the one with its address, else the oldest whose name is its name or
/// looks like it).
pub fn scanned(book: &contacts::Book, code: uri::ProfileCode) -> Scanned {
    let clash = book
        .owner(&code.address)
        .map(|c| {
            let why = if c.send_address.as_deref() == Some(code.address.as_str()) {
                format!("This address is {}'s send address.", c.name)
            } else {
                format!("{} was paid at this address.", c.name)
            };
            (c, why, false)
        })
        .or_else(|| {
            book.named(&code.name).map(|c| {
                let why = if c.name.to_lowercase() == code.name.to_lowercase() {
                    format!("You have a contact named {}.", c.name)
                } else {
                    format!("You have a contact whose name looks like this one: {}.", c.name)
                };
                (c, why, true)
            })
        })
        .map(|(contact, why, by_name)| {
            let (changes, photo_changes) = changes(contact, &code);
            let what: Vec<String> = changes.iter().map(|c| c.label.to_lowercase()).collect();
            let ask = match what.as_slice() {
                [] => format!("{} already has this profile's name, photo and send address.", contact.name),
                [one] => format!("Update {}? Their {one} changes to the profile's.", contact.name),
                [rest @ .., last] => format!("Update {}? Their {} and {last} change to the profile's.", contact.name, rest.join(", ")),
            };
            Clash {
                update: if changes.is_empty() { format!("Open {}", contact.name) } else { format!("Update {}", contact.name) },
                keep: "Keep as is".into(),
                add_new: by_name.then(|| "Add as new contact".into()),
                contact: contact.clone(),
                why,
                ask,
                changes,
                photo_changes,
            }
        });
    Scanned {
        initial: contacts::initial(&code.name),
        address_short: contacts::three_part(&code.address),
        note: format!("A Fob profile. The name and photo are {}'s own, not checked by Fob.", code.name),
        pay: format!("Pay {}", code.name),
        add: "Add contact".into(),
        clash,
        code,
    }
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
    use crate::{keys::Keys, network::Network, price::Record, wallet::{Direction, UNLOCK}};

    /// 2026-10-31 12:00 UTC.
    const NOW: u64 = 1_793_448_000;
    const DAY: u64 = 86_400;

    fn clock() -> Clock {
        Clock { now: NOW, utc_offset: 0, hours_24: true }
    }

    fn webp(len: usize) -> Vec<u8> {
        crate::contacts::test_webp(len, 96)
    }

    fn paid_in(subaddress: u32, amount: u64, time: u64, price: Option<u64>) -> Activity {
        Activity {
            tx: format!("in-{subaddress}-{time}"),
            direction: Direction::In,
            amount,
            fee: None,
            to: None,
            height: Some(100),
            time,
            confirmations: 20,
            needed: UNLOCK,
            receipt: None,
            receipt_json: None,
            price: price.map(|p| Record { currency: "USD".into(), price: p, provider: "Kraken".into(), at: time, daily: false }),
            subaddress: Some(subaddress),
            pieces: vec![amount],
            note: None,
            split: None,
            contact: None,
            contact_id: None,
            shared: None,
            parts: vec![],
        }
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
    fn a_share_needs_a_profile_and_an_address_of_its_own_and_goes_out_as_pressed() {
        let keys = Keys::generate(Network::Stagenet).keys;
        let mut book = Book::default();
        assert!(book.share(5, keys.address(5), ShareKind::CodeShown, NOW).is_err(), "no profile yet");
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
    fn shares_list_newest_first_with_what_came() {
        let keys = Keys::generate(Network::Stagenet).keys;
        let mut book = Book::default();
        book.set("Henry", None).unwrap();
        let old = book.share(3, keys.address(3), ShareKind::CodeShown, NOW - DAY * 3).unwrap();
        book.share(4, keys.address(4), ShareKind::CodeShown, NOW - 2 * 3600).unwrap();
        book.went_out(old.id, ShareKind::LinkCopied);
        let rows = vec![paid_in(4, 30_000_000_000, NOW - 3600, Some(166_000_000)), paid_in(4, 10_000_000_000, NOW - 1800, Some(165_000_000)), paid_in(3, 5_000_000_000, NOW, None)];
        let lines = lines(&book, &rows, clock());
        assert_eq!(lines.iter().map(|l| (l.title.as_str(), l.when.as_str(), l.received.as_str(), l.any)).collect::<Vec<_>>(), [
            ("Code shown", "Today 10:00", "+$6.63", true),
            ("Link copied", "28 Oct", "+0.005 XMR", true),
        ]);
        assert_eq!(super::lines(&book, &[], clock())[0].received, "nothing yet");
        let activity = activity_rows(&book, clock());
        assert_eq!(
            (activity[0].title.as_str(), activity[0].line.as_str(), activity[0].share, activity[0].amount.as_str()),
            ("Shared your profile", "A new address for this share · 10:00", Some(2), "")
        );
        assert_eq!(shared_text(NOW - 2 * 3600, clock()), "Shared 10:00");
        assert_eq!(shared_text(NOW - DAY * 3, clock()), "Shared 28 Oct");
    }

    #[test]
    fn a_share_by_touching_phones_names_whom_and_stays_one() {
        let keys = Keys::generate(Network::Stagenet).keys;
        let mut book = Book::default();
        book.set("Henry", None).unwrap();
        let sam = book.share(7, keys.address(7), ShareKind::TouchedPhones { name: Some("Sam".into()) }, NOW).unwrap();
        let nobody = book.share(8, keys.address(8), ShareKind::TouchedPhones { name: None }, NOW + 1).unwrap();
        // Copy link on its code again doesn't make it a link.
        assert!(!book.went_out(sam.id, ShareKind::LinkCopied));
        let titles: Vec<String> = lines(&book, &[], clock()).into_iter().map(|l| l.title).collect();
        assert_eq!(titles, ["Touched phones", "Touched phones with Sam"]);
        let json = serde_json::to_string(&book).unwrap();
        assert!(json.contains(r#""kind":{"touched_phones":{"name":"Sam"}}"#), "{json}");
        assert_eq!(serde_json::from_str::<Book>(&json).unwrap(), book);
        assert_eq!(book.get(nobody.id).unwrap().kind, ShareKind::TouchedPhones { name: None });
        // A code's share that phones then touched on: kept as the touch.
        let shown = book.share(9, keys.address(9), ShareKind::CodeShown, NOW + 2).unwrap();
        assert_eq!(book.touched(shown.id, None).unwrap().kind, ShareKind::TouchedPhones { name: None });
        assert!(book.touched(shown.id, None).is_none(), "kept already");
        assert_eq!(book.touched(shown.id, Some("Jo".into())).unwrap().kind, ShareKind::TouchedPhones { name: Some("Jo".into()) });
        assert!(book.touched(shown.id, None).is_none(), "the name stays");
        assert!(book.touched(99, None).is_none());
        assert_eq!(swapped_words("Sam", true), ("Swapped with Sam".into(), "Sam got your profile, and you got Sam's.".into()));
        assert_eq!(swapped_words("Sam", false).1, "You got Sam's profile and sent yours. If Sam's phone doesn't say Swapped, touch again.");
    }

    #[test]
    fn settings_says_whose_profile_it_is() {
        let mut book = Book::default();
        let row = settings_row(book.profile());
        assert_eq!((row.title.as_str(), row.name, row.suffix), ("Add a shareable Fob contact", None, None), "no second line");
        book.set("Henry", None).unwrap();
        let row = settings_row(book.profile());
        assert_eq!((row.title.as_str(), row.name.as_deref(), row.suffix.as_deref()), ("Your profile", Some("Henry"), Some(" · share as a code")));
        // A long name comes whole: the app cuts it, never the suffix.
        book.set("Henrietta Montgomery-Fairweather", None).unwrap();
        assert_eq!(settings_row(book.profile()).name.as_deref(), Some("Henrietta Montgomery-Fairweather"));
        // An old file's blank name, with a photo or not, isn't a contact.
        let blank = Profile { name: " ".into(), photo: None, name_in_requests: true };
        assert_eq!(settings_row(Some(&blank)).title, "Add a shareable Fob contact");
    }

    fn code(name: &str, address: &str, photo: Option<Vec<u8>>) -> uri::ProfileCode {
        uri::ProfileCode { address: address.into(), name: name.into(), photo }
    }

    #[test]
    fn a_scanned_profile_that_would_change_a_contact_asks_first() {
        let keys = Keys::generate(Network::Stagenet).keys;
        let (henry_at, sam_at, other_at) = (keys.address(1), keys.address(2), keys.address(3));
        let mut book = contacts::Book::default();
        // Nobody: just the profile.
        let s = scanned(&book, code("Henry", &henry_at, Some(webp(50))));
        assert_eq!((s.pay.as_str(), s.add.as_str(), s.clash.is_none()), ("Pay Henry", "Add contact", true));
        assert_eq!(s.note, "A Fob profile. The name and photo are Henry's own, not checked by Fob.");
        assert_eq!(s.address_short, contacts::three_part(&henry_at));
        let sam = book
            .add(Network::Stagenet, contacts::Draft { name: "Sam".into(), note: Some("Lunch".into()), send_address: Some(sam_at.clone()), also_paid_at: vec![other_at.clone()], ..Default::default() }, NOW)
            .unwrap();

        // The same name, another address: the hijack the question is for.
        let c = scanned(&book, code("sam", &henry_at, Some(webp(50)))).clash.unwrap();
        assert_eq!((c.contact.id, c.why.as_str()), (sam.id, "You have a contact named Sam."));
        assert_eq!(
            c.changes,
            [
                Change { label: "Name".into(), now: "Sam".into(), new: "sam".into() },
                Change { label: "Photo".into(), now: "No photo".into(), new: "The profile's photo".into() },
                Change { label: "Send address".into(), now: contacts::three_part(&sam_at), new: contacts::three_part(&henry_at) },
            ]
        );
        assert!(c.photo_changes);
        assert_eq!(c.ask, "Update Sam? Their name, photo and send address change to the profile's.");
        assert_eq!((c.update.as_str(), c.keep.as_str(), c.add_new.as_deref()), ("Update Sam", "Keep as is", Some("Add as new contact")));
        // A name made to look like Sam's: a zero-width space in it, a Cyrillic "Ѕ", full-width letters.
        for lookalike in ["Sa\u{200b}m", "\u{405}am", "Ｓａｍ", "S\u{430}m"] {
            let c = scanned(&book, code(lookalike, &henry_at, None)).clash.expect(lookalike);
            assert_eq!(c.contact.id, sam.id, "{lookalike}");
        }
        let c = scanned(&book, code("\u{405}am", &henry_at, None)).clash.unwrap();
        assert_eq!(c.why, "You have a contact whose name looks like this one: Sam.");
        assert!(scanned(&book, code("Pam", &henry_at, None)).clash.is_none());

        // The same address under another name: the address wins, and no second contact gets it.
        let c = scanned(&book, code("Samuel", &sam_at, None)).clash.unwrap();
        assert_eq!((c.why.as_str(), c.ask.as_str(), c.photo_changes), ("This address is Sam's send address.", "Update Sam? Their name changes to the profile's.", false));
        assert_eq!(c.add_new, None);
        let c = scanned(&book, code("Sam", &other_at, None)).clash.unwrap();
        assert_eq!((c.why.as_str(), c.changes.len()), ("Sam was paid at this address.", 1));
        // Nothing to change.
        let c = scanned(&book, code("Sam", &sam_at, None)).clash.unwrap();
        assert!(c.changes.is_empty());
        assert_eq!((c.update.as_str(), c.ask.as_str()), ("Open Sam", "Sam already has this profile's name, photo and send address."));

        // Taken: name, photo and send address; the note and paid-at stay, and
        // a profile without a photo keeps theirs.
        let draft = updated(&sam, &code("Samuel", &other_at, Some(webp(60))));
        let edited = book.edit(Network::Stagenet, sam.id, draft, &|_| false).unwrap();
        assert_eq!((edited.name.as_str(), edited.note.as_deref(), edited.photo.clone()), ("Samuel", Some("Lunch"), Some(webp(60))));
        assert_eq!((edited.send_address.as_deref(), edited.also_paid_at.len()), (Some(other_at.as_str()), 0), "moved out of paid at");
        assert_eq!(updated(&edited, &code("Sam", &sam_at, None)).photo, Some(webp(60)));
        let fresh = new_contact(&code("Jo", &henry_at, Some(webp(40))));
        assert_eq!((fresh.name.as_str(), fresh.send_address.as_deref(), fresh.photo), ("Jo", Some(henry_at.as_str()), Some(webp(40))));
    }
}
