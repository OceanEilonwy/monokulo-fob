//! Contacts: the people this wallet pays, kept in the wallet file on this
//! phone only. A contact has a name, maybe a note and a small photo, maybe
//! a send address (where you pay them, which you set), and the addresses
//! of codes named for them that you paid (`also_paid_at`), which are never
//! taken as their send address.
//!
//! Everything a contact screen says is written here from the contacts and
//! the payments; the apps only draw it.

use serde::{Deserialize, Serialize};

use crate::{
    activity::{self, Clock, short_address},
    amount,
    error::{FobError, Result},
    network::Network,
    requests, uri,
    wallet::{Activity, Direction},
};

/// The most characters a contact's name has (a name from a payment code
/// is cut to it).
pub const NAME_MAX: usize = 40;
/// The most characters a contact's note has.
pub const NOTE_MAX: usize = 80;
/// The most bytes a contact's photo takes: a 96 × 96 WebP, its quality
/// stepped down (then made smaller) by the app until it fits.
pub const PHOTO_MAX: usize = 1536;
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
}

impl Contact {
    /// Every address that's theirs: the send address, then the others.
    pub fn addresses(&self) -> impl Iterator<Item = &str> {
        self.send_address.as_deref().into_iter().chain(self.also_paid_at.iter().map(String::as_str))
    }

    pub fn owns(&self, address: &str) -> bool {
        self.addresses().any(|a| a == address)
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

/// Characters a name or note keeps: no control characters (a line break,
/// a bell) and none of the marks that turn text round on screen.
pub(crate) fn shown(c: &char) -> bool {
    !c.is_control() && !matches!(c, '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
}

/// A name with control characters taken out and spaces trimmed, cut to
/// [`NAME_MAX`] characters; `None` when nothing is left. For names that
/// come from payment codes.
pub fn clean_name(text: &str) -> Option<String> {
    let kept: String = text.chars().filter(shown).collect();
    let cut: String = kept.trim().chars().take(NAME_MAX).collect();
    let cut = cut.trim_end();
    (!cut.is_empty()).then(|| cut.to_owned())
}

/// A typed name, checked: 1 to [`NAME_MAX`] characters once trimmed (control
/// characters, a pasted line break, dropped).
pub fn check_name(text: &str) -> Result<String> {
    let kept: String = text.chars().filter(shown).collect();
    let name = kept.trim();
    match name.chars().count() {
        0 => Err(FobError::Contact("A contact needs a name.".into())),
        n if n > NAME_MAX => Err(FobError::Contact(format!("A name can be up to {NAME_MAX} characters."))),
        _ => Ok(name.to_owned()),
    }
}

/// A typed note, checked: blank is none, at most [`NOTE_MAX`] characters.
pub fn check_note(text: Option<&str>) -> Result<Option<String>> {
    let kept: Option<String> = text.map(|t| t.chars().filter(shown).collect());
    let Some(note) = kept.as_deref().map(str::trim).filter(|n| !n.is_empty()) else { return Ok(None) };
    if note.chars().count() > NOTE_MAX {
        return Err(FobError::Contact(format!("A note can be up to {NOTE_MAX} characters.")));
    }
    Ok(Some(note.to_owned()))
}

/// The most pixels a side of a photo has: the app makes them 96 at most,
/// and every avatar is drawn smaller than this.
pub const PHOTO_SIDE_MAX: u32 = 128;

/// A little-endian number in `bytes` (at most four of them).
fn le(bytes: &[u8]) -> u32 {
    bytes.iter().rev().fold(0, |n, b| n << 8 | u32::from(*b))
}

/// The size a lossy (`VP8 `) image says it is: a key frame's, after its
/// start code.
fn vp8_size(body: &[u8]) -> Option<(u32, u32)> {
    let key_frame = body.first()? & 1 == 0;
    let start = body.get(3..6)? == [0x9d, 0x01, 0x2a];
    let (width, height) = (le(body.get(6..8)?) & 0x3fff, le(body.get(8..10)?) & 0x3fff);
    (key_frame && start).then_some((width, height))
}

/// The size a lossless (`VP8L`) image says it is: 14 bits each, less one.
fn vp8l_size(body: &[u8]) -> Option<(u32, u32)> {
    let bits = le(body.get(1..5)?);
    // The signature, then version 0 in the top three bits.
    (body[0] == 0x2f && bits >> 29 == 0).then_some(((bits & 0x3fff) + 1, (bits >> 14 & 0x3fff) + 1))
}

/// The pixel size of a WebP Fob keeps, from its headers: one still image,
/// lossy or lossless, alone or after an extended header (`VP8X`) and its
/// alpha. Anything else (animation, metadata, a chunk Fob doesn't know,
/// sizes that don't add up) is `None`, so no decoder is handed it.
fn webp_size(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.get(..4)? != b"RIFF" || bytes.get(8..12)? != b"WEBP" || le(&bytes[4..8]) as usize != bytes.len() - 8 {
        return None;
    }
    let (mut rest, mut canvas, mut image) = (&bytes[12..], None, None);
    let mut first = true;
    while !rest.is_empty() {
        let tag = rest.get(..4)?;
        let size = le(rest.get(4..8)?) as usize;
        let body = rest.get(8..8 + size)?;
        // A chunk of odd size is padded to even.
        rest = rest.get(8 + size + (size & 1)..)?;
        match tag {
            // The extended header: flags (no animation), three bytes kept, then the canvas, 24 bits a side, less one.
            b"VP8X" if first && size >= 10 && body[0] & 0x02 == 0 => canvas = Some((le(&body[4..7]) + 1, le(&body[7..10]) + 1)),
            b"ALPH" if canvas.is_some() && image.is_none() => {}
            b"VP8 " if image.is_none() => image = Some(vp8_size(body)?),
            b"VP8L" if image.is_none() => image = Some(vp8l_size(body)?),
            _ => return None,
        }
        first = false;
    }
    let image = image?;
    canvas.is_none_or(|c| c == image).then_some(image)
}

/// A photo Fob keeps: a still WebP of at most [`PHOTO_MAX`] bytes and
/// [`PHOTO_SIDE_MAX`] pixels a side, its headers read before anything
/// decodes it (a few bytes can claim a picture that takes a gigabyte to
/// draw).
pub fn check_photo(bytes: &[u8]) -> Result<()> {
    if bytes.len() > PHOTO_MAX {
        return Err(FobError::Contact(format!("That photo is {} bytes; a contact's photo can be at most {PHOTO_MAX}.", bytes.len())));
    }
    let Some((width, height)) = webp_size(bytes) else { return Err(FobError::Contact("That photo isn't a WebP image Fob can show.".into())) };
    if width == 0 || height == 0 || width > PHOTO_SIDE_MAX || height > PHOTO_SIDE_MAX {
        return Err(FobError::Contact(format!("That photo is {width} × {height}; a contact's photo can be at most {PHOTO_SIDE_MAX} × {PHOTO_SIDE_MAX}.")));
    }
    Ok(())
}

/// A valid lossless WebP header of `side` pixels a side, padded to `len`
/// bytes (even, at least 26; an odd length is never a valid WebP), for
/// tests.
#[cfg(test)]
pub(crate) fn test_webp(len: usize, side: u32) -> Vec<u8> {
    let size = (len - 20) & !1;
    let bits = (side - 1) | (side - 1) << 14;
    let mut bytes = b"RIFF".to_vec();
    bytes.extend_from_slice(&(len as u32 - 8).to_le_bytes());
    bytes.extend_from_slice(b"WEBPVP8L");
    bytes.extend_from_slice(&(size as u32).to_le_bytes());
    bytes.push(0x2f);
    bytes.extend_from_slice(&bits.to_le_bytes());
    let header = bytes.len();
    bytes.extend((header..len).map(|i| (i * 37 % 251) as u8));
    bytes
}

/// Whether the app's encoding of a photo is one Fob keeps, for its loop of
/// lowering the quality, then the size, until it is.
pub fn photo_fits(bytes: &[u8]) -> bool {
    check_photo(bytes).is_ok()
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
        self.contacts.iter_mut().find(|c| c.id == id).ok_or_else(|| FobError::Contact("That contact was deleted.".into()))
    }

    /// The contact `address` belongs to (send address or paid at), the
    /// oldest if several.
    pub fn owner(&self, address: &str) -> Option<&Contact> {
        self.contacts.iter().find(|c| c.owns(address))
    }

    /// The contact named `name`, ignoring case (the oldest if several).
    pub fn named(&self, name: &str) -> Option<&Contact> {
        let name = name.trim().to_lowercase();
        self.contacts.iter().find(|c| c.name.to_lowercase() == name)
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
        let contact = Contact { id, name, note, photo: draft.photo, send_address, also_paid_at, created: now };
        self.contacts.push(contact.clone());
        Ok(contact)
    }

    /// Changes a contact's name, note, photo and send address (its paid-at
    /// addresses stay). A send address that was one of them moves out of
    /// them.
    pub fn edit(&mut self, network: Network, id: u64, draft: Draft) -> Result<Contact> {
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
        if let Some(address) = &send_address {
            contact.also_paid_at.retain(|a| a != address);
        }
        contact.send_address = send_address;
        Ok(contact.clone())
    }

    /// Sets only the send address (Change on the contact's card).
    pub fn set_send_address(&mut self, network: Network, id: u64, text: &str) -> Result<Contact> {
        let address = check_address(network, text)?;
        let contact = self.get_mut(id)?;
        contact.also_paid_at.retain(|a| *a != address);
        contact.send_address = Some(address);
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

/// "today", "yesterday", "27 Oct", after "paid".
fn paid_day(time: u64, clock: Clock) -> String {
    let day = activity::day_name(time, clock);
    match day.as_str() {
        "Today" | "Yesterday" => day.to_lowercase(),
        _ => day,
    }
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
    /// On Contacts: "Lunch money · paid 27 Oct", "Paid 3 Oct", the note,
    /// or "Not paid yet". On Send's sheet: the last payment, as Paid
    /// before had it ("Lunch · 27 Oct", "Today · $24.50"), else the note.
    pub line: String,
}

pub(crate) fn row(contact: &Contact, line: String) -> Row {
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
            let last = paid_to(c, rows).first().map(|p| paid_day(p.time, clock));
            let line = match (&c.note, last) {
                (Some(note), Some(day)) => format!("{note} · paid {day}"),
                (None, Some(day)) => format!("Paid {day}"),
                (Some(note), None) => note.clone(),
                (None, None) => "Not paid yet".into(),
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
                Some(last) => activity::last_line(last, clock),
                None => c.note.clone().unwrap_or_else(|| "Not paid yet".into()),
            };
            row(c, line)
        })
        .collect()
}

/// Addresses this wallet paid that are no contact's, the most recently paid
/// first, for "Paid, not saved": each address cut to its ends ("84Bv …
/// 9fQ2") and "Paid 25 Oct · $33.21".
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
            name: short_address(to),
            line: format!("Paid {} · {}", paid_day(last.time, clock), activity::paid_amount(last)),
        });
    }
    out
}

/// An address on a contact's card: "86Uxm … 3EkYw … e7kQa", its first,
/// middle and last five (the groups Receive marks).
pub fn three_part(address: &str) -> String {
    let groups = uri::address_groups(address);
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
    /// "Not their send address · paid once, 25 Oct"
    pub line: String,
}

/// A payment to a contact, or from them (to a request made for them), on
/// their card.
#[derive(Clone, Debug, PartialEq)]
pub struct Paid {
    pub activity: Activity,
    /// The payment's note, else the shop on its receipt, else "Sent"; "From
    /// Sam" for one from them.
    pub title: String,
    /// "Today", "27 Oct"; "Concert tickets · Today" for one from them.
    pub when: String,
    /// "−0.1204", "+0.5"
    pub amount: String,
    /// Its worth when it moved: "$20.00".
    pub fiat: Option<String>,
}

/// A contact's card, every word ready to draw.
#[derive(Clone, Debug, PartialEq)]
pub struct Card {
    pub contact: Contact,
    pub initial: String,
    /// "Send address · you pay here", or "No send address yet".
    pub send_label: String,
    /// "86Uxm … 3EkYw … e7kQa"
    pub send_short: Option<String>,
    /// "Change", or "Add" without one.
    pub send_button: String,
    /// "Requests · Sam pays you", between the send address and Also paid at.
    pub requests_title: String,
    /// "3 addresses", beside it.
    pub requests_note: String,
    /// Their requests, newest first; the section shows only with one.
    pub requests: Vec<requests::CardLine>,
    pub also_paid_at: Vec<PaidAt>,
    /// "codes named Sam", beside "Also paid at".
    pub also_note: String,
    /// "Paid to Sam", or "Payments with Sam" once something came from them.
    pub paid_title: String,
    pub paid: Vec<Paid>,
    /// "Nothing paid to Sam yet.", when nothing was.
    pub paid_empty: Option<String>,
}

/// `contact`'s card, from their requests and the payments in `rows`.
pub fn card(contact: &Contact, requests: &requests::Book, rows: &[Activity], clock: Clock) -> Card {
    let name = &contact.name;
    let paid = paid_to(contact, rows);
    let theirs = requests::on_card(contact.id, requests, rows, clock);
    let also_paid_at = contact
        .also_paid_at
        .iter()
        .map(|address| {
            let to_it: Vec<&&Activity> = paid.iter().filter(|p| p.to.as_deref() == Some(address)).collect();
            let times = match (to_it.len(), to_it.first()) {
                (0, _) | (_, None) => "not paid yet".to_owned(),
                (1, Some(last)) => format!("paid once, {}", paid_day(last.time, clock)),
                (n, Some(last)) => format!("paid {n} times, last {}", paid_day(last.time, clock)),
            };
            PaidAt { address: address.clone(), short: three_part(address), line: format!("Not their send address · {times}") }
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
                title: format!("From {name}"),
                when: [p.note.clone(), Some(activity::day_name(p.time, clock))].into_iter().flatten().collect::<Vec<_>>().join(" · "),
                amount: format!("+{}", amount::format_short(p.amount)),
                fiat: p.price.as_ref().map(|_| activity::paid_amount(p)),
            },
            Direction::Out => Paid {
                activity: (*p).clone(),
                title: p.note.clone().or_else(|| p.receipt.as_ref().map(|r| r.merchant.clone())).unwrap_or_else(|| "Sent".into()),
                when: activity::day_name(p.time, clock),
                amount: format!("−{}", amount::format_short(p.amount)),
                fiat: p.price.as_ref().map(|_| activity::paid_amount(p)),
            },
        })
        .collect();
    Card {
        initial: contact.initial(),
        send_label: if contact.send_address.is_some() { "Send address · you pay here" } else { "No send address yet" }.into(),
        send_short: contact.send_address.as_deref().map(three_part),
        send_button: if contact.send_address.is_some() { "Change" } else { "Add" }.into(),
        requests_title: format!("Requests · {name} pays you"),
        requests_note: requests::addresses_note(theirs.len()),
        requests: theirs,
        also_paid_at,
        also_note: format!("codes named {name}"),
        paid_title: if from_them { format!("Payments with {name}") } else { format!("Paid to {name}") },
        paid_empty: paid_rows.is_empty().then(|| format!("Nothing paid to {name} yet.")),
        paid: paid_rows,
        contact: contact.clone(),
    }
}

/// "Sam's send address", "Their send address" (no name yet).
fn whose(name: Option<&str>) -> (String, String) {
    match name.map(str::trim).filter(|n| !n.is_empty()) {
        Some(name) => (format!("{name}'s"), name.to_owned()),
        None => ("their".into(), "them".into()),
    }
}

fn capitalised(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

/// The words of the edit card for `name` (as typed so far).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditTexts {
    /// "New contact", "Edit contact"
    pub title: String,
    /// Under the send address.
    pub address_hint: String,
    /// "Delete Sam?"
    pub delete_title: String,
    pub delete_body: String,
}

pub fn edit_texts(name: &str, new: bool) -> EditTexts {
    let (_, them) = whose(Some(name));
    let shown = if name.trim().is_empty() { "this contact".to_owned() } else { name.trim().to_owned() };
    EditTexts {
        title: if new { "New contact" } else { "Edit contact" }.into(),
        address_hint: if new {
            "Scan their code or paste it. Leave it empty if they only pay you.".into()
        } else {
            format!("Scan or paste to change it. Payments already sent stay with {them}.")
        },
        delete_title: format!("Delete {shown}?"),
        delete_body: "The name and note go. Payments to this address stay in Activity, shown by address again. This can't be undone.".into(),
    }
}

/// The scanner for a send address: its title and what to point at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScanTexts {
    /// "Sam's send address", "Their send address"
    pub title: String,
    /// "Point at the code on Sam's phone (their Receive screen) or any
    /// Monero address code"
    pub hint: String,
}

pub fn scan_texts(name: Option<&str>) -> ScanTexts {
    let (theirs, _) = whose(name);
    ScanTexts {
        title: capitalised(&format!("{theirs} send address")),
        hint: format!("Point at the code on {theirs} phone (their Receive screen) or any Monero address code"),
    }
}

/// A send address found by the scanner (or pasted), for its sheet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    pub address: String,
    /// "Found an address"
    pub title: String,
    /// "87Wq2 … Lm4Zx … 2bNcT"
    pub short: String,
    /// When it would replace one: "This replaces Sam's current send address
    /// (86Uxm … e7kQa). Check the pattern with Sam."; when it's the same:
    /// "That's already Sam's send address."
    pub warning: Option<String>,
}

/// What a scanned or pasted `text` gives as `name`'s send address, where
/// `current` is the one it has (if any). A payment code's address is taken.
pub fn found(network: Network, name: Option<&str>, current: Option<&str>, text: &str) -> Result<Found> {
    let address = check_address(network, text)?;
    let (theirs, them) = whose(name);
    let warning = match current.map(str::trim).filter(|c| !c.is_empty()) {
        Some(current) if current == address => Some(capitalised(&format!("that's already {theirs} send address."))),
        Some(current) => Some(format!("This replaces {theirs} current send address ({}). Check the pattern with {them}.", uri::short(current))),
        None => None,
    };
    Ok(Found { short: three_part(&address), title: "Found an address".into(), warning, address })
}

/// What the Paid screen offers after a payment (Round 4 › "Sent: a small
/// Save contact button"; Round 5 › "Paid a code named Sam").
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Offer {
    /// The address paid.
    pub address: String,
    /// The contact the code's name matches (any case): the button keeps
    /// the address with them.
    pub add_to: Option<Contact>,
    /// "Add to Sam", "Save as a contact"
    pub button: String,
    /// "A different Sam? New contact", with Add to.
    pub other: Option<String>,
    /// "to a code named “Sam”", for a code with a name.
    pub named: Option<String>,
    /// Under the buttons, for a code with a name.
    pub footnote: Option<String>,
    /// New contact, filled.
    pub draft: Draft,
}

/// Under a named code's buttons.
pub const NAMED_FOOTNOTE: &str =
    "The name comes from their code, not checked by Fob. The address is kept with the contact as one they were paid at, not as their send address.";

/// New contact for `address`, paid with a code named `name` (if it had one)
/// for `note`: a name from a code only names them, so the address is one
/// they were paid at; without one, it's their send address.
pub fn new_contact(address: &str, name: Option<&str>, note: Option<&str>) -> Draft {
    let note = note.map(str::trim).filter(|n| !n.is_empty()).map(|n| n.chars().filter(shown).take(NOTE_MAX).collect::<String>());
    match name.and_then(clean_name) {
        Some(name) => Draft { name, note, photo: None, send_address: None, also_paid_at: vec![address.to_owned()] },
        None => Draft { name: String::new(), note, photo: None, send_address: Some(address.to_owned()), also_paid_at: vec![] },
    }
}

/// What to offer after paying `address` with a code named `name` (if any)
/// for `note`: nothing when the address is already a contact's; Add to the
/// contact the name matches; otherwise Save as a contact.
pub fn offer(book: &Book, address: &str, name: Option<&str>, note: Option<&str>) -> Option<Offer> {
    if book.owner(address).is_some() {
        return None;
    }
    let draft = new_contact(address, name, note);
    let name = name.and_then(clean_name);
    let add_to = name.as_deref().and_then(|n| book.named(n)).cloned();
    Some(Offer {
        address: address.to_owned(),
        button: add_to.as_ref().map(|c| format!("Add to {}", c.name)).unwrap_or_else(|| "Save as a contact".into()),
        other: add_to.as_ref().zip(name.as_ref()).map(|(_, n)| format!("A different {n}? New contact")),
        named: name.as_ref().map(|n| format!("to a code named “{n}”")),
        footnote: name.as_ref().map(|_| NAMED_FOOTNOTE.to_owned()),
        add_to,
        draft,
    })
}

/// Under the send address on New contact, saved with the addresses of
/// codes named for them: what happens to them.
pub fn paid_at_note(addresses: &[String]) -> Option<String> {
    match addresses {
        [] => None,
        [one] => Some(format!("Paid at {} from their code: kept with them, not as their send address.", three_part(one))),
        many => Some(format!("Paid at {} addresses from their codes: kept with them, not as their send address.", many.len())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{keys::Keys, receipt::ReceiptView, wallet::UNLOCK};

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
        crate::contacts::test_webp(len, 96)
    }

    fn draft(name: &str) -> Draft {
        Draft { name: name.into(), ..Draft::default() }
    }

    fn paid(to: &str, amount: u64, time: u64) -> Activity {
        Activity {
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
            contact: None,
            contact_id: None,
            shared: None,
        }
    }

    #[test]
    fn names_notes_and_photos_are_checked() {
        assert_eq!(check_name("  Sam \n").unwrap(), "Sam");
        assert!(check_name("   ").is_err());
        assert!(check_name(&"a".repeat(41)).is_err());
        // Characters, not bytes: 40 accented letters are fine.
        assert_eq!(check_name(&"é".repeat(40)).unwrap().chars().count(), 40);
        assert_eq!(check_note(Some("  ")).unwrap(), None);
        assert_eq!(check_note(Some(" Lunch money ")).unwrap().as_deref(), Some("Lunch money"));
        assert!(check_note(Some(&"n".repeat(81))).is_err());
        assert!(photo_fits(&webp(PHOTO_MAX)));
        assert!(!photo_fits(&webp(PHOTO_MAX + 1)));
        // Only a header, saying the old check let it through.
        assert!(!photo_fits(b"RIFF\0\0\0\0WEBPVP8 \x07\x07\x07\x07"));
        let mut png = webp(100);
        png[..4].copy_from_slice(b"\x89PNG");
        assert!(!photo_fits(&png));
        assert!(!photo_fits(b"RIFF"));
    }

    /// Encoded by libwebp: 16 × 16, lossy, lossless, and lossy with alpha
    /// (an extended header, then ALPH and VP8).
    const LOSSY: &str = "524946463600000057454250565038202a000000f001009d012a1000100002c04c25a00274ba01f800044c0000fedf803fdf7a03793fff929aff1b71d800";
    const LOSSLESS: &str = "524946461e000000574542505650384c110000002f0fc003000750a82297a2ff8188e87f0000";
    const ALPHA: &str = "524946465a00000057454250565038580a000000100000000f00000f0000414c50480a000000010750c088084444ff03565038202a000000f001009d012a1000100002c04c25a00274ba01f800044c0000fedf803fdf7a03793fff929aff1b71d800";

    #[test]
    fn photos_are_checked_by_the_size_their_headers_claim() {
        for real in [LOSSY, LOSSLESS, ALPHA] {
            let bytes = hex::decode(real).unwrap();
            assert_eq!(webp_size(&bytes), Some((16, 16)), "{real}");
            assert!(photo_fits(&bytes));
        }
        // Each form claiming 16383 × 16383 in a few bytes (a gigabyte to decode): refused.
        let forged = |real: &str, at: usize, side: &[u8]| {
            let mut bytes = hex::decode(real).unwrap();
            bytes[at..at + side.len()].copy_from_slice(side);
            bytes
        };
        let lossy = forged(LOSSY, 26, &[0xff, 0x3f, 0xff, 0x3f]);
        assert_eq!(webp_size(&lossy), Some((16383, 16383)));
        assert!(!photo_fits(&lossy));
        let lossless = forged(LOSSLESS, 21, &[0xff, 0xff, 0xff, 0x0f]);
        assert_eq!(webp_size(&lossless), Some((16384, 16384)));
        assert!(!photo_fits(&lossless));
        // An extended header's canvas, and the image inside it, must agree, and both be small.
        let canvas = forged(ALPHA, 24, &[0xff, 0xff, 0xff, 0xff, 0xff, 0xff]);
        assert!(webp_size(&canvas).is_none());
        let both = forged(&hex::encode(&canvas), 58, &[0xff, 0x3f, 0xff, 0x3f]);
        assert!(webp_size(&both).is_none(), "16383 against a 16777216 canvas");
        // 129 a side is one too many; 128 is fine.
        assert!(photo_fits(&test_webp(200, 128)));
        assert!(!photo_fits(&test_webp(200, 129)));
        // Animation, metadata, an unknown chunk, a second image, a chunk running past the end, a wrong RIFF size: refused.
        let mut animated = hex::decode(ALPHA).unwrap();
        animated[20] |= 0x02;
        assert!(!photo_fits(&animated));
        let mut exif = hex::decode(LOSSLESS).unwrap();
        exif.extend_from_slice(b"EXIF\x02\0\0\0ab");
        let riff = (exif.len() as u32 - 8).to_le_bytes();
        exif[4..8].copy_from_slice(&riff);
        assert!(!photo_fits(&exif));
        let mut twice = hex::decode(LOSSLESS).unwrap();
        twice.extend_from_slice(&hex::decode(LOSSLESS).unwrap()[12..]);
        let riff = (twice.len() as u32 - 8).to_le_bytes();
        twice[4..8].copy_from_slice(&riff);
        assert!(!photo_fits(&twice));
        let mut long = hex::decode(LOSSLESS).unwrap();
        long[16] = 0x40;
        assert!(!photo_fits(&long));
        let mut riff = hex::decode(LOSSY).unwrap();
        riff[4] += 2;
        assert!(!photo_fits(&riff));
        // A lossy frame that isn't a key frame, a lossless one of another version.
        assert!(!photo_fits(&forged(LOSSY, 20, &[0x31])));
        assert!(!photo_fits(&forged(LOSSLESS, 24, &[0x2f])));
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
        assert!(matches!(book.add(Network::Stagenet, Draft { send_address: Some(mainnet), ..draft("Mum") }, NOW), Err(FobError::WrongNetwork { .. })));
        assert!(book.add(Network::Stagenet, Draft { photo: Some(webp(2000)), ..draft("Mum") }, NOW).is_err());
        assert!(book.add(Network::Stagenet, draft(""), NOW).is_err());
        assert_eq!(book.all().len(), 1);
        // The same send address twice is allowed.
        let twin = book.add(Network::Stagenet, Draft { send_address: Some(sam_address.clone()), ..draft("Sam at work") }, NOW).unwrap();
        assert_eq!(twin.id, 2);
        let edited = book.edit(Network::Stagenet, sam.id, Draft { photo: None, send_address: None, ..draft("Samuel") }).unwrap();
        assert_eq!((edited.name.as_str(), edited.note, edited.photo, edited.send_address), ("Samuel", None, None, None));
        assert!(book.delete(twin.id));
        assert!(!book.delete(twin.id));
        // Ids aren't used again.
        assert_eq!(book.add(Network::Stagenet, draft("Mum"), NOW).unwrap().id, 3);
        assert!(book.edit(Network::Stagenet, twin.id, draft("Ghost")).is_err());
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
        let changed = book.set_send_address(Network::Stagenet, sam.id, &code).unwrap();
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
    fn contacts_list_by_name_and_search_names_and_notes() {
        let mut book = Book::default();
        let sam_address = address(1);
        book.add(Network::Stagenet, Draft { note: Some("Lunch money".into()), send_address: Some(sam_address.clone()), ..draft("Sam") }, NOW).unwrap();
        book.add(Network::Stagenet, Draft { note: Some("Monthly, by the 1st".into()), ..draft("Rent") }, NOW).unwrap();
        book.add(Network::Stagenet, draft("corner Café"), NOW).unwrap();
        book.add(Network::Stagenet, draft("Mum"), NOW).unwrap();
        let rows = [paid(&sam_address, 1, NOW - 4 * DAY), paid(&sam_address, 1, NOW - 9 * DAY)];
        let shown: Vec<_> = list(&book, &rows, "", clock()).into_iter().map(|r| (r.name, r.line)).collect();
        assert_eq!(
            shown,
            [
                ("corner Café".into(), "Not paid yet".into()),
                ("Mum".into(), "Not paid yet".into()),
                ("Rent".into(), "Monthly, by the 1st".into()),
                ("Sam".into(), "Lunch money · paid 27 Oct".into()),
            ] as [(String, String); 4]
        );
        let names = |search: &str| list(&book, &rows, search, clock()).into_iter().map(|r| r.name).collect::<Vec<_>>();
        assert_eq!(names("LUNCH"), ["Sam"]);
        assert_eq!(names("café"), ["corner Café"]);
        assert_eq!(names("m"), ["Mum", "Rent", "Sam"]);
        assert!(names("nobody").is_empty());
        let today = [paid(&sam_address, 1, NOW)];
        book.edit(Network::Stagenet, 1, Draft { send_address: Some(sam_address.clone()), ..draft("Sam") }).unwrap();
        assert_eq!(list(&book, &today, "sam", clock())[0].line, "Paid today");
        assert_eq!(list(&book, &today, "sam", clock())[0].initial, "S");
    }

    #[test]
    fn send_s_sheet_lists_contacts_with_a_send_address_most_recently_paid_first() {
        let mut book = Book::default();
        let (a, b, c, d) = (address(1), address(2), address(3), address(4));
        book.add(Network::Stagenet, Draft { send_address: Some(a.clone()), note: Some("Coffee".into()), ..draft("Corner Café") }, NOW).unwrap();
        book.add(Network::Stagenet, Draft { send_address: Some(b.clone()), ..draft("Sam") }, NOW).unwrap();
        book.add(Network::Stagenet, Draft { send_address: Some(c.clone()), ..draft("Mum") }, NOW).unwrap();
        book.add(Network::Stagenet, Draft { send_address: Some(d.clone()), ..draft("Aunt") }, NOW).unwrap();
        book.add(Network::Stagenet, draft("No address"), NOW).unwrap();
        let mut lunch = paid(&b, 1, NOW - 4 * DAY);
        lunch.note = Some("Lunch".into());
        let rows = [lunch, paid(&c, 300_000_000_000, NOW - 28 * DAY), paid(&b, 1, NOW - 9 * DAY)];
        let shown: Vec<_> = on_send(&book, &rows, clock()).into_iter().map(|r| (r.name, r.line)).collect();
        assert_eq!(
            shown,
            [("Sam".into(), "Lunch · 27 Oct".into()), ("Mum".into(), "3 Oct · 0.3 XMR".into()), ("Aunt".into(), "Not paid yet".into())]
                as [(String, String); 3]
        );
        // Never paid ones, by name: the note when there is one.
        let fresh = on_send(&book, &[], clock());
        assert_eq!(fresh[0].name, "Aunt");
        assert_eq!(fresh[1].line, "Coffee");
    }

    #[test]
    fn paid_not_saved_lists_each_unknown_address_once_newest_first() {
        let mut book = Book::default();
        let (sam, code, stranger, shop) = (address(1), address(2), address(3), address(4));
        book.add(Network::Stagenet, Draft { send_address: Some(sam.clone()), also_paid_at: vec![code.clone()], ..draft("Sam") }, NOW).unwrap();
        let mut bought = paid(&shop, 1, NOW - 6 * DAY);
        bought.receipt = Some(ReceiptView {
            merchant: "Kiosk".into(),
            issued: String::new(),
            lines: vec![],
            item_count: 1,
            tax: None,
            total: "$33.21".into(),
            currency: "USD".into(),
        });
        let split = Activity { split: Some(3), ..paid(&address(5), 1, NOW) };
        let rows = [paid(&sam, 1, NOW), paid(&code, 1, NOW), paid(&stranger, 1_000_000_000, NOW - DAY), bought, paid(&stranger, 1, NOW - 9 * DAY), split];
        let shown: Vec<_> = unsaved(&book, &rows, clock()).into_iter().map(|p| (p.address, p.name, p.line)).collect();
        assert_eq!(
            shown,
            [
                (stranger.clone(), short_address(&stranger), "Paid yesterday · 0.001 XMR".to_owned()),
                // What was paid, not the receipt's $33.21.
                (shop.clone(), short_address(&shop), "Paid 25 Oct · 0.000000000001 XMR".to_owned()),
            ]
        );
    }

    #[test]
    fn a_card_shows_payments_to_every_address_and_deleting_keeps_them() {
        let mut book = Book::default();
        let (send, code) = (address(1), address(2));
        let sam = book
            .add(
                Network::Stagenet,
                Draft { send_address: Some(send.clone()), also_paid_at: vec![code.clone()], note: Some("Lunch money".into()), ..draft("Sam") },
                NOW,
            )
            .unwrap();
        let mut lunch = paid(&send, 120_400_000_000, NOW - 4 * DAY);
        lunch.note = Some("Lunch".into());
        lunch.price = Some(crate::price::Record { currency: "USD".into(), price: 166_112_956, provider: "Kraken".into(), at: NOW, daily: false });
        let rows = vec![paid(&code, 98_000_000_000, NOW - 6 * DAY), lunch, paid(&address(3), 1, NOW)];
        let shown = card(&sam, &requests::Book::default(), &rows, clock());
        assert_eq!(shown.send_short.as_deref(), Some(three_part(&send).as_str()));
        assert_eq!((shown.send_label.as_str(), shown.send_button.as_str()), ("Send address · you pay here", "Change"));
        assert_eq!(shown.also_paid_at[0].line, "Not their send address · paid once, 25 Oct");
        assert_eq!(shown.also_note, "codes named Sam");
        assert_eq!(shown.paid_title, "Paid to Sam");
        let history: Vec<_> = shown.paid.iter().map(|p| (p.title.as_str(), p.when.as_str(), p.amount.as_str(), p.fiat.as_deref())).collect();
        assert_eq!(history, [("Lunch", "27 Oct", "−0.1204", Some("$20.00")), ("Sent", "25 Oct", "−0.098", None)]);
        assert_eq!(shown.paid_empty, None);
        assert!(shown.requests.is_empty());
        let none = card(&Contact { send_address: None, also_paid_at: vec![], ..sam.clone() }, &requests::Book::default(), &rows, clock());
        assert_eq!(
            (none.send_label.as_str(), none.send_button.as_str(), none.paid_empty.as_deref()),
            ("No send address yet", "Add", Some("Nothing paid to Sam yet."))
        );
        // Deleted, the payments are still there and no one's.
        book.delete(sam.id);
        assert_eq!(unsaved(&book, &rows, clock()).len(), 3);
    }

    #[test]
    fn a_card_lists_their_requests_and_what_came_from_them() {
        let keys = Keys::generate(Network::Stagenet).keys;
        let mut book = Book::default();
        let send = address(1);
        let sam = book.add(Network::Stagenet, Draft { send_address: Some(send.clone()), ..draft("Sam") }, NOW).unwrap();
        let mut reqs = requests::Book::default();
        let draft = |subaddress: u32, contact: Option<u64>, note: &str| requests::Draft {
            subaddress,
            address: keys.address(subaddress),
            contact,
            note: Some(note.into()),
            asked: None,
        };
        reqs.add(draft(9, Some(sam.id), "Concert tickets"), NOW - 2 * DAY).unwrap();
        reqs.add(draft(10, None, "Not Sam's"), NOW).unwrap();
        let mut from_sam = paid("in", 500_000_000_000, NOW - DAY);
        from_sam.direction = Direction::In;
        from_sam.to = None;
        from_sam.subaddress = Some(9);
        from_sam.note = Some("Concert tickets".into());
        from_sam.contact_id = Some(sam.id);
        let rows = vec![from_sam, paid(&send, 120_400_000_000, NOW - 3 * DAY)];
        let shown = card(&sam, &reqs, &rows, clock());
        assert_eq!((shown.requests_title.as_str(), shown.requests_note.as_str()), ("Requests · Sam pays you", "1 address"));
        assert_eq!(shown.requests.iter().map(|r| (r.note.as_str(), r.state.as_str())).collect::<Vec<_>>(), [("Concert tickets", "Received 0.5 XMR")]);
        assert_eq!(shown.paid_title, "Payments with Sam");
        let history: Vec<_> = shown.paid.iter().map(|p| (p.title.as_str(), p.when.as_str(), p.amount.as_str())).collect();
        assert_eq!(history, [("From Sam", "Concert tickets · Yesterday", "+0.5"), ("Sent", "28 Oct", "−0.1204")]);
        // What came from them isn't "paid to" them anywhere else.
        assert_eq!(list(&book, &rows, "", clock())[0].line, "Paid 28 Oct");
    }

    #[test]
    fn after_paying_a_named_code_fob_offers_its_contact_or_a_new_one() {
        let mut book = Book::default();
        let (sam_send, code, henry, plain) = (address(1), address(2), address(3), address(4));
        let sam = book.add(Network::Stagenet, Draft { send_address: Some(sam_send.clone()), ..draft("Sam") }, NOW).unwrap();
        // The name matches Sam, any case: Add to Sam, or a new contact named Sam paid at that address.
        let o = offer(&book, &code, Some(" sAm "), Some("Pizza")).unwrap();
        assert_eq!(o.add_to.as_ref().map(|c| c.id), Some(sam.id));
        assert_eq!(
            (o.button.as_str(), o.other.as_deref(), o.named.as_deref()),
            ("Add to Sam", Some("A different sAm? New contact"), Some("to a code named “sAm”"))
        );
        assert_eq!(o.footnote.as_deref(), Some(NAMED_FOOTNOTE));
        assert_eq!(o.draft, Draft { name: "sAm".into(), note: Some("Pizza".into()), photo: None, send_address: None, also_paid_at: vec![code.clone()] });
        // A name that's nobody's: Save as a contact, the address one they were paid at, never their send address.
        let o = offer(&book, &henry, Some("Henry"), None).unwrap();
        assert_eq!((o.add_to, o.button.as_str(), o.other), (None, "Save as a contact", None));
        assert_eq!((o.draft.name.as_str(), o.draft.send_address, o.draft.also_paid_at), ("Henry", None, vec![henry.clone()]));
        // No name: the address is their send address, the payment's note theirs.
        let o = offer(&book, &plain, None, Some(" Lunch ")).unwrap();
        assert_eq!((o.named, o.footnote, o.button.as_str()), (None, None, "Save as a contact"));
        assert_eq!(o.draft, Draft { name: String::new(), note: Some("Lunch".into()), photo: None, send_address: Some(plain.clone()), also_paid_at: vec![] });
        // A name that cleans to nothing counts as none.
        assert_eq!(offer(&book, &plain, Some("\n"), None).unwrap().draft.send_address.as_deref(), Some(plain.as_str()));
        // Already a contact's (send address or paid at): nothing.
        assert!(offer(&book, &sam_send, Some("Henry"), None).is_none());
        book.add_paid_at(Network::Stagenet, sam.id, &code).unwrap();
        assert!(offer(&book, &code, Some("Sam"), None).is_none());
        // Saving the offered draft keeps the address apart from the send address.
        let saved = book.add(Network::Stagenet, new_contact(&henry, Some("Henry"), None), NOW).unwrap();
        assert_eq!((saved.send_address, saved.also_paid_at), (None, vec![henry.clone()]));
        assert_eq!(
            paid_at_note(std::slice::from_ref(&henry)).unwrap(),
            format!("Paid at {} from their code: kept with them, not as their send address.", three_part(&henry))
        );
        assert_eq!(paid_at_note(&[]), None);
        assert!(paid_at_note(&[henry, code]).unwrap().starts_with("Paid at 2 addresses"));
    }

    #[test]
    fn addresses_show_their_first_middle_and_last_five() {
        let a = address(1);
        let groups = uri::address_groups(&a);
        assert_eq!(three_part(&a), format!("{} … {} … {}", groups[0].text, groups[9].text, groups[18].text));
        assert_eq!(three_part("86Ux"), "86Ux");
    }

    #[test]
    fn the_scanner_and_edit_card_speak_of_the_contact() {
        assert_eq!(
            scan_texts(Some("Sam")),
            ScanTexts { title: "Sam's send address".into(), hint: "Point at the code on Sam's phone (their Receive screen) or any Monero address code".into() }
        );
        assert_eq!(scan_texts(Some(" ")).title, "Their send address");
        assert_eq!(scan_texts(None).hint, "Point at the code on their phone (their Receive screen) or any Monero address code");
        let (old, new) = (address(1), address(2));
        let f = found(Network::Stagenet, Some("Sam"), Some(&old), &format!("monero:{new}")).unwrap();
        assert_eq!((f.address.as_str(), f.title.as_str()), (new.as_str(), "Found an address"));
        assert_eq!(f.warning.unwrap(), format!("This replaces Sam's current send address ({}). Check the pattern with Sam.", uri::short(&old)));
        assert_eq!(
            found(Network::Stagenet, None, Some(&old), &new).unwrap().warning.unwrap(),
            format!("This replaces their current send address ({}). Check the pattern with them.", uri::short(&old))
        );
        assert_eq!(found(Network::Stagenet, Some("Sam"), Some(&new), &new).unwrap().warning.as_deref(), Some("That's already Sam's send address."));
        assert_eq!(found(Network::Stagenet, Some("Sam"), None, &new).unwrap().warning, None);
        assert!(found(Network::Stagenet, Some("Sam"), None, "hello").is_err());
        let edit = edit_texts("Sam", false);
        assert_eq!((edit.title.as_str(), edit.delete_title.as_str()), ("Edit contact", "Delete Sam?"));
        assert_eq!(edit.address_hint, "Scan or paste to change it. Payments already sent stay with Sam.");
        let new = edit_texts("", true);
        assert_eq!((new.title.as_str(), new.address_hint.as_str()), ("New contact", "Scan their code or paste it. Leave it empty if they only pay you."));
        assert_eq!(initial(" émile"), "É");
        assert_eq!(initial(""), "?");
    }
}
