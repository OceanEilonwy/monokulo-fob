//! Requests to be paid, in words.

use fob_screens::{clock::{Day, When}, requests::{self, Received, Request}};

use crate::{activity, clock};

/// A request's own screen as it's said.
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

pub fn detail(d: requests::Detail) -> Detail {
    let who = match (&d.asked, &d.name) {
        (Some(_), Some(n)) => format!("asked of {n}"),
        (Some(_), None) => "asked, no name".into(),
        (None, Some(n)) => format!("for {n}"),
        (None, None) => "No name".into(),
    };
    Detail {
        headline: d.asked.clone().unwrap_or_else(|| "Any amount".into()),
        asked_line: [Some(who), d.request.note.clone()].into_iter().flatten().collect::<Vec<_>>().join(" · "),
        when: [Some(clock::when(&d.when)), d.xmr_then.as_ref().map(|xmr| format!("{xmr} XMR at the time"))].into_iter().flatten().collect::<Vec<_>>().join(" · "),
        received_label: "Received".into(),
        received: match &d.received {
            Some(Received { fiat: Some(fiat), xmr, .. }) => format!("{fiat} · {xmr}"),
            Some(r) => r.xmr.clone(),
            None => "Waiting".into(),
        },
        received_any: d.received.is_some(),
        address_label: format!("Address {}", d.request.subaddress),
        address_short: d.address_short,
        payments_empty: d.payments.is_empty().then(|| "Nothing paid to it yet.".into()),
        payments: d.payments.into_iter().map(activity::row).collect(),
        delete_title: "Delete this request?".into(),
        delete_body: match &d.name {
            Some(n) => format!(
                "It leaves Activity and {n}'s card. Its address still works: anything paid there later still arrives, shown without a name. Payments already in stay."
            ),
            None => "It leaves Activity. Its address still works: anything paid there later still arrives. Payments already in stay.".into(),
        },
        request: d.request,
    }
}

/// One of a contact's requests, on their card, as it's said.
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

pub fn card_line(l: requests::CardLine) -> CardLine {
    CardLine {
        id: l.id,
        address: l.address,
        note: l.note.unwrap_or_else(|| "Request".into()),
        line: format!("Address {} · {}", l.subaddress, when_on_a_card(&l.when)),
        state: l.received.as_ref().map_or("Waiting".to_owned(), |text| format!("Received {text}")),
        received: l.received.is_some(),
        asked: l.asked.map(|text| format!("asked {text}")),
    }
}

fn when_on_a_card(when: &When) -> String {
    match &when.day {
        Day::Today => format!("today {}", when.time),
        day => clock::in_a_line(day),
    }
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
pub fn summary(paid: usize, waiting: usize) -> String {
    match (paid, waiting) {
        (0, 0) => "No requests yet".into(),
        (p, 0) => format!("{p} {} paid", requests_word(p)),
        (0, w) => format!("{w} {} waiting", requests_word(w)),
        (p, w) => format!("{p} {} paid, {w} waiting", requests_word(p)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fob_monero::{amount, fiat::{self, Record}, network::{Network, UNLOCK}};
    use fob_screens::{activity::{Activity, Direction}, clock::Clock, contacts, requests::{Asked, Book, Draft}};
    use fob_vault::keys::Keys;

    fn detail(request: &Request, book: &contacts::Book, rows: &[Activity], clock: Clock) -> Detail {
        super::detail(requests::detail(request, book, rows, clock))
    }

    fn activity_rows(requests: &Book, book: &contacts::Book, rows: &[Activity], clock: Clock) -> Vec<activity::Row> {
        fob_screens::requests::activity_rows(requests, book, rows, clock).into_iter().map(activity::row).collect()
    }

    fn activity_row(request: &Request, book: &contacts::Book, rows: &[Activity], clock: Clock) -> activity::Row {
        activity::row(requests::activity_row(request, book, rows, clock))
    }

    fn on_card(contact: u64, requests: &Book, rows: &[Activity], clock: Clock) -> Vec<CardLine> {
        fob_screens::requests::on_card(contact, requests, rows, clock).into_iter().map(card_line).collect()
    }

    fn said_summary(contact: u64, book: &Book, rows: &[Activity]) -> String {
        match requests::summary(contact, book, rows) {
            contacts::ContactLine::Requests { paid, waiting } => summary(paid, waiting),
            other => panic!("{other:?}"),
        }
    }

    fn sheet(book: &contacts::Book, requests: &Book, rows: &[Activity]) -> Vec<crate::contacts::Row> {
        fob_screens::requests::sheet(book, requests, rows).into_iter().map(crate::contacts::row).collect()
    }

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
            contact: None,
            contact_id: None,
            shared: None,
            parts: vec![],
        }
    }

    fn sam_book() -> (contacts::Book, u64) {
        let mut book = contacts::Book::default();
        let sam = book.add(Network::Stagenet, contacts::Draft { name: "Sam".into(), ..Default::default() }, NOW).unwrap();
        (book, sam.id)
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
        requests.delete(bike.id, NOW, &[]);
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
        assert_eq!(said_summary(sam, &requests, &rows), "2 requests paid, 1 waiting");
        assert_eq!(said_summary(mum, &requests, &rows), "No requests yet");
        let sheet: Vec<_> = sheet(&book, &requests, &rows).into_iter().map(|r| (r.name, r.line)).collect();
        assert_eq!(sheet, [("Mum".into(), "No requests yet".into()), ("Sam".into(), "2 requests paid, 1 waiting".into())] as [(String, String); 2]);
        // One of each, and a deleted one counts for nothing.
        requests.delete(concert.id, NOW, &rows);
        assert_eq!(said_summary(sam, &requests, &rows), "1 request paid, 1 waiting");
        requests.delete(lock.id, NOW, &rows);
        assert_eq!(said_summary(sam, &requests, &rows), "1 request paid");
        assert_eq!(said_summary(sam, &requests, &[]), "1 request waiting");
        assert_eq!(on_card(sam, &requests, &rows, clock()).len(), 1);
    }
}
