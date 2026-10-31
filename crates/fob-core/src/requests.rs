//! Payment requests: a code shared from Receive, made only when Share or
//! Copy is pressed, each on a new address of this wallet that's used for
//! nothing else. A request keeps who it's for (a contact, or no one), its
//! note, the amount asked and when; it's never changed after, only deleted.
//! A request shows only what its address received, never what's left.
//!
//! Every word the request screens, Activity's request lines and a
//! contact's "Requests · Sam pays you" say is written here.

use serde::{Deserialize, Serialize};

use crate::{
    activity::{self, Clock},
    amount, contacts,
    error::{FobError, Result},
    price, uri,
    wallet::{Activity, Direction},
};

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
}

/// A note as a request keeps it: control characters and the marks that
/// turn text round taken out, trimmed, cut to what a payment code holds.
pub fn clean_note(note: Option<&str>) -> Option<String> {
    let kept: String = note?.chars().filter(contacts::shown).collect();
    let fitted = uri::fit_note(kept.trim()).0;
    let fitted = fitted.trim_end();
    (!fitted.is_empty()).then(|| fitted.to_owned())
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

    /// The contact a payment to `subaddress` at `time` is from: the one its
    /// request was for, unless the request was deleted before it came.
    pub fn payer(&self, subaddress: u32, time: u64) -> Option<u64> {
        self.at(subaddress).filter(|r| r.deleted.is_none_or(|d| time <= d)).and_then(|r| r.contact)
    }

    /// Makes a request. Its address must be one no request has had.
    pub fn add(&mut self, draft: Draft, now: u64) -> Result<Request> {
        if self.at(draft.subaddress).is_some() {
            return Err(FobError::Request("That address was already given to a request.".into()));
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
        };
        self.requests.push(request.clone());
        Ok(request)
    }

    /// Deletes a request (it's kept, marked, so its address stays this
    /// wallet's). Returns whether there was one.
    pub fn delete(&mut self, id: u64, now: u64) -> bool {
        match self.requests.iter_mut().find(|r| r.id == id && r.deleted.is_none()) {
            Some(r) => {
                r.deleted = Some(now);
                true
            }
            None => false,
        }
    }
}

/// The payments to `request`'s address, newest first (unconfirmed on top,
/// as Activity has them).
pub fn payments<'a>(request: &Request, rows: &'a [Activity]) -> Vec<&'a Activity> {
    rows.iter().filter(|a| a.direction == Direction::In && a.subaddress == Some(request.subaddress)).collect()
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
        let currency = price::currency(unit);
        let minor: Option<u64> =
            paid.iter().map(|a| a.price.as_ref().filter(|p| p.currency == unit).map(|p| price::to_minor(a.amount, p.price, &currency))).sum();
        minor.map(|m| price::format(m, &currency))
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

fn capitalised(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

/// A request's line in Activity: "Request to Sam" with "Concert tickets ·
/// received $83.03 · 08:30", or "Request" with "No name · Bike · waiting ·
/// 08:12"; the amount asked on the right, "asked" under it.
pub fn activity_row(request: &Request, book: &contacts::Book, rows: &[Activity], clock: Clock) -> activity::Row {
    let name = name(request, book);
    let state = match received(request, rows) {
        Some(r) => format!("received {}", r.text),
        None => "waiting".into(),
    };
    let (_, seconds) = activity::local(request.created, clock);
    let parts = [name.is_none().then(|| "No name".to_owned()), request.note.clone(), Some(state), Some(activity::time_of_day(seconds, clock))];
    activity::Row {
        activity: None,
        request: Some(request.id),
        share: None,
        time: request.created,
        title: name.map(|n| format!("Request to {n}")).unwrap_or_else(|| "Request".into()),
        line: capitalised(&parts.into_iter().flatten().collect::<Vec<_>>().join(" · ")),
        amount: request.asked.as_ref().map(Asked::text).unwrap_or_default(),
        fiat: request.asked.as_ref().map(|_| "asked".into()),
    }
}

/// The request lines for Activity: every request not deleted, newest first.
pub fn activity_rows(requests: &Book, book: &contacts::Book, rows: &[Activity], clock: Clock) -> Vec<activity::Row> {
    let mut lines: Vec<&Request> = requests.live().collect();
    lines.sort_by_key(|r| std::cmp::Reverse((r.created, r.id)));
    lines.into_iter().map(|r| activity_row(r, book, rows, clock)).collect()
}

/// A request's own screen, every word ready to draw.
#[derive(Clone, Debug, PartialEq)]
pub struct Detail {
    pub request: Request,
    /// "$80.00", "0.5 XMR", or "Any amount".
    pub headline: String,
    /// "asked of Sam · Concert tickets", "asked, no name · Bike", "for Sam".
    pub asked_line: String,
    /// "Today 08:30 · 0.4817 XMR at the time"
    pub when: String,
    /// "Received"
    pub received_label: String,
    /// "$83.03 · 0.5 XMR", "0.5 XMR", or "Waiting".
    pub received: String,
    pub received_any: bool,
    /// "Address 9"
    pub address_label: String,
    /// "84Kp7 … Q2mXa … t9VbL"
    pub address_short: String,
    /// Each payment to its address, newest first, titled as Activity has
    /// it ("From Sam", "Received"), its line its day, time and status.
    pub payments: Vec<activity::Row>,
    /// "Nothing paid to it yet.", while nothing is.
    pub payments_empty: Option<String>,
    /// "Delete this request?"
    pub delete_title: String,
    pub delete_body: String,
}

/// `request`'s own screen.
pub fn detail(request: &Request, book: &contacts::Book, rows: &[Activity], clock: Clock) -> Detail {
    let name = name(request, book);
    let who = match (&request.asked, name) {
        (Some(_), Some(n)) => format!("asked of {n}"),
        (Some(_), None) => "asked, no name".into(),
        (None, Some(n)) => format!("for {n}"),
        (None, None) => "No name".into(),
    };
    let then = request.asked.as_ref().map(|a| match &a.fiat {
        Some(_) => format!("{} XMR at the time", amount::format_short(a.xmr)),
        None => format!("{} XMR", amount::format(a.xmr)),
    });
    let got = received(request, rows);
    let payments = payments(request, rows)
        .into_iter()
        .map(|a| {
            let mut row = activity::payment_row(a.clone(), clock);
            let status = activity::waiting(a).map(|w| w.to_lowercase());
            row.line = [Some(activity::when(a.time, clock)), status].into_iter().flatten().collect::<Vec<_>>().join(" · ");
            row
        })
        .collect::<Vec<_>>();
    Detail {
        headline: request.asked.as_ref().map(Asked::text).unwrap_or_else(|| "Any amount".into()),
        asked_line: [Some(who), request.note.clone()].into_iter().flatten().collect::<Vec<_>>().join(" · "),
        // An amount asked in XMR is the headline already.
        when: [Some(activity::when(request.created, clock)), then.filter(|_| request.asked.as_ref().is_some_and(|a| a.fiat.is_some()))]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · "),
        received_label: "Received".into(),
        received: match &got {
            Some(Received { fiat: Some(fiat), xmr, .. }) => format!("{fiat} · {xmr}"),
            Some(r) => r.xmr.clone(),
            None => "Waiting".into(),
        },
        received_any: got.is_some(),
        address_label: format!("Address {}", request.subaddress),
        address_short: contacts::three_part(&request.address),
        payments_empty: payments.is_empty().then(|| "Nothing paid to it yet.".into()),
        payments,
        delete_title: "Delete this request?".into(),
        delete_body: match name {
            Some(n) => format!(
                "It leaves Activity and {n}'s card. Its address still works: anything paid there later still arrives, shown without a name. Payments already in stay."
            ),
            None => "It leaves Activity. Its address still works: anything paid there later still arrives. Payments already in stay.".into(),
        },
        request: request.clone(),
    }
}

/// One of a contact's requests, on their card.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CardLine {
    pub id: u64,
    pub address: String,
    /// The note, else "Request".
    pub note: String,
    /// "Address 9 · today 08:30", "Address 6 · 21 Oct"
    pub line: String,
    /// "Received $83.03", or "Waiting".
    pub state: String,
    /// Something came: drawn green.
    pub received: bool,
    /// "asked $80.00", with an amount.
    pub asked: Option<String>,
}

/// A contact's requests for their card, newest first.
pub fn on_card(contact: u64, requests: &Book, rows: &[Activity], clock: Clock) -> Vec<CardLine> {
    let mut theirs: Vec<&Request> = requests.live().filter(|r| r.contact == Some(contact)).collect();
    theirs.sort_by_key(|r| std::cmp::Reverse((r.created, r.id)));
    theirs
        .into_iter()
        .map(|r| {
            let day = activity::day_name(r.created, clock);
            let when = match day.as_str() {
                "Today" => format!("today {}", activity::time_of_day(activity::local(r.created, clock).1, clock)),
                "Yesterday" => "yesterday".into(),
                _ => day,
            };
            let got = received(r, rows);
            CardLine {
                id: r.id,
                address: r.address.clone(),
                note: r.note.clone().unwrap_or_else(|| "Request".into()),
                line: format!("Address {} · {when}", r.subaddress),
                state: got.as_ref().map(|g| format!("Received {}", g.text)).unwrap_or_else(|| "Waiting".into()),
                received: got.is_some(),
                asked: r.asked.as_ref().map(|a| format!("asked {}", a.text())),
            }
        })
        .collect()
}

/// "3 addresses", "1 address": beside "Requests · Sam pays you".
pub fn addresses_note(count: usize) -> String {
    if count == 1 { "1 address".into() } else { format!("{count} addresses") }
}

fn requests_word(n: usize) -> &'static str {
    if n == 1 { "request" } else { "requests" }
}

/// A contact's requests in a line, for Share's sheet: "2 requests paid,
/// 1 waiting", "1 request waiting", "No requests yet".
pub fn summary(contact: u64, requests: &Book, rows: &[Activity]) -> String {
    let theirs: Vec<&Request> = requests.live().filter(|r| r.contact == Some(contact)).collect();
    let paid = theirs.iter().filter(|r| !payments(r, rows).is_empty()).count();
    let waiting = theirs.len() - paid;
    match (paid, waiting) {
        (0, 0) => "No requests yet".into(),
        (p, 0) => format!("{p} {} paid", requests_word(p)),
        (0, w) => format!("{w} {} waiting", requests_word(w)),
        (p, w) => format!("{p} {} paid, {w} waiting", requests_word(p)),
    }
}

/// Share's sheet ("Who is this for?"): every contact by name, each with
/// its requests in a line.
pub fn sheet(book: &contacts::Book, requests: &Book, rows: &[Activity]) -> Vec<contacts::Row> {
    let mut all: Vec<&contacts::Contact> = book.all().iter().collect();
    all.sort_by_cached_key(|c| (c.name.to_lowercase(), c.id));
    all.into_iter().map(|c| contacts::row(c, summary(c.id, requests, rows))).collect()
}

/// A request's payment code: its address, the XMR it asked for, its note,
/// and `name` (who's asking) when the profile puts one in.
pub fn code_uri(request: &Request, name: Option<&str>) -> String {
    uri::payment_uri(&request.address, request.asked.as_ref().map(|a| a.xmr), request.note.as_deref(), name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        keys::Keys,
        network::Network,
        price::Record,
        wallet::{Direction, UNLOCK},
    };

    /// 2026-10-31 12:00 UTC.
    const NOW: u64 = 1_793_448_000;
    const HOUR: u64 = 3600;
    const DAY: u64 = 86_400;
    const XMR: u64 = amount::ATOMIC_PER_XMR;

    fn clock() -> Clock {
        Clock { now: NOW, utc_offset: 0, hours_24: true }
    }

    fn keys() -> Keys {
        Keys::generate(Network::Stagenet).keys
    }

    fn usd(typed: &str, xmr: u64) -> Option<Asked> {
        let minor = (typed.parse::<f64>().unwrap() * 100.0).round() as u64;
        Some(Asked { typed: typed.into(), unit: "USD".into(), xmr, fiat: Some(price::format(minor, &price::currency("USD"))) })
    }

    fn draft(keys: &Keys, subaddress: u32, contact: Option<u64>, note: Option<&str>, asked: Option<Asked>) -> Draft {
        Draft { subaddress, address: keys.address(subaddress), contact, note: note.map(Into::into), asked }
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
            confirmations: 3,
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
        }
    }

    fn sam_book() -> (contacts::Book, u64) {
        let mut book = contacts::Book::default();
        let sam = book.add(Network::Stagenet, contacts::Draft { name: "Sam".into(), ..Default::default() }, NOW).unwrap();
        (book, sam.id)
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
        assert!(requests.delete(made.id, NOW + 10));
        assert!(!requests.delete(made.id, NOW + 20));
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
        assert_eq!(requests.payer(4, NOW + 1), Some(7));
        requests.delete(r.id, NOW + HOUR);
        assert_eq!(requests.payer(4, NOW + 1), Some(7));
        assert_eq!(requests.payer(4, NOW + 2 * HOUR), None);
        assert_eq!(requests.at(4).map(|r| r.id), Some(r.id), "its address is still known");
        assert_eq!(requests.payer(5, NOW), None);
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
    fn activity_lines_read_like_the_board() {
        let keys = keys();
        let (book, sam) = sam_book();
        let mut requests = Book::default();
        let concert = requests.add(draft(&keys, 9, Some(sam), Some("Concert tickets"), usd("80", 481_700_000_000)), NOW - 3 * HOUR - 1800).unwrap();
        let bike = requests.add(draft(&keys, 10, None, Some("Bike"), usd("45", 1)), NOW - 3 * HOUR - 48 * 60).unwrap();
        let plain = requests.add(draft(&keys, 11, Some(sam), None, None), NOW - 4 * HOUR).unwrap();
        let gone = requests.add(draft(&keys, 12, Some(99), None, Some(Asked { typed: "0.5".into(), unit: "XMR".into(), xmr: XMR / 2, fiat: None })), NOW - 5 * HOUR).unwrap();
        let rows = [paid_in(9, XMR / 2, NOW - 3 * HOUR - 1200, Some(166_060_000))];
        let row = |r: &Request| {
            let row = activity_row(r, &book, &rows, clock());
            (row.title, row.line, row.amount, row.fiat)
        };
        assert_eq!(row(&concert), ("Request to Sam".into(), "Concert tickets · received $83.03 · 08:30".into(), "$80.00".into(), Some("asked".into())));
        assert_eq!(row(&bike), ("Request".into(), "No name · Bike · waiting · 08:12".into(), "$45.00".into(), Some("asked".into())));
        assert_eq!(row(&plain), ("Request to Sam".into(), "Waiting · 08:00".into(), String::new(), None));
        // A contact since deleted: no name; asked in XMR.
        assert_eq!(row(&gone), ("Request".into(), "No name · waiting · 07:00".into(), "0.5 XMR".into(), Some("asked".into())));
        let r = activity_row(&concert, &book, &rows, clock());
        assert_eq!((r.activity, r.request, r.time), (None, Some(concert.id), concert.created));
        // Deleted ones leave; the rest newest first.
        requests.delete(bike.id, NOW);
        let ids: Vec<_> = activity_rows(&requests, &book, &rows, clock()).into_iter().map(|r| r.request.unwrap()).collect();
        assert_eq!(ids, [concert.id, plain.id, gone.id]);
    }

    #[test]
    fn a_request_s_screen_reads_like_the_board() {
        let keys = keys();
        let (book, sam) = sam_book();
        let mut requests = Book::default();
        let r = requests.add(draft(&keys, 9, Some(sam), Some("Concert tickets"), usd("80", 481_700_000_000)), NOW - 3 * HOUR - 1800).unwrap();
        let waiting = detail(&r, &book, &[], clock());
        assert_eq!(
            (waiting.headline.as_str(), waiting.asked_line.as_str(), waiting.when.as_str()),
            ("$80.00", "asked of Sam · Concert tickets", "Today 08:30 · 0.4817 XMR at the time")
        );
        assert_eq!((waiting.received.as_str(), waiting.received_any, waiting.payments_empty.as_deref()), ("Waiting", false, Some("Nothing paid to it yet.")));
        assert_eq!((waiting.address_label.as_str(), waiting.address_short.clone()), ("Address 9", contacts::three_part(&r.address)));
        assert_eq!(waiting.delete_title, "Delete this request?");
        assert_eq!(
            waiting.delete_body,
            "It leaves Activity and Sam's card. Its address still works: anything paid there later still arrives, shown without a name. Payments already in stay."
        );
        let mut first = paid_in(9, XMR / 2, NOW - 3 * HOUR - 1200, Some(166_060_000));
        first.contact = Some("Sam".into());
        let paid = detail(&r, &book, &[first, paid_in(3, XMR, NOW, None)], clock());
        assert_eq!((paid.received.as_str(), paid.received_any), ("$83.03 · 0.5 XMR", true));
        let lines: Vec<_> = paid.payments.iter().map(|p| (p.title.as_str(), p.line.as_str(), p.amount.as_str(), p.fiat.as_deref())).collect();
        assert_eq!(lines, [("From Sam", "Today 08:40 · not ready · 3 of 10", "+0.5", Some("$83.03"))]);
        assert_eq!(paid.payments_empty, None);
        // No name, no amount, no note.
        let bare = requests.add(draft(&keys, 10, None, None, None), NOW - DAY).unwrap();
        let bare = detail(&bare, &book, &[], clock());
        assert_eq!((bare.headline.as_str(), bare.asked_line.as_str(), bare.when.as_str()), ("Any amount", "No name", "Yesterday 12:00"));
        assert_eq!(bare.delete_body, "It leaves Activity. Its address still works: anything paid there later still arrives. Payments already in stay.");
        // Asked in XMR: the headline is the XMR.
        let xmr = requests.add(draft(&keys, 11, Some(sam), None, Some(Asked { typed: "0.5".into(), unit: "XMR".into(), xmr: XMR / 2, fiat: None })), NOW).unwrap();
        let xmr = detail(&xmr, &book, &[], clock());
        assert_eq!((xmr.headline.as_str(), xmr.asked_line.as_str(), xmr.when.as_str()), ("0.5 XMR", "asked of Sam", "Today 12:00"));
        let any = requests.add(draft(&keys, 12, Some(sam), Some("Rent"), None), NOW).unwrap();
        assert_eq!(detail(&any, &book, &[], clock()).asked_line, "for Sam · Rent");
    }

    #[test]
    fn a_contact_s_requests_are_listed_and_summed_up() {
        let keys = keys();
        let (mut book, sam) = sam_book();
        let mum = book.add(Network::Stagenet, contacts::Draft { name: "Mum".into(), ..Default::default() }, NOW).unwrap().id;
        let mut requests = Book::default();
        let concert = requests.add(draft(&keys, 9, Some(sam), Some("Concert tickets"), usd("80", 1)), NOW - 3 * HOUR - 1800).unwrap();
        requests.add(draft(&keys, 6, Some(sam), Some("Pizza"), usd("12", 1)), NOW - 10 * DAY).unwrap();
        let lock = requests.add(draft(&keys, 12, Some(sam), None, None), NOW - DAY).unwrap();
        requests.add(draft(&keys, 13, None, Some("Not Sam's"), None), NOW).unwrap();
        let rows = [paid_in(9, XMR / 2, NOW - HOUR, Some(166_060_000)), paid_in(6, XMR / 10, NOW - 9 * DAY, None)];
        let lines: Vec<_> = on_card(sam, &requests, &rows, clock()).into_iter().map(|l| (l.note, l.line, l.state, l.received, l.asked)).collect();
        assert_eq!(
            lines,
            [
                ("Concert tickets".into(), "Address 9 · today 08:30".into(), "Received $83.03".into(), true, Some("asked $80.00".into())),
                ("Request".into(), "Address 12 · yesterday".into(), "Waiting".into(), false, None),
                ("Pizza".into(), "Address 6 · 21 Oct".into(), "Received 0.1 XMR".into(), true, Some("asked $12.00".into())),
            ] as [(String, String, String, bool, Option<String>); 3]
        );
        assert_eq!((addresses_note(3), addresses_note(1)), ("3 addresses".into(), "1 address".into()));
        assert_eq!(summary(sam, &requests, &rows), "2 requests paid, 1 waiting");
        assert_eq!(summary(mum, &requests, &rows), "No requests yet");
        let sheet: Vec<_> = sheet(&book, &requests, &rows).into_iter().map(|r| (r.name, r.line)).collect();
        assert_eq!(sheet, [("Mum".into(), "No requests yet".into()), ("Sam".into(), "2 requests paid, 1 waiting".into())] as [(String, String); 2]);
        // One of each, and a deleted one counts for nothing.
        requests.delete(concert.id, NOW);
        assert_eq!(summary(sam, &requests, &rows), "1 request paid, 1 waiting");
        requests.delete(lock.id, NOW);
        assert_eq!(summary(sam, &requests, &rows), "1 request paid");
        assert_eq!(summary(sam, &requests, &[]), "1 request waiting");
        assert_eq!(on_card(sam, &requests, &rows, clock()).len(), 1);
    }

    #[test]
    fn a_request_s_code_asks_for_what_it_asked_and_may_name_who_asks() {
        let keys = keys();
        let mut requests = Book::default();
        let r = requests.add(draft(&keys, 9, None, Some("Concert tickets"), usd("80", 481_700_000_000)), NOW).unwrap();
        let address = keys.address(9);
        assert_eq!(code_uri(&r, None), format!("monero:{address}?tx_amount=0.4817&tx_description=Concert%20tickets"));
        let named = code_uri(&r, Some("Henry H"));
        assert_eq!(named, format!("monero:{address}?tx_amount=0.4817&recipient_name=Henry%20H&tx_description=Concert%20tickets"));
        // Read back as the payer's Fob reads it.
        let read = uri::parse_payment(Network::Stagenet, &named).unwrap();
        assert_eq!((read.recipient_name.as_deref(), read.description.as_deref(), read.amount), (Some("Henry H"), Some("Concert tickets"), Some(481_700_000_000)));
    }
}
