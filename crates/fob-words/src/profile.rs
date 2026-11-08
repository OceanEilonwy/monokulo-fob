//! Your profile and profiles scanned, in words.

use fob_codes::uri;
use fob_screens::{
    clock::{Day, When},
    contacts::Contact,
    profile::{self, ChangeKind, ChangeValue, ClashWhy, ShareKind},
};

use crate::clock;

/// "Code shown", "Link copied", "Link shared", "Touched phones with Sam".
pub fn share_title(kind: &ShareKind) -> String {
    match kind {
        ShareKind::CodeShown => "Code shown".into(),
        ShareKind::LinkCopied => "Link copied".into(),
        ShareKind::LinkShared => "Link shared".into(),
        ShareKind::TouchedPhones { name: Some(name) } => format!("Touched phones with {name}"),
        ShareKind::TouchedPhones { name: None } => "Touched phones".into(),
        ShareKind::Met { name } => format!("Met {name}"),
    }
}

/// A share on Your profile as it's said.
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

pub fn line(l: profile::Line) -> Line {
    Line { id: l.id, address: l.address, title: share_title(&l.kind), kind: l.kind, when: share_when(&l.when), any: l.received.is_some(), received: l.received.unwrap_or_else(|| "nothing yet".into()) }
}

/// A share's time on Your profile: with the hour only today and yesterday.
fn share_when(when: &When) -> String {
    match &when.day {
        Day::Date(date) => date.clone(),
        _ => clock::when(when),
    }
}

/// What Swapped says once phones have touched: "Swapped with Sam" and
/// "Sam got your profile, and you got Sam's." when Sam's phone said it took
/// ours (`sure`); otherwise only what this phone knows, since its last
/// answer may not have reached Sam's.
pub fn swapped_words(name: &str, sure: bool) -> (String, String) {
    if name.is_empty() {
        return ("Swapped".into(), "You got their address and sent yours. They didn't share a name.".into());
    }
    let line = if sure {
        format!("{name} got your profile, and you got {name}'s.")
    } else {
        format!("You got {name}'s profile and sent yours. If {name}'s phone doesn't say Swapped, touch again.")
    };
    (format!("Swapped with {name}"), line)
}

/// Settings' row for the profile as it's said.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingsRow {
    /// "Your profile", or "Add a shareable Fob contact" before there's a name.
    pub title: String,
    /// The line under it, in two parts so the app cuts only the name:
    /// "Henry" and " · share as a code". None without a name.
    pub name: Option<String>,
    pub suffix: Option<String>,
}

pub fn settings_row(row: profile::SettingsRow) -> SettingsRow {
    match row.name {
        Some(name) => SettingsRow { title: "Your profile".into(), name: Some(name), suffix: Some(" · share as a code".into()) },
        None => SettingsRow { title: "Add a shareable Fob contact".into(), name: None, suffix: None },
    }
}

/// A change a scanned profile would make, as it's said.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub kind: ChangeKind,
    /// "Name", "Photo", "Send address"
    pub label: String,
    /// "Sam", "No photo", "84Bv9 … 2kLmQ … 99fQ2", "None"
    pub now: String,
    /// "Henry", "The profile's photo", "89Hn2 … Tq4Lp … 6wKeR"
    pub new: String,
}

fn label(kind: ChangeKind) -> &'static str {
    match kind {
        ChangeKind::Name => "Name",
        ChangeKind::Photo => "Photo",
        ChangeKind::SendAddress => "Send address",
    }
}

fn value(value: ChangeValue) -> String {
    match value {
        ChangeValue::Text(text) => text,
        ChangeValue::Nothing => "None".into(),
        ChangeValue::NoPhoto => "No photo".into(),
        ChangeValue::TheirPhoto => "Their photo now".into(),
        ChangeValue::ProfilePhoto => "The profile's photo".into(),
    }
}

/// A contact a scanned profile matches, as it's said.
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
    /// "Update Sam? Their name, photo and send address change to the
    /// profile's.", or that they have it already.
    pub ask: String,
    /// "Update Sam", or "Open Sam" when nothing would change.
    pub update: String,
    pub keep: String,
    /// "Add as new contact", for a name only.
    pub add_new: Option<String>,
}

fn clash(c: profile::Clash) -> Clash {
    let name = &c.contact.name;
    let why = match c.why {
        ClashWhy::SendAddress => format!("This address is {name}'s send address."),
        ClashWhy::PaidAt => format!("{name} was paid at this address."),
        ClashWhy::SameName => format!("You have a contact named {name}."),
        ClashWhy::NameLooksLike => format!("You have a contact whose name looks like this one: {name}."),
    };
    let what: Vec<String> = c.changes.iter().map(|c| label(c.kind).to_lowercase()).collect();
    let ask = match what.as_slice() {
        [] => format!("{name} already has this profile's name, photo and send address."),
        [one] => format!("Update {name}? Their {one} changes to the profile's."),
        [rest @ .., last] => format!("Update {name}? Their {} and {last} change to the profile's.", rest.join(", ")),
    };
    let update = if c.changes.is_empty() { format!("Open {name}") } else { format!("Update {name}") };
    Clash {
        why,
        ask,
        update,
        keep: "Keep as is".into(),
        add_new: c.can_add_new.then(|| "Add as new contact".into()),
        photo_changes: c.photo_changes,
        changes: c.changes.into_iter().map(|c| Change { kind: c.kind, label: label(c.kind).into(), now: value(c.now), new: value(c.new) }).collect(),
        contact: c.contact,
    }
}

/// A scanned profile as it's said ("Another Fob scans the code").
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
    pub clash: Option<Clash>,
}

pub fn scanned(s: profile::Scanned) -> Scanned {
    let name = &s.code.name;
    Scanned {
        initial: s.initial,
        address_short: s.address_short,
        note: if name.is_empty() { "A Fob user who didn't share a name.".into() } else { format!("A Fob profile. The name and photo are {name}'s own, not checked by Fob.") },
        pay: if name.is_empty() { "Pay them".into() } else { format!("Pay {name}") },
        add: "Add contact".into(),
        clash: s.clash.map(clash),
        code: s.code,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fob_monero::{network::{Network, UNLOCK}, fiat::Record};
    use fob_screens::{activity::{Activity, Direction}, clock::Clock, contacts, profile::{Book, Profile, new_contact, updated}};
    use fob_vault::keys::Keys;

    fn lines(book: &Book, rows: &[Activity], clock: Clock) -> Vec<Line> {
        profile::lines(book, rows, clock).into_iter().map(line).collect()
    }

    fn activity_rows(book: &Book, clock: Clock) -> Vec<crate::activity::Row> {
        profile::activity_rows(book, clock).into_iter().map(crate::activity::row).collect()
    }

    fn settings_row(p: Option<&profile::Profile>) -> SettingsRow {
        super::settings_row(profile::settings_row(p))
    }

    /// What leads a payment's line when it came to a share's address.
    fn shared_text(created: u64, clock: Clock) -> String {
        let paid = Activity { shared: Some(created), ..paid_in(1, 1, NOW, None) };
        let row = fob_screens::activity::by_day(vec![paid], vec![], clock).remove(0).rows.remove(0);
        crate::activity::row(row).line.split(" · ").next().unwrap().to_owned()
    }

    fn scanned(book: &contacts::Book, code: uri::ProfileCode) -> Scanned {
        super::scanned(profile::scanned(book, code))
    }

    /// 2026-10-31 12:00 UTC.
    const NOW: u64 = 1_793_448_000;
    const DAY: u64 = 86_400;

    fn clock() -> Clock {
        Clock { now: NOW, utc_offset: 0, hours_24: true }
    }

    fn webp(len: usize) -> Vec<u8> {
        fob_codes::names::test_webp(len, 96)
    }

    fn paid_in(subaddress: u32, amount: u64, time: u64, price: Option<u64>) -> Activity {
        Activity {
            not_confirmed: false,
            released: false,
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
            to_self: false,
            contact: None,
            contact_id: None,
            shared: None,
            parts: vec![],
        }
    }







    fn code(name: &str, address: &str, photo: Option<Vec<u8>>) -> uri::ProfileCode {
        uri::ProfileCode { address: address.into(), name: name.into(), photo, reply: false }
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
        assert_eq!(self::lines(&book, &[], clock())[0].received, "nothing yet");
        // Activity lists only a share someone took: a code shown, or a link sent, isn't one.
        assert!(activity_rows(&book, clock()).is_empty());
        let met = book.met(2, "Sam".into()).unwrap();
        assert!(book.met(2, "Other".into()).is_none(), "a meeting stays the one it was");
        assert_eq!(share_title(&met.kind), "Met Sam");
        let activity = activity_rows(&book, clock());
        assert_eq!(activity.len(), 1);
        assert_eq!(
            (activity[0].title.as_str(), activity[0].line.as_str(), activity[0].share, activity[0].amount.as_str()),
            ("Met Sam", "A new address for this share · 10:00", Some(2), "")
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
                Change { kind: ChangeKind::Name, label: "Name".into(), now: "Sam".into(), new: "sam".into() },
                Change { kind: ChangeKind::Photo, label: "Photo".into(), now: "No photo".into(), new: "The profile's photo".into() },
                Change { kind: ChangeKind::SendAddress, label: "Send address".into(), now: contacts::three_part(&sam_at), new: contacts::three_part(&henry_at) },
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
