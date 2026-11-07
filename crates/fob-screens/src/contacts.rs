//! Contacts: the people this wallet pays, kept in the wallet file on this
//! phone only. A contact has a name, maybe a note and a small photo, maybe
//! a send address (where you pay them, which you set), and the addresses
//! of codes named for them that you paid (`also_paid_at`), which are never
//! taken as their send address.
//!
//! Everything a contact screen says is written here from the contacts and
//! the payments; the apps only draw it.

use serde::{Deserialize, Serialize};

use crate::{activity::{self, Activity, Direction}, clock::{self, Clock}, error::ScreenError, error::Result, requests};
use fob_monero::{address::{self, short_title}, amount, network::Network};
use fob_codes::uri;

pub use fob_codes::names::{NAME_MAX, NOTE_MAX, PHOTO_MAX, PHOTO_SIDE_MAX, check_name, check_note, check_photo, clean, clean_name, photo_kept, skeleton};

/// How many contacts Send's sheet lists.
pub const ON_SEND: usize = 3;

/// Someone this wallet pays.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contact {
    pub id: u64,
    /// 1 to [`NAME_MAX`] characters, trimmed.
    pub name: String,
    #[serde(default)]
    pub note: Option<String>,
    /// A WebP of at most [`PHOTO_MAX`] bytes (hex in the file).
    #[serde(default, with = "photo_hex")]
    pub photo: Option<Vec<u8>>,
    /// Where you pay them; set by you (scanned or pasted), never from a code
    /// that only named them.
    #[serde(default)]
    pub send_address: Option<String>,
    /// Addresses of codes named for them that you paid: kept so those
    /// payments show as theirs, never used to pay them.
    #[serde(default)]
    pub also_paid_at: Vec<String>,
    /// When it was made, seconds since the epoch.
    #[serde(default)]
    pub created: u64,
    /// Half set up: scanned from a profile code on its own, so they have
    /// no address of yours yet. Meeting them (touch or both codes) clears it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub half: bool,
}

impl Contact {
    /// Every address that's theirs: the send address, then the others.
    pub fn addresses(&self) -> impl Iterator<Item = &str> {
        self.send_address.as_deref().into_iter().chain(self.also_paid_at.iter().map(String::as_str))
    }

    pub fn owns(&self, address: &str) -> bool {
        self.addresses().any(|a| a == address)
    }

    /// Makes `address` the send address: out of paid at if it was there,
    /// and the old one, when `paid`, into it (it was theirs when paid).
    fn replace_send_address(&mut self, address: Option<String>, paid: &dyn Fn(&str) -> bool) {
        if let Some(old) = self.send_address.take().filter(|old| Some(old) != address.as_ref() && paid(old) && !self.also_paid_at.contains(old)) {
            self.also_paid_at.push(old);
        }
        if let Some(address) = &address {
            self.also_paid_at.retain(|a| a != address);
        }
        self.send_address = address;
    }

    /// The letter on their avatar without a photo: "S".
    pub fn initial(&self) -> String {
        initial(&self.name)
    }
}

/// The first letter of a name, upper case, for an avatar: "S", "?" for none.
pub fn initial(name: &str) -> String {
    name.trim().chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_else(|| "?".into())
}

/// The photo as hex in the wallet file's JSON, not a list of numbers.
pub(crate) mod photo_hex {
    use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error};

    pub fn serialize<S: Serializer>(photo: &Option<Vec<u8>>, s: S) -> Result<S::Ok, S::Error> {
        photo.as_ref().map(hex::encode).serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Vec<u8>>, D::Error> {
        Option::<String>::deserialize(d)?.map(|h| hex::decode(h).map_err(D::Error::custom)).transpose()
    }
}

/// What the edit card saves: a new contact, or the changes to one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Draft {
    pub name: String,
    pub note: Option<String>,
    pub photo: Option<Vec<u8>>,
    /// As typed, scanned or pasted: an address or a payment code; blank for
    /// none.
    pub send_address: Option<String>,
    /// For a new contact only (saved after paying a code named for them).
    pub also_paid_at: Vec<String>,
}

/// The address in a scanned or pasted text (an address, or a payment code,
/// whose address is taken), checked for `network`.
pub fn check_address(network: Network, text: &str) -> Result<String> {
    Ok(uri::parse_payment(network, text)?.address)
}

/// Every contact, kept in the wallet file.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Book {
    #[serde(default)]
    contacts: Vec<Contact>,
    /// The next contact's id: ids aren't used again after a delete, so a
    /// screen still showing a deleted contact can't change another.
    #[serde(default)]
    next_id: u64,
}

impl Book {
    pub fn all(&self) -> &[Contact] {
        &self.contacts
    }

    pub fn get(&self, id: u64) -> Option<&Contact> {
        self.contacts.iter().find(|c| c.id == id)
    }

    fn get_mut(&mut self, id: u64) -> Result<&mut Contact> {
        self.contacts.iter_mut().find(|c| c.id == id).ok_or(ScreenError::ContactDeleted)
    }

    /// The contact `address` belongs to (send address or paid at), the
    /// oldest if several.
    pub fn owner(&self, address: &str) -> Option<&Contact> {
        self.contacts.iter().find(|c| c.owns(address))
    }

    /// The contact named `name`, or whose name looks like it ([`skeleton`]:
    /// any case, an invisible character, a Cyrillic "Ѕ" for an "S"), the
    /// oldest if several.
    pub fn named(&self, name: &str) -> Option<&Contact> {
        let name = skeleton(name);
        self.contacts.iter().find(|c| skeleton(&c.name) == name)
    }

    /// Adds a contact; every field checked, the send address for `network`.
    pub fn add(&mut self, network: Network, draft: Draft, now: u64) -> Result<Contact> {
        let name = check_name(&draft.name)?;
        let note = check_note(draft.note.as_deref())?;
        if let Some(photo) = &draft.photo {
            check_photo(photo)?;
        }
        let send_address = send_address(network, draft.send_address.as_deref())?;
        let mut also_paid_at: Vec<String> = vec![];
        for text in &draft.also_paid_at {
            let address = check_address(network, text)?;
            if Some(&address) != send_address.as_ref() && !also_paid_at.contains(&address) {
                also_paid_at.push(address);
            }
        }
        let id = self.next_id.max(self.contacts.iter().map(|c| c.id + 1).max().unwrap_or(1)).max(1);
        self.next_id = id + 1;
        let contact = Contact { id, name, note, photo: draft.photo, send_address, also_paid_at, created: now, half: false };
        self.contacts.push(contact.clone());
        Ok(contact)
    }

    /// Sets or clears a contact's half set up mark.
    pub fn set_half(&mut self, id: u64, half: bool) -> Result<Contact> {
        let contact = self.get_mut(id)?;
        contact.half = half;
        Ok(contact.clone())
    }

    /// Changes a contact's name, note, photo and send address (its paid-at
    /// addresses stay). A send address that was one of them moves out of
    /// them; the one it replaces, when it was `paid`, moves in, so what was
    /// sent there stays theirs.
    pub fn edit(&mut self, network: Network, id: u64, draft: Draft, paid: &dyn Fn(&str) -> bool) -> Result<Contact> {
        let name = check_name(&draft.name)?;
        let note = check_note(draft.note.as_deref())?;
        if let Some(photo) = &draft.photo {
            check_photo(photo)?;
        }
        let send_address = send_address(network, draft.send_address.as_deref())?;
        let contact = self.get_mut(id)?;
        contact.name = name;
        contact.note = note;
        contact.photo = draft.photo;
        contact.replace_send_address(send_address, paid);
        Ok(contact.clone())
    }

    /// Sets only the send address (Change on the contact's card), keeping
    /// the one it replaces as paid at when it was `paid`.
    pub fn set_send_address(&mut self, network: Network, id: u64, text: &str, paid: &dyn Fn(&str) -> bool) -> Result<Contact> {
        let address = check_address(network, text)?;
        let contact = self.get_mut(id)?;
        contact.replace_send_address(Some(address), paid);
        Ok(contact.clone())
    }

    /// Keeps `address` (paid, from a code named for them) with the contact;
    /// nothing changes when it's already theirs.
    pub fn add_paid_at(&mut self, network: Network, id: u64, address: &str) -> Result<Contact> {
        let address = check_address(network, address)?;
        let contact = self.get_mut(id)?;
        if !contact.owns(&address) {
            contact.also_paid_at.push(address);
        }
        Ok(contact.clone())
    }

    /// Drops every photo [`check_photo`] refuses (one saved before it read
    /// headers). Returns whether any was.
    pub fn drop_bad_photos(&mut self) -> bool {
        let mut dropped = false;
        for contact in &mut self.contacts {
            if contact.photo.as_deref().is_some_and(|p| check_photo(p).is_err()) {
                contact.photo = None;
                dropped = true;
            }
        }
        dropped
    }

    /// Deletes a contact. Payments to its addresses stay, shown by address
    /// again. Returns whether there was one.
    pub fn delete(&mut self, id: u64) -> bool {
        let before = self.contacts.len();
        self.contacts.retain(|c| c.id != id);
        self.contacts.len() != before
    }
}

fn send_address(network: Network, text: Option<&str>) -> Result<Option<String>> {
    text.map(str::trim).filter(|t| !t.is_empty()).map(|t| check_address(network, t)).transpose()
}

/// Payments this wallet sent to `contact` (to any of its addresses), newest
/// first. Splits pay this wallet, so they're never anyone's.
fn paid_to<'a>(contact: &Contact, rows: &'a [Activity]) -> Vec<&'a Activity> {
    let mut paid: Vec<&Activity> =
        rows.iter().filter(|a| a.direction == Direction::Out && a.split.is_none() && a.to.as_deref().is_some_and(|to| contact.owns(to))).collect();
    paid.sort_by_key(|a| std::cmp::Reverse(a.time));
    paid
}

/// A contact in a list, ready to draw.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub id: u64,
    pub name: String,
    /// The avatar's letter, without a photo.
    pub initial: String,
    pub photo: Option<Vec<u8>>,
    pub send_address: Option<String>,
    pub line: ContactLine,
}

/// What's said about a contact under their name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContactLine {
    Note(String),
    NotPaid,
    /// On Contacts: their note, if any, and when they were last paid.
    Paid { note: Option<String>, day: clock::Day },
    /// On Send's sheet: the last payment.
    Last(activity::PaidLine),
    /// On Share's sheet: how many of their requests were paid and how many wait.
    Requests { paid: usize, waiting: usize },
}

pub(crate) fn row(contact: &Contact, line: ContactLine) -> Row {
    Row {
        id: contact.id,
        name: contact.name.clone(),
        initial: contact.initial(),
        photo: contact.photo.clone(),
        send_address: contact.send_address.clone(),
        line,
    }
}

/// Contacts, by name, those whose name or note has `search` in it (any
/// case; blank for all), each with its line about the last payment to it.
pub fn list(book: &Book, rows: &[Activity], search: &str, clock: Clock) -> Vec<Row> {
    let search = search.trim().to_lowercase();
    let mut found: Vec<&Contact> = book
        .all()
        .iter()
        .filter(|c| search.is_empty() || c.name.to_lowercase().contains(&search) || c.note.as_deref().is_some_and(|n| n.to_lowercase().contains(&search)))
        .collect();
    found.sort_by_cached_key(|c| (c.name.to_lowercase(), c.id));
    found
        .into_iter()
        .map(|c| {
            let last = paid_to(c, rows).first().map(|p| clock::day_name(p.time, clock));
            let line = match (&c.note, last) {
                (note, Some(day)) => ContactLine::Paid { note: note.clone(), day },
                (Some(note), None) => ContactLine::Note(note.clone()),
                (None, None) => ContactLine::NotPaid,
            };
            row(c, line)
        })
        .collect()
}

/// Send's sheet: contacts with a send address, the most recently paid
/// first (never paid ones after, by name), at most [`ON_SEND`].
pub fn on_send(book: &Book, rows: &[Activity], clock: Clock) -> Vec<Row> {
    let mut found: Vec<(&Contact, Option<&Activity>)> =
        book.all().iter().filter(|c| c.send_address.is_some()).map(|c| (c, paid_to(c, rows).first().copied())).collect();
    found.sort_by_cached_key(|(c, last)| (std::cmp::Reverse(last.map(|l| l.time)), c.name.to_lowercase(), c.id));
    found
        .into_iter()
        .take(ON_SEND)
        .map(|(c, last)| {
            let line = match last {
                Some(last) => ContactLine::Last(activity::last_line(last, clock)),
                None => c.note.clone().map_or(ContactLine::NotPaid, ContactLine::Note),
            };
            row(c, line)
        })
        .collect()
}

/// Addresses this wallet paid that are no contact's, the most recently paid
/// first, for "Paid, not saved": each address cut to its ends ("84Bv …
/// 9fQ2"), with when it was paid and how much.
pub fn unsaved(book: &Book, rows: &[Activity], clock: Clock) -> Vec<activity::PaidBefore> {
    let mut sent: Vec<(&Activity, &str)> =
        rows.iter().filter(|a| a.direction == Direction::Out && a.split.is_none()).filter_map(|a| Some((a, a.to.as_deref()?))).collect();
    sent.sort_by_key(|(a, _)| std::cmp::Reverse(a.time));
    let mut seen: Vec<&str> = vec![];
    let mut out = vec![];
    for (last, to) in sent {
        if seen.contains(&to) || book.owner(to).is_some() {
            continue;
        }
        seen.push(to);
        out.push(activity::PaidBefore {
            address: to.to_owned(),
            name: short_title(to),
            line: activity::PaidLine::PaidDayAmount { day: clock::day_name(last.time, clock), amount: activity::paid_amount(last) },
        });
    }
    out
}

/// An address on a contact's card: "86Uxm … 3EkYw … e7kQa", its first,
/// middle and last five (the groups Receive marks).
pub fn three_part(address: &str) -> String {
    let groups = address::address_groups(address);
    if groups.len() < 4 {
        return address.trim().to_owned();
    }
    let middle = &groups[groups.len() / 2].text;
    format!("{} … {middle} … {}", groups[0].text, groups[groups.len() - 1].text)
}

/// One of the addresses a contact was also paid at, on their card.
#[derive(Clone, Debug, PartialEq)]
pub struct PaidAt {
    pub address: String,
    /// "84Bv9 … 2kLmQ … 99fQ2"
    pub short: String,
    pub times: PaidTimes,
}

/// How often an address was paid.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PaidTimes {
    Never,
    Once(clock::Day),
    Many { times: usize, last: clock::Day },
}

/// What a payment on a contact's card is called.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PaidTitle {
    /// The payment's note, else the shop on its receipt.
    Named(String),
    Sent,
    From(String),
}

/// A payment to a contact, or from them (to a request made for them), on
/// their card.
#[derive(Clone, Debug, PartialEq)]
pub struct Paid {
    pub activity: Activity,
    pub title: PaidTitle,
    pub day: clock::Day,
    /// For one from them, what it was for.
    pub note: Option<String>,
    /// "−0.1204", "+0.5"
    pub amount: String,
    /// Its worth when it moved: "$20.00".
    pub fiat: Option<String>,
}

/// A contact's card.
#[derive(Clone, Debug, PartialEq)]
pub struct Card {
    pub contact: Contact,
    pub initial: String,
    /// "86Uxm … 3EkYw … e7kQa"
    pub send_short: Option<String>,
    /// Their requests, newest first; the section shows only with one.
    pub requests: Vec<requests::CardLine>,
    pub also_paid_at: Vec<PaidAt>,
    /// Something has come from them.
    pub from_them: bool,
    pub paid: Vec<Paid>,
    pub half: bool,
}

/// `contact`'s card, from their requests and the payments in `rows`.
pub fn card(contact: &Contact, requests: &requests::Book, rows: &[Activity], clock: Clock) -> Card {
    let paid = paid_to(contact, rows);
    let also_paid_at = contact
        .also_paid_at
        .iter()
        .map(|address| {
            let to_it: Vec<&&Activity> = paid.iter().filter(|p| p.to.as_deref() == Some(address)).collect();
            let times = match (to_it.len(), to_it.first()) {
                (0, _) | (_, None) => PaidTimes::Never,
                (1, Some(last)) => PaidTimes::Once(clock::day_name(last.time, clock)),
                (times, Some(last)) => PaidTimes::Many { times, last: clock::day_name(last.time, clock) },
            };
            PaidAt { address: address.clone(), short: three_part(address), times }
        })
        .collect();
    // Payments to their requests: from them.
    let mut history: Vec<&Activity> =
        paid.iter().copied().chain(rows.iter().filter(|a| a.direction == Direction::In && a.contact_id == Some(contact.id))).collect();
    history.sort_by_key(|a| std::cmp::Reverse(a.time));
    let from_them = history.iter().any(|a| a.direction == Direction::In);
    let paid_rows: Vec<Paid> = history
        .iter()
        .map(|p| match p.direction {
            Direction::In => Paid {
                activity: (*p).clone(),
                title: PaidTitle::From(contact.name.clone()),
                day: clock::day_name(p.time, clock),
                note: p.note.clone(),
                amount: format!("+{}", amount::format_short(p.amount)),
                fiat: p.price.as_ref().map(|_| activity::paid_amount(p)),
            },
            Direction::Out => Paid {
                activity: (*p).clone(),
                title: p.note.clone().or_else(|| p.receipt.as_ref().map(|r| r.merchant.clone())).map_or(PaidTitle::Sent, PaidTitle::Named),
                day: clock::day_name(p.time, clock),
                note: None,
                amount: format!("−{}", amount::format_short(p.amount)),
                fiat: p.price.as_ref().map(|_| activity::paid_amount(p)),
            },
        })
        .collect();
    Card {
        initial: contact.initial(),
        send_short: contact.send_address.as_deref().map(three_part),
        requests: requests::on_card(contact.id, requests, rows, clock),
        also_paid_at,
        from_them,
        half: contact.half,
        paid: paid_rows,
        contact: contact.clone(),
    }
}

/// A send address found by the scanner (or pasted), for its sheet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    pub address: String,
    /// "87Wq2 … Lm4Zx … 2bNcT"
    pub short: String,
    pub warning: Option<FoundWarning>,
}

/// What finding a send address does to the one a contact has.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FoundWarning {
    Same,
    /// Replaces this address, cut to its ends.
    Replaces(String),
}

/// What a scanned or pasted `text` gives as a send address, where `current`
/// is the one it has (if any). A payment code's address is taken.
pub fn found(network: Network, current: Option<&str>, text: &str) -> Result<Found> {
    let address = check_address(network, text)?;
    let warning = match current.map(str::trim).filter(|c| !c.is_empty()) {
        Some(current) if current == address => Some(FoundWarning::Same),
        Some(current) => Some(FoundWarning::Replaces(address::short(current))),
        None => None,
    };
    Ok(Found { short: three_part(&address), warning, address })
}

/// What the Paid screen offers after a payment (Round 4 › "Sent: a small
/// Save contact button"; Round 5 › "Paid a code named Sam").
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Offer {
    /// The address paid.
    pub address: String,
    /// The contact the code's name looks like ([`Book::named`]), though the
    /// address isn't one of theirs. Only said, never offered as a one-tap
    /// Add to: the name is whatever the code wrote, so anyone's code can be
    /// named for a contact, and filing its address with them would make
    /// every later payment to it read as theirs in Activity.
    pub namesake: Option<Contact>,
    /// The name on the code, when it had one.
    pub name: Option<String>,
    /// New contact, filled.
    pub draft: Draft,
}

/// New contact for `address`, paid with a code named `name` (if it had one)
/// for `note`: a name from a code only names them, so the address is one
/// they were paid at; without one, it's their send address.
pub fn new_contact(address: &str, name: Option<&str>, note: Option<&str>) -> Draft {
    let note = note.map(clean).map(|n| n.trim().chars().take(NOTE_MAX).collect::<String>().trim_end().to_owned()).filter(|n| !n.is_empty());
    match name.and_then(clean_name) {
        Some(name) => Draft { name, note, photo: None, send_address: None, also_paid_at: vec![address.to_owned()] },
        None => Draft { name: String::new(), note, photo: None, send_address: Some(address.to_owned()), also_paid_at: vec![] },
    }
}

/// What to offer after paying `address` with a code named `name` (if any)
/// for `note`: nothing when the address is already a contact's; otherwise
/// Save as a contact, saying which contact the name looks like when one
/// does. A code carries nothing that proves who made it (its name is only
/// text), so an address nobody has is never filed with an existing contact
/// from here; if it really is theirs, it's set on their card, where the
/// address is shown.
pub fn offer(book: &Book, address: &str, name: Option<&str>, note: Option<&str>) -> Option<Offer> {
    if book.owner(address).is_some() {
        return None;
    }
    let draft = new_contact(address, name, note);
    let name = name.and_then(clean_name);
    let namesake = name.as_deref().and_then(|n| book.named(n)).cloned();
    Some(Offer { address: address.to_owned(), namesake, name, draft })
}

/// What happens to the addresses of codes named for someone, under the send
/// address on New contact.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PaidAtNote {
    /// This one address, cut to three parts.
    One(String),
    Many(usize),
}

pub fn paid_at_note(addresses: &[String]) -> Option<PaidAtNote> {
    match addresses {
        [] => None,
        [one] => Some(PaidAtNote::One(three_part(one))),
        many => Some(PaidAtNote::Many(many.len())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fob_vault::keys::Keys;
    use fob_monero::network::UNLOCK;

    /// 2026-10-31 12:00 UTC.
    const NOW: u64 = 1_793_448_000;
    const DAY: u64 = 86_400;

    fn clock() -> Clock {
        Clock { now: NOW, utc_offset: 0, hours_24: true }
    }

    fn address(n: u32) -> String {
        Keys::generate(Network::Stagenet).keys.address(n)
    }

    fn webp(len: usize) -> Vec<u8> {
        fob_codes::names::test_webp(len, 96)
    }

    fn draft(name: &str) -> Draft {
        Draft { name: name.into(), ..Draft::default() }
    }

    fn paid_out(to: &str, amount: u64, time: u64) -> Activity {
        paid(to, amount, time)
    }

    fn paid(to: &str, amount: u64, time: u64) -> Activity {
        Activity {
            not_confirmed: false,
            released: false,
            tx: format!("{to}-{time}"),
            direction: Direction::Out,
            amount,
            fee: Some(1),
            to: Some(to.into()),
            height: Some(1),
            time,
            confirmations: 20,
            needed: UNLOCK,
            receipt: None,
            receipt_json: None,
            price: None,
            subaddress: None,
            pieces: vec![],
            note: None,
            split: None,
            to_self: false,
            contact: None,
            contact_id: None,
            shared: None,
            parts: vec![],
        }
    }

    #[test]
    fn what_draws_nothing_is_taken_out_and_lookalike_names_are_one_name() {
        // Zero-width and invisible characters, separators, the Arabic letter mark: gone, so "Sa​m" is "Sam".
        for hidden in ['\u{200b}', '\u{200c}', '\u{2060}', '\u{feff}', '\u{61c}', '\u{2028}', '\u{2029}', '\u{180e}', '\u{ad}', '\u{e0041}'] {
            assert_eq!(clean_name(&format!("Sa{hidden}m")).as_deref(), Some("Sam"), "U+{:04X}", hidden as u32);
        }
        assert_eq!(check_note(Some("Lunch\u{2028}(verified)")).unwrap().as_deref(), Some("Lunch(verified)"));
        // A joiner inside an emoji, a variation selector and a flag's tags stay: they make one emoji of several.
        let family = "👨\u{200d}👩\u{200d}👧";
        assert_eq!(clean_name(&format!("Sam {family}")).as_deref(), Some(&*format!("Sam {family}")));
        assert_eq!(clean_name("Sam ❤\u{fe0f}").as_deref(), Some("Sam ❤\u{fe0f}"));
        let england = "🏴\u{e0067}\u{e0062}\u{e0065}\u{e006e}\u{e0067}\u{e007f}";
        assert_eq!(clean_name(england).as_deref(), Some(england));
        // A joiner between letters does nothing visible: gone.
        assert_eq!(clean_name("S\u{200d}am").as_deref(), Some("Sam"));
        // Names that look alike: one skeleton.
        let sam = skeleton("Sam");
        for lookalike in ["sam", " SAM ", "Sa\u{200b}m", "\u{405}am", "S\u{430}m", "Ｓａｍ", "5am".replace('5', "\u{455}").as_str()] {
            assert_eq!(skeleton(lookalike), sam, "{lookalike:?}");
        }
        assert_ne!(skeleton("Pam"), sam);
        let mut book = Book::default();
        let id = book.add(Network::Stagenet, draft("Sam"), NOW).unwrap().id;
        assert_eq!(book.named("\u{405}am").map(|c| c.id), Some(id));
    }

    #[test]
    fn a_changed_send_address_that_was_paid_stays_theirs() {
        let mut book = Book::default();
        let (old, new, newer, unpaid) = (address(1), address(2), address(3), address(4));
        let sam = book.add(Network::Stagenet, Draft { send_address: Some(old.clone()), ..draft("Sam") }, NOW).unwrap();
        let paid = |a: &str| a == old || a == newer;
        // Changed on the card: the old one, paid twice, is kept as paid at, and both payments stay on the card.
        let changed = book.set_send_address(Network::Stagenet, sam.id, &new, &paid).unwrap();
        assert_eq!((changed.send_address.as_deref(), changed.also_paid_at.clone()), (Some(new.as_str()), vec![old.clone()]));
        let rows = [paid_out(&old, 1, NOW - DAY), paid_out(&old, 2, NOW - 2 * DAY)];
        let shown = card(&changed, &requests::Book::default(), &rows, clock());
        assert_eq!((shown.paid.len(), shown.from_them), (2, false));
        assert_eq!(shown.also_paid_at[0].times, PaidTimes::Many { times: 2, last: clock::Day::Yesterday });
        assert!(unsaved(&book, &rows, clock()).is_empty(), "not \"Paid, not saved\"");
        // Never paid: just replaced. Cleared in an edit: kept the same way when paid.
        book.set_send_address(Network::Stagenet, sam.id, &unpaid, &paid).unwrap();
        let edited = book.set_send_address(Network::Stagenet, sam.id, &newer, &paid).unwrap();
        assert_eq!(edited.also_paid_at, vec![old.clone()]);
        let cleared = book.edit(Network::Stagenet, sam.id, Draft { send_address: None, ..draft("Sam") }, &paid).unwrap();
        assert_eq!((cleared.send_address, cleared.also_paid_at), (None, vec![old.clone(), newer.clone()]));
        // Set back to one it was paid at: out of paid at again.
        let back = book.set_send_address(Network::Stagenet, sam.id, &old, &paid).unwrap();
        assert_eq!((back.send_address.as_deref(), back.also_paid_at), (Some(old.as_str()), vec![newer]));
    }

    #[test]
    fn a_name_from_a_code_is_cleaned_and_cut() {
        assert_eq!(clean_name(" Sam\u{7}\n ").as_deref(), Some("Sam"));
        assert_eq!(clean_name("\u{1b}[31m").as_deref(), Some("[31m"));
        assert_eq!(clean_name(" \t "), None);
        assert_eq!(clean_name("\u{202e}maS\u{2066}").as_deref(), Some("maS"));
        assert_eq!(check_name("\u{202e}").ok(), None);
        assert_eq!(clean_name(&format!("{} tail", "x".repeat(39))).as_deref(), Some(&*"x".repeat(39)));
    }

    #[test]
    fn contacts_are_added_edited_and_deleted() {
        let mut book = Book::default();
        let sam_address = address(1);
        let sam = book
            .add(
                Network::Stagenet,
                Draft {
                    note: Some("Lunch money".into()),
                    send_address: Some(format!("monero:{sam_address}?tx_amount=1")),
                    photo: Some(webp(900)),
                    ..draft(" Sam ")
                },
                NOW,
            )
            .unwrap();
        assert_eq!((sam.id, sam.name.as_str(), sam.send_address.as_deref(), sam.created), (1, "Sam", Some(sam_address.as_str()), NOW));
        // Another network's address, a bad photo, no name: refused, nothing added.
        let mainnet = Keys::generate(Network::Mainnet).keys.address(0);
        assert!(matches!(book.add(Network::Stagenet, Draft { send_address: Some(mainnet), ..draft("Mum") }, NOW), Err(ScreenError::Code(fob_codes::error::CodeError::Monero(fob_monero::error::MoneroError::WrongNetwork { .. })))));
        assert!(book.add(Network::Stagenet, Draft { photo: Some(webp(PHOTO_MAX + 1)), ..draft("Mum") }, NOW).is_err());
        assert!(book.add(Network::Stagenet, draft(""), NOW).is_err());
        assert_eq!(book.all().len(), 1);
        // The same send address twice is allowed.
        let twin = book.add(Network::Stagenet, Draft { send_address: Some(sam_address.clone()), ..draft("Sam at work") }, NOW).unwrap();
        assert_eq!(twin.id, 2);
        let edited = book.edit(Network::Stagenet, sam.id, Draft { photo: None, send_address: None, ..draft("Samuel") }, &|_| false).unwrap();
        assert_eq!((edited.name.as_str(), edited.note, edited.photo, edited.send_address), ("Samuel", None, None, None));
        assert!(book.delete(twin.id));
        assert!(!book.delete(twin.id));
        // Ids aren't used again.
        assert_eq!(book.add(Network::Stagenet, draft("Mum"), NOW).unwrap().id, 3);
        assert!(book.edit(Network::Stagenet, twin.id, draft("Ghost"), &|_| false).is_err());
    }

    #[test]
    fn paid_at_addresses_are_never_the_send_address() {
        let mut book = Book::default();
        let (send, code) = (address(1), address(2));
        let sam = book
            .add(
                Network::Stagenet,
                Draft { send_address: Some(send.clone()), also_paid_at: vec![code.clone(), send.clone(), code.clone()], ..draft("Sam") },
                NOW,
            )
            .unwrap();
        assert_eq!(sam.also_paid_at, std::slice::from_ref(&code));
        assert_eq!(book.add_paid_at(Network::Stagenet, sam.id, &code).unwrap().also_paid_at.len(), 1);
        assert_eq!(book.owner(&code).map(|c| c.id), Some(sam.id));
        // Scanned as the send address, it moves out of the others.
        let changed = book.set_send_address(Network::Stagenet, sam.id, &code, &|_| false).unwrap();
        assert_eq!((changed.send_address.as_deref(), changed.also_paid_at.len()), (Some(code.as_str()), 0));
        assert_eq!(book.named("sAM").map(|c| c.id), Some(sam.id));
        assert!(book.named("Sam2").is_none());
    }

    #[test]
    fn the_book_round_trips_and_photos_are_hex() {
        let mut book = Book::default();
        book.add(Network::Stagenet, Draft { photo: Some(webp(40)), note: Some("n".into()), ..draft("Sam") }, NOW).unwrap();
        let json = serde_json::to_string(&book).unwrap();
        assert!(json.contains(&hex::encode(webp(40))), "{json}");
        assert_eq!(serde_json::from_str::<Book>(&json).unwrap(), book);
        // A contact saved before a field existed still reads.
        let old: Contact = serde_json::from_str(r#"{"id":4,"name":"Mum"}"#).unwrap();
        assert_eq!((old.note, old.photo, old.send_address, old.also_paid_at.len(), old.created), (None, None, None, 0, 0));
    }

    #[test]
    fn addresses_show_their_first_middle_and_last_five() {
        let a = address(1);
        let groups = address::address_groups(&a);
        assert_eq!(three_part(&a), format!("{} … {} … {}", groups[0].text, groups[9].text, groups[18].text));
        assert_eq!(three_part("86Ux"), "86Ux");
    }
}
