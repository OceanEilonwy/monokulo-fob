//! Payment requests: a code shared from Receive, made only when Share or
//! Copy is pressed, each on a new address of this wallet that's used for
//! nothing else. A request keeps who it's for (a contact, or no one), its
//! note, the amount asked and when; it's never changed after, only deleted.
//! A request shows only what its address received, never what's left.
//!
//! Every word the request screens, Activity's request lines and a
//! contact's "Requests · Sam pays you" say is written here.

use serde::{Deserialize, Serialize};

use crate::{activity::{self, Activity}, clock::{self, Clock}, contacts, error::ScreenError, error::Result};
use fob_monero::{amount, fiat};
use fob_codes::uri;

pub use fob_codes::names::clean_note;

/// What a request asked for, as it was typed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Asked {
    /// As typed: "80", "0.5".
    pub typed: String,
    /// What it was typed in: a currency's code ("USD") or "XMR".
    pub unit: String,
    /// The XMR it came to then, which the code asks for.
    pub xmr: u64,
    /// Typed in money: the amount written out, "$80.00".
    #[serde(default)]
    pub fiat: Option<String>,
}

impl Asked {
    /// "$80.00", or "0.5 XMR".
    pub fn text(&self) -> String {
        self.fiat.clone().unwrap_or_else(|| format!("{} XMR", amount::format(self.xmr)))
    }
}

/// A request to be paid.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Request {
    pub id: u64,
    /// Its address's index: handed out for this request only.
    pub subaddress: u32,
    pub address: String,
    /// The contact it's for; `None` for "Share without a name".
    #[serde(default)]
    pub contact: Option<u64>,
    #[serde(default)]
    pub note: Option<String>,
    /// `None` when no amount was typed.
    #[serde(default)]
    pub asked: Option<Asked>,
    /// When it was made, seconds since the epoch.
    pub created: u64,
    /// When it was deleted: it's gone from every screen, but its address
    /// is still this wallet's, and payments to it from before still name
    /// who it was for.
    #[serde(default)]
    pub deleted: Option<u64>,
    /// The payments already in (the pool's too) when it was deleted, by
    /// transaction: those, and only those, still name who it was for.
    /// `None` for one deleted before these were kept, which goes by time.
    #[serde(default)]
    pub paid_before_delete: Option<Vec<String>>,
    /// The name its code carries (`recipient_name`), as made: the code
    /// shown again is the one that went out.
    #[serde(default)]
    pub name: Option<String>,
}

/// A request about to be made.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Draft {
    pub subaddress: u32,
    pub address: String,
    pub contact: Option<u64>,
    /// As typed: cut to what a code holds, blank for none.
    pub note: Option<String>,
    pub asked: Option<Asked>,
    /// Who's asking, for the code: the profile's name while requests carry it.
    pub name: Option<String>,
}

/// Every request, kept in the wallet file.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Book {
    #[serde(default)]
    requests: Vec<Request>,
    /// Ids aren't used again after a delete.
    #[serde(default)]
    next_id: u64,
}

impl Book {
    /// The requests not deleted, oldest first.
    pub fn live(&self) -> impl Iterator<Item = &Request> {
        self.requests.iter().filter(|r| r.deleted.is_none())
    }

    pub fn get(&self, id: u64) -> Option<&Request> {
        self.live().find(|r| r.id == id)
    }

    /// The request made on the address at `subaddress`, deleted or not.
    pub fn at(&self, subaddress: u32) -> Option<&Request> {
        self.requests.iter().find(|r| r.subaddress == subaddress)
    }

    /// The contact payment `tx` to `subaddress` (at `time`) is from: the
    /// one its request was for, unless the request was deleted before it
    /// came.
    pub fn payer(&self, subaddress: u32, tx: &str, time: u64) -> Option<u64> {
        let before_delete = |r: &&Request| match (&r.deleted, &r.paid_before_delete) {
            (None, _) => true,
            (Some(_), Some(paid)) => paid.iter().any(|t| t == tx),
            (Some(deleted), None) => time <= *deleted,
        };
        self.at(subaddress).filter(before_delete).and_then(|r| r.contact)
    }

    /// The newest request, when it's what `draft` would make again (the
    /// same contact, amount as typed, unit, note and name) and nothing has
    /// come to it: Share and Copy pressed again for the same ask give it
    /// again, not a new address.
    pub fn same_again(&self, draft: &Draft, rows: &[Activity]) -> Option<&Request> {
        let asked = |a: &Option<Asked>| a.as_ref().filter(|a| a.xmr > 0).map(|a| (a.typed.clone(), a.unit.clone()));
        let newest = self.live().max_by_key(|r| (r.created, r.id))?;
        let same = newest.contact == draft.contact
            && newest.note == clean_note(draft.note.as_deref())
            && newest.name == draft.name.as_deref().and_then(contacts::clean_name)
            && asked(&newest.asked) == asked(&draft.asked);
        (same && payments(newest, rows).is_empty()).then_some(newest)
    }

    /// Makes a request. Its address must be one no request has had.
    pub fn add(&mut self, draft: Draft, now: u64) -> Result<Request> {
        if self.at(draft.subaddress).is_some() {
            return Err(ScreenError::RequestAddressUsed);
        }
        let id = self.next_id.max(self.requests.iter().map(|r| r.id + 1).max().unwrap_or(1)).max(1);
        self.next_id = id + 1;
        let request = Request {
            id,
            subaddress: draft.subaddress,
            address: draft.address,
            contact: draft.contact,
            note: clean_note(draft.note.as_deref()),
            asked: draft.asked.filter(|a| a.xmr > 0),
            created: now,
            deleted: None,
            paid_before_delete: None,
            name: draft.name.as_deref().and_then(contacts::clean_name),
        };
        self.requests.push(request.clone());
        Ok(request)
    }

    /// Deletes a request (it's kept, marked, so its address stays this
    /// wallet's), with the payments to it in `rows` (the pool's too) kept
    /// as the ones still named. Returns whether there was one.
    pub fn delete(&mut self, id: u64, now: u64, rows: &[Activity]) -> bool {
        match self.requests.iter_mut().find(|r| r.id == id && r.deleted.is_none()) {
            Some(r) => {
                r.deleted = Some(now);
                r.paid_before_delete = Some(payments(r, rows).into_iter().map(|a| a.tx).collect());
                true
            }
            None => false,
        }
    }
}

/// The payments to `request`'s address, newest first (unconfirmed on top,
/// as Activity has them), each as much as came to it: one transaction
/// paying two requests is each one's part.
pub fn payments(request: &Request, rows: &[Activity]) -> Vec<Activity> {
    rows.iter().filter_map(|a| a.part(request.subaddress)).collect()
}

/// What `request`'s address received, or `None` while nothing has: in the
/// money it asked in when every payment has a price in it ("$83.03"),
/// else in XMR ("0.5 XMR").
pub fn received(request: &Request, rows: &[Activity]) -> Option<Received> {
    let paid = payments(request, rows);
    if paid.is_empty() {
        return None;
    }
    let atomic: u64 = paid.iter().map(|a| a.amount).sum();
    let xmr = format!("{} XMR", amount::format_short(atomic));
    let unit = request.asked.as_ref().filter(|a| a.fiat.is_some()).map(|a| a.unit.as_str());
    let fiat = unit.and_then(|unit| {
        let currency = fiat::currency(unit);
        let minor: Option<u64> =
            paid.iter().map(|a| a.price.as_ref().filter(|p| p.currency == unit).map(|p| fiat::to_minor(a.amount, p.price, &currency))).sum();
        minor.map(|m| fiat::format(m, &currency))
    });
    Some(Received { atomic, text: fiat.clone().unwrap_or_else(|| xmr.clone()), xmr, fiat })
}

/// What a request's address received.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Received {
    pub atomic: u64,
    /// "$83.03" or "0.5 XMR": what the lines say.
    pub text: String,
    /// "0.5 XMR"
    pub xmr: String,
    /// "$83.03", when it can be said in the money asked in.
    pub fiat: Option<String>,
}

/// The name of the contact `request` is for, while it's a contact.
fn name<'a>(request: &Request, book: &'a contacts::Book) -> Option<&'a str> {
    request.contact.and_then(|id| book.get(id)).map(|c| c.name.as_str())
}

/// A request's line in Activity, with the amount asked on the right.
pub fn activity_row(request: &Request, book: &contacts::Book, rows: &[Activity], clock: Clock) -> activity::Row {
    let name = name(request, book);
    let (_, seconds) = clock::local(request.created, clock);
    activity::Row {
        activity: None,
        request: Some(request.id),
        share: None,
        time: request.created,
        title: activity::RowTitle::Request(name.map(str::to_owned)),
        line: activity::RowLine::Request { named: name.is_some(), note: request.note.clone(), received: received(request, rows).map(|r| r.text), time: clock::time_of_day(seconds, clock) },
        amount: request.asked.as_ref().map(Asked::text).unwrap_or_default(),
        fiat: request.asked.as_ref().map(|_| activity::RowFiat::Asked),
    }
}

/// The request lines for Activity: every request not deleted, newest first.
pub fn activity_rows(requests: &Book, book: &contacts::Book, rows: &[Activity], clock: Clock) -> Vec<activity::Row> {
    let mut lines: Vec<&Request> = requests.live().collect();
    lines.sort_by_key(|r| std::cmp::Reverse((r.created, r.id)));
    lines.into_iter().map(|r| activity_row(r, book, rows, clock)).collect()
}

/// A request's own screen.
#[derive(Clone, Debug, PartialEq)]
pub struct Detail {
    pub request: Request,
    /// Who it's for, while they're a contact.
    pub name: Option<String>,
    /// What it asked: "$80.00", "0.5 XMR"; `None` for any amount.
    pub asked: Option<String>,
    pub when: clock::When,
    /// What an amount asked in money was in XMR at the time: "0.4817".
    pub xmr_then: Option<String>,
    pub received: Option<Received>,
    /// "84Kp7 … Q2mXa … t9VbL"
    pub address_short: String,
    /// Each payment to its address, newest first, titled as Activity has it.
    pub payments: Vec<activity::Row>,
}

/// `request`'s own screen.
pub fn detail(request: &Request, book: &contacts::Book, rows: &[Activity], clock: Clock) -> Detail {
    let payments = payments(request, rows)
        .into_iter()
        .map(|a| {
            let mut row = activity::payment_row(a.clone(), clock);
            row.line = activity::RowLine::RequestPayment { when: clock::when(a.time, clock), waiting: activity::waiting(&a) };
            row
        })
        .collect::<Vec<_>>();
    Detail {
        name: name(request, book).map(str::to_owned),
        asked: request.asked.as_ref().map(Asked::text),
        when: clock::when(request.created, clock),
        // An amount asked in XMR is the headline already.
        xmr_then: request.asked.as_ref().filter(|a| a.fiat.is_some()).map(|a| amount::format_short(a.xmr)),
        received: received(request, rows),
        address_short: contacts::three_part(&request.address),
        payments,
        request: request.clone(),
    }
}

/// One of a contact's requests, on their card.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CardLine {
    pub id: u64,
    pub address: String,
    pub note: Option<String>,
    pub subaddress: u32,
    pub when: clock::When,
    /// What came: "$83.03"; `None` while nothing has.
    pub received: Option<String>,
    /// What it asked: "$80.00".
    pub asked: Option<String>,
}

/// A contact's requests for their card, newest first.
pub fn on_card(contact: u64, requests: &Book, rows: &[Activity], clock: Clock) -> Vec<CardLine> {
    let mut theirs: Vec<&Request> = requests.live().filter(|r| r.contact == Some(contact)).collect();
    theirs.sort_by_key(|r| std::cmp::Reverse((r.created, r.id)));
    theirs
        .into_iter()
        .map(|r| CardLine {
            id: r.id,
            address: r.address.clone(),
            note: r.note.clone(),
            subaddress: r.subaddress,
            when: clock::when(r.created, clock),
            received: received(r, rows).map(|g| g.text),
            asked: r.asked.as_ref().map(Asked::text),
        })
        .collect()
}

/// A contact's requests in a line, for Share's sheet.
pub fn summary(contact: u64, requests: &Book, rows: &[Activity]) -> contacts::ContactLine {
    let theirs: Vec<&Request> = requests.live().filter(|r| r.contact == Some(contact)).collect();
    let paid = theirs.iter().filter(|r| !payments(r, rows).is_empty()).count();
    contacts::ContactLine::Requests { paid, waiting: theirs.len() - paid }
}

/// Share's sheet ("Who is this for?"): every contact by name, each with
/// its requests in a line.
pub fn sheet(book: &contacts::Book, requests: &Book, rows: &[Activity]) -> Vec<contacts::Row> {
    let mut all: Vec<&contacts::Contact> = book.all().iter().collect();
    all.sort_by_cached_key(|c| (c.name.to_lowercase(), c.id));
    all.into_iter().map(|c| contacts::row(c, summary(c.id, requests, rows))).collect()
}

/// A request's payment code: its address, the XMR it asked for, its note,
/// and the name (who's asking) it was made with.
pub fn code_uri(request: &Request) -> String {
    uri::payment_uri(&request.address, request.asked.as_ref().map(|a| a.xmr), request.note.as_deref(), request.name.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use fob_vault::keys::Keys;
    use fob_monero::{network::Network, fiat::Record};
    use crate::activity::Direction;
    use fob_monero::network::UNLOCK;

    /// 2026-10-31 12:00 UTC.
    const NOW: u64 = 1_793_448_000;
    const HOUR: u64 = 3600;
    const XMR: u64 = amount::ATOMIC_PER_XMR;

    fn keys() -> Keys {
        Keys::generate(Network::Stagenet).keys
    }

    fn usd(typed: &str, xmr: u64) -> Option<Asked> {
        let minor = (typed.parse::<f64>().unwrap() * 100.0).round() as u64;
        Some(Asked { typed: typed.into(), unit: "USD".into(), xmr, fiat: Some(fiat::format(minor, &fiat::currency("USD"))) })
    }

    fn draft(keys: &Keys, subaddress: u32, contact: Option<u64>, note: Option<&str>, asked: Option<Asked>) -> Draft {
        Draft { subaddress, address: keys.address(subaddress), contact, note: note.map(Into::into), asked, name: None }
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
            confirmations: 3,
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

    #[test]
    fn a_request_is_kept_as_made_on_an_address_of_its_own() {
        let keys = keys();
        let mut requests = Book::default();
        let made = requests.add(draft(&keys, 9, Some(1), Some("  Concert\u{7} tickets \n"), usd("80", 481_700_000_000)), NOW).unwrap();
        assert_eq!((made.id, made.subaddress, made.note.as_deref(), made.created), (1, 9, Some("Concert tickets"), NOW));
        assert_eq!(made.asked.as_ref().unwrap().text(), "$80.00");
        // An address goes to one request only, deleted or not.
        assert!(requests.add(draft(&keys, 9, None, None, None), NOW).is_err());
        assert!(requests.delete(made.id, NOW + 10, &[]));
        assert!(!requests.delete(made.id, NOW + 20, &[]));
        assert!(requests.get(made.id).is_none());
        assert!(requests.add(draft(&keys, 9, None, None, None), NOW).is_err());
        // Ids aren't used again; a zero amount or blank note is none.
        let next = requests.add(draft(&keys, 10, None, Some("  "), Some(Asked { typed: "0".into(), unit: "XMR".into(), xmr: 0, fiat: None })), NOW).unwrap();
        assert_eq!((next.id, next.note, next.asked), (2, None, None));
        // Kept in the file as made; an older file reads with none.
        let json = serde_json::to_string(&requests).unwrap();
        assert_eq!(serde_json::from_str::<Book>(&json).unwrap(), requests);
        assert_eq!(serde_json::from_str::<Book>("{}").unwrap(), Book::default());
        // A long note is cut to what a code holds.
        let long = requests.add(draft(&keys, 11, None, Some(&"n".repeat(200)), None), NOW).unwrap();
        assert_eq!(long.note.unwrap().chars().count(), uri::NOTE_MAX_CHARS);
    }

    #[test]
    fn a_deleted_request_names_only_what_came_before_it_went() {
        let keys = keys();
        let mut requests = Book::default();
        let r = requests.add(draft(&keys, 4, Some(7), Some("Pizza"), None), NOW).unwrap();
        assert_eq!(requests.payer(4, "in", NOW + 1), Some(7));
        // In the pool when it's deleted; mined after, its block's time later than the delete: still theirs.
        let pooled = Activity { not_confirmed: false, released: false, tx: "pooled".into(), height: None, ..paid_in(4, 1, NOW + 30, None) };
        requests.delete(r.id, NOW + HOUR, &[pooled, paid_in(5, 1, NOW, None)]);
        assert_eq!(requests.payer(4, "pooled", NOW + 3 * HOUR), Some(7));
        assert_eq!(requests.payer(4, "later", NOW + 1), None, "came after, whatever its block's time says");
        assert_eq!(requests.at(4).map(|r| r.id), Some(r.id), "its address is still known");
        assert_eq!(requests.payer(5, "in", NOW), None);
        // Deleted before the payments in were kept: by time.
        let mut old = requests.clone();
        old.requests[0].paid_before_delete = None;
        assert_eq!(old.payer(4, "x", NOW + 1), Some(7));
        assert_eq!(old.payer(4, "x", NOW + 2 * HOUR), None);
    }

    #[test]
    fn the_same_ask_again_gives_the_same_request_until_something_comes() {
        let keys = keys();
        let mut requests = Book::default();
        let ask = |sub: u32| Draft { name: Some("Henry".into()), ..draft(&keys, sub, Some(7), Some("Pizza"), usd("20", 120_400_000_000)) };
        assert!(requests.same_again(&ask(1), &[]).is_none());
        let first = requests.add(ask(1), NOW).unwrap();
        assert_eq!(requests.same_again(&ask(2), &[]).map(|r| r.id), Some(first.id));
        // The same typed amount comes to other XMR at another price: still the same ask.
        let repriced = Draft { asked: usd("20", 119_000_000_000), ..ask(2) };
        assert_eq!(requests.same_again(&repriced, &[]).map(|r| r.id), Some(first.id));
        // Anything else different, or something paid to it: a new one.
        for other in [
            Draft { contact: None, ..ask(2) },
            Draft { note: Some("Pasta".into()), ..ask(2) },
            Draft { name: None, ..ask(2) },
            Draft { asked: usd("21", 1), ..ask(2) },
            Draft { asked: Some(Asked { typed: "20".into(), unit: "XMR".into(), xmr: 1, fiat: None }), ..ask(2) },
        ] {
            assert!(requests.same_again(&other, &[]).is_none(), "{other:?}");
        }
        assert!(requests.same_again(&ask(2), &[paid_in(1, 5, NOW, None)]).is_none());
        // Only the newest: an older one alike isn't given again.
        requests.add(Draft { note: Some("Other".into()), ..ask(2) }, NOW + 1).unwrap();
        assert!(requests.same_again(&ask(3), &[]).is_none());
    }

    #[test]
    fn a_request_shows_what_its_address_received_or_waiting() {
        let keys = keys();
        let mut requests = Book::default();
        let r = requests.add(draft(&keys, 9, None, None, usd("80", 481_700_000_000)), NOW).unwrap();
        assert_eq!(received(&r, &[]), None);
        // Each payment counts, in the money asked in when each has a price in it.
        let rows = [paid_in(9, XMR / 2, NOW + HOUR, Some(166_060_000)), paid_in(9, XMR / 100, NOW + 2 * HOUR, Some(166_060_000)), paid_in(3, XMR, NOW, None)];
        let got = received(&r, &rows).unwrap();
        assert_eq!((got.atomic, got.text.as_str(), got.xmr.as_str()), (XMR / 2 + XMR / 100, "$84.69", "0.51 XMR"));
        // One without a price: in XMR.
        let unpriced = [paid_in(9, XMR / 2, NOW + HOUR, None), paid_in(9, XMR / 100, NOW + HOUR, Some(166_060_000))];
        assert_eq!(received(&r, &unpriced).unwrap().text, "0.51 XMR");
        // Asked in XMR: in XMR.
        let xmr = requests.add(draft(&keys, 10, None, None, Some(Asked { typed: "0.5".into(), unit: "XMR".into(), xmr: XMR / 2, fiat: None })), NOW).unwrap();
        assert_eq!(received(&xmr, &[paid_in(10, XMR / 2, NOW, Some(1))]).unwrap().text, "0.5 XMR");
    }

    #[test]
    fn a_request_s_code_asks_for_what_it_asked_and_may_name_who_asks() {
        let keys = keys();
        let mut requests = Book::default();
        let r = requests.add(draft(&keys, 9, None, Some("Concert tickets"), usd("80", 481_700_000_000)), NOW).unwrap();
        let address = keys.address(9);
        assert_eq!(code_uri(&r), format!("monero:{address}?tx_amount=0.4817&tx_description=Concert%20tickets"));
        let named = code_uri(&Request { name: Some("Henry H".into()), ..r.clone() });
        assert_eq!(named, format!("monero:{address}?tx_amount=0.4817&recipient_name=Henry%20H&tx_description=Concert%20tickets"));
        // Read back as the payer's Fob reads it.
        let read = uri::parse_payment(Network::Stagenet, &named).unwrap();
        assert_eq!((read.recipient_name.as_deref(), read.description.as_deref(), read.amount), (Some("Henry H"), Some("Concert tickets"), Some(481_700_000_000)));
    }
}
