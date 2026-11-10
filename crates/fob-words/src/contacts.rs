//! Contacts, in words.

use fob_screens::{
    activity::Activity,
    contacts::{self, Contact, ContactLine, Draft, FoundWarning, PaidAtNote, PaidTimes, PaidTitle},
};

use crate::{activity, capitalised, clock, requests};

/// A contact in a list as it's said.
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

pub fn row(r: contacts::Row) -> Row {
    Row { id: r.id, name: r.name, initial: r.initial, photo: r.photo, send_address: r.send_address, line: line(&r.line) }
}

fn line(line: &ContactLine) -> String {
    match line {
        ContactLine::Note(note) => note.clone(),
        ContactLine::NotPaid => "Not paid yet".into(),
        ContactLine::Paid { note: Some(note), day } => format!("{note} · paid {}", clock::in_a_line(day)),
        ContactLine::Paid { note: None, day } => format!("Paid {}", clock::in_a_line(day)),
        ContactLine::Last(paid) => activity::paid_line(paid),
        ContactLine::Requests { paid, waiting } => requests::summary(*paid, *waiting),
    }
}

/// One of the addresses a contact was also paid at, as it's said.
#[derive(Clone, Debug, PartialEq)]
pub struct PaidAt {
    pub address: String,
    /// "84Bv9 … 2kLmQ … 99fQ2"
    pub short: String,
    /// "Not their send address · paid once, 25 Oct"
    pub line: String,
}

fn paid_at(a: contacts::PaidAt) -> PaidAt {
    let times = match &a.times {
        PaidTimes::Never => "not paid yet".to_owned(),
        PaidTimes::Once(day) => format!("paid once, {}", clock::in_a_line(day)),
        PaidTimes::Many { times, last } => format!("paid {times} times, last {}", clock::in_a_line(last)),
    };
    PaidAt { address: a.address, short: a.short, line: format!("Not their send address · {times}") }
}

/// A payment on a contact's card, as it's said.
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

fn paid(p: contacts::Paid) -> Paid {
    let title = match p.title {
        PaidTitle::Named(name) => name,
        PaidTitle::Sent => "Sent".into(),
        PaidTitle::From(name) => format!("From {name}"),
    };
    let when = [p.note, Some(clock::day(&p.day))].into_iter().flatten().collect::<Vec<_>>().join(" · ");
    Paid { activity: p.activity, title, when, amount: p.amount, fiat: p.fiat }
}

/// A contact's card as it's said.
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
    /// Half set up: the alert's title and line.
    pub half: Option<(String, String)>,
}

pub fn card(c: contacts::Card) -> Card {
    let name = c.contact.name.clone();
    Card {
        initial: c.initial,
        send_label: if c.send_short.is_some() { "Send address · you pay here" } else { "No send address yet" }.into(),
        send_button: if c.send_short.is_some() { "Change" } else { "Add" }.into(),
        send_short: c.send_short,
        requests_title: format!("Requests · {name} pays you"),
        requests_note: requests::addresses_note(c.requests.len()),
        requests: c.requests.into_iter().map(requests::card_line).collect(),
        also_paid_at: c.also_paid_at.into_iter().map(paid_at).collect(),
        also_note: format!("codes named {name}"),
        paid_title: if c.from_them { format!("Payments with {name}") } else { format!("Paid to {name}") },
        paid_empty: c.paid.is_empty().then(|| format!("Nothing paid to {name} yet.")),
        half: c.half.then(|| ("Half set up".into(), format!("{name} doesn't have an address from you yet. Meet to give them one, so what they pay you shows as from {name}."))),
        paid: c.paid.into_iter().map(paid).collect(),
        contact: c.contact,
    }
}

/// "Sam's" and "Sam", or "their" and "them" with no name yet.
fn whose(name: Option<&str>) -> (String, String) {
    match name.map(str::trim).filter(|n| !n.is_empty()) {
        Some(name) => (format!("{name}'s"), name.to_owned()),
        None => ("their".into(), "them".into()),
    }
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

/// What finding `f` says, as `name`'s send address.
pub fn found(f: contacts::Found, name: Option<&str>) -> Found {
    let (theirs, them) = whose(name);
    let warning = f.warning.map(|warning| match warning {
        FoundWarning::Same => capitalised(&format!("that's already {theirs} send address.")),
        FoundWarning::Replaces(current) => format!("This replaces {theirs} current send address ({current}). Check the pattern with {them}."),
    });
    Found { address: f.address, title: "Found an address".into(), short: f.short, warning }
}

/// What the Paid screen offers after a payment, as it's said.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Offer {
    /// The address paid.
    pub address: String,
    /// The contact a one-tap Add to would keep the address with. Always
    /// none now: a code's name proves nothing, so an address isn't filed
    /// with a contact on its word (the footnote names the contact it looks
    /// like instead). Kept so the apps' Add to stays for an offer that can
    /// one day prove the address is theirs.
    pub add_to: Option<Contact>,
    /// "Save as a contact"
    pub button: String,
    /// A second button's words, with Add to; none while there's no Add to.
    pub other: Option<String>,
    /// "to a code named “Sam”", for a code with a name.
    pub named: Option<String>,
    /// Under the buttons, for a code with a name.
    pub footnote: Option<String>,
    /// New contact, filled.
    pub draft: Draft,
}

/// Under a named code's buttons.
const NAMED_FOOTNOTE: &str =
    "The name comes from their code, not checked by Fob. The address is kept with the contact as one they were paid at, not as their send address.";

pub fn offer(o: contacts::Offer) -> Offer {
    // A code named like a contact whose addresses don't include this one:
    // say so, with the address, rather than offering to file it with them.
    let namesake = o.namesake.as_ref().map(|c| {
        format!(
            "This code said it was {}, but Fob can't check that, and {} hasn't been paid at {} before. It's saved as a new contact. If it really is theirs, set it on their card.",
            c.name,
            c.name,
            contacts::three_part(&o.address)
        )
    });
    Offer {
        button: "Save as a contact".to_owned(),
        other: None,
        named: o.name.as_ref().map(|n| format!("to a code named “{n}”")),
        footnote: namesake.or_else(|| o.name.as_ref().map(|_| NAMED_FOOTNOTE.to_owned())),
        add_to: None,
        address: o.address,
        draft: o.draft,
    }
}

/// Under the send address on New contact, saved with the addresses of
/// codes named for them: what happens to them.
pub fn paid_at_note(note: &PaidAtNote) -> String {
    match note {
        PaidAtNote::One(short) => format!("Paid at {short} from their code: kept with them, not as their send address."),
        PaidAtNote::Many(count) => format!("Paid at {count} addresses from their codes: kept with them, not as their send address."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fob_codes::receipt::ReceiptView;
    use fob_monero::{address::{self, short_title}, network::{Network, UNLOCK}};
    use fob_screens::{activity::Direction, clock::Clock, contacts::{Book, initial, new_contact, three_part}, requests as asked};
    use fob_vault::keys::Keys;

    fn list(book: &Book, rows: &[Activity], search: &str, clock: Clock) -> Vec<Row> {
        contacts::list(book, rows, search, clock).into_iter().map(row).collect()
    }

    fn on_send(book: &Book, rows: &[Activity], clock: Clock) -> Vec<Row> {
        contacts::on_send(book, rows, clock).into_iter().map(row).collect()
    }

    fn unsaved(book: &Book, rows: &[Activity], clock: Clock) -> Vec<activity::PaidBefore> {
        contacts::unsaved(book, rows, clock).into_iter().map(activity::paid_before).collect()
    }

    fn card(contact: &Contact, requests: &asked::Book, rows: &[Activity], clock: Clock) -> Card {
        super::card(contacts::card(contact, requests, rows, clock))
    }

    fn found(network: Network, name: Option<&str>, current: Option<&str>, text: &str) -> fob_screens::error::Result<Found> {
        contacts::found(network, current, text).map(|f| super::found(f, name))
    }

    fn offer(book: &Book, address: &str, name: Option<&str>) -> Option<Offer> {
        contacts::offer(book, address, name).map(super::offer)
    }

    fn paid_at_note(addresses: &[String]) -> Option<String> {
        contacts::paid_at_note(addresses).as_ref().map(super::paid_at_note)
    }

    /// 2026-10-31 12:00 UTC.
    const NOW: u64 = 1_793_448_000;
    const DAY: u64 = 86_400;

    fn clock() -> Clock {
        Clock { now: NOW, utc_offset: 0, hours_24: true }
    }

    fn address(n: u32) -> String {
        Keys::generate(Network::Stagenet).keys.address(n)
    }

    fn draft(name: &str) -> Draft {
        Draft { name: name.into(), ..Draft::default() }
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
        book.edit(Network::Stagenet, 1, Draft { send_address: Some(sam_address.clone()), ..draft("Sam") }, &|_| false).unwrap();
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
            total_value: 33.21,
        });
        let split = Activity { split: Some(3), ..paid(&address(5), 1, NOW) };
        let rows = [paid(&sam, 1, NOW), paid(&code, 1, NOW), paid(&stranger, 1_000_000_000, NOW - DAY), bought, paid(&stranger, 1, NOW - 9 * DAY), split];
        let shown: Vec<_> = unsaved(&book, &rows, clock()).into_iter().map(|p| (p.address, p.name, p.line)).collect();
        assert_eq!(
            shown,
            [
                (stranger.clone(), short_title(&stranger), "Paid yesterday · 0.001 XMR".to_owned()),
                // What was paid, not the receipt's $33.21.
                (shop.clone(), short_title(&shop), "Paid 25 Oct · 0.000000000001 XMR".to_owned()),
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
        lunch.price = Some(fob_monero::fiat::Record { currency: "USD".into(), price: 166_112_956, provider: "Kraken".into(), at: NOW, daily: false });
        let rows = vec![paid(&code, 98_000_000_000, NOW - 6 * DAY), lunch, paid(&address(3), 1, NOW)];
        let shown = card(&sam, &asked::Book::default(), &rows, clock());
        assert_eq!(shown.send_short.as_deref(), Some(three_part(&send).as_str()));
        assert_eq!((shown.send_label.as_str(), shown.send_button.as_str()), ("Send address · you pay here", "Change"));
        assert_eq!(shown.also_paid_at[0].line, "Not their send address · paid once, 25 Oct");
        assert_eq!(shown.also_note, "codes named Sam");
        assert_eq!(shown.paid_title, "Paid to Sam");
        let history: Vec<_> = shown.paid.iter().map(|p| (p.title.as_str(), p.when.as_str(), p.amount.as_str(), p.fiat.as_deref())).collect();
        assert_eq!(history, [("Lunch", "27 Oct", "−0.1204", Some("$20.00")), ("Sent", "25 Oct", "−0.098", None)]);
        assert_eq!(shown.paid_empty, None);
        assert!(shown.requests.is_empty());
        let none = card(&Contact { send_address: None, also_paid_at: vec![], ..sam.clone() }, &asked::Book::default(), &rows, clock());
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
        let mut reqs = asked::Book::default();
        let draft = |subaddress: u32, contact: Option<u64>, note: &str| asked::Draft {
            subaddress,
            address: keys.address(subaddress),
            contact,
            note: Some(note.into()),
            asked: None,
            name: None,
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
        // The name looks like Sam's, but the code's name proves nothing: no
        // one-tap Add to Sam (which filed anyone's address under Sam and made
        // later payments to it read as Sam's), only a new contact, and the
        // footnote says whose name it borrowed and shows the address.
        let o = offer(&book, &code, Some(" sAm ")).unwrap();
        assert_eq!(o.add_to, None);
        assert_eq!((o.button.as_str(), o.other.as_deref(), o.named.as_deref()), ("Save as a contact", None, Some("to a code named “sAm”")));
        assert_eq!(
            o.footnote.as_deref(),
            Some(&*format!(
                "This code said it was Sam, but Fob can't check that, and Sam hasn't been paid at {} before. It's saved as a new contact. If it really is theirs, set it on their card.",
                three_part(&code)
            ))
        );
        // A lookalike name is the same namesake.
        let lookalike = offer(&book, &code, Some("\u{405}am\u{2800}")).unwrap();
        assert!(lookalike.footnote.unwrap().starts_with("This code said it was Sam,"));
        assert_eq!(lookalike.add_to, None);
        assert_eq!(o.draft, Draft { name: "sAm".into(), note: None, photo: None, send_address: None, also_paid_at: vec![code.clone()] });
        // A name that's nobody's: Save as a contact, the address one they were paid at, never their send address.
        let o = offer(&book, &henry, Some("Henry")).unwrap();
        assert_eq!((o.add_to, o.button.as_str(), o.other), (None, "Save as a contact", None));
        assert_eq!(o.footnote.as_deref(), Some(NAMED_FOOTNOTE));
        assert_eq!((o.draft.name.as_str(), o.draft.send_address, o.draft.also_paid_at), ("Henry", None, vec![henry.clone()]));
        // No name: the address is their send address. A payment's note is
        // what it was for, never the contact's note.
        let o = offer(&book, &plain, None).unwrap();
        assert_eq!((o.named, o.footnote, o.button.as_str()), (None, None, "Save as a contact"));
        assert_eq!(o.draft, Draft { name: String::new(), note: None, photo: None, send_address: Some(plain.clone()), also_paid_at: vec![] });
        // A name that cleans to nothing counts as none.
        assert_eq!(offer(&book, &plain, Some("\n")).unwrap().draft.send_address.as_deref(), Some(plain.as_str()));
        // Already a contact's (send address or paid at): nothing.
        assert!(offer(&book, &sam_send, Some("Henry")).is_none());
        book.add_paid_at(Network::Stagenet, sam.id, &code).unwrap();
        assert!(offer(&book, &code, Some("Sam")).is_none());
        // Saving the offered draft keeps the address apart from the send address.
        let saved = book.add(Network::Stagenet, new_contact(&henry, Some("Henry")), NOW).unwrap();
        assert_eq!((saved.send_address, saved.also_paid_at), (None, vec![henry.clone()]));
        assert_eq!(
            paid_at_note(std::slice::from_ref(&henry)).unwrap(),
            format!("Paid at {} from their code: kept with them, not as their send address.", three_part(&henry))
        );
        assert_eq!(paid_at_note(&[]), None);
        assert!(paid_at_note(&[henry, code]).unwrap().starts_with("Paid at 2 addresses"));
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
        assert_eq!(f.warning.unwrap(), format!("This replaces Sam's current send address ({}). Check the pattern with Sam.", address::short(&old)));
        assert_eq!(
            found(Network::Stagenet, None, Some(&old), &new).unwrap().warning.unwrap(),
            format!("This replaces their current send address ({}). Check the pattern with them.", address::short(&old))
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

    #[test]
    fn an_address_paid_again_says_how_often() {
        let said = |times| paid_at(contacts::PaidAt { address: String::new(), short: String::new(), times }).line;
        assert_eq!(said(PaidTimes::Never), "Not their send address · not paid yet");
        assert_eq!(said(PaidTimes::Once(fob_screens::clock::Day::Date("25 Oct".into()))), "Not their send address · paid once, 25 Oct");
        assert_eq!(said(PaidTimes::Many { times: 2, last: fob_screens::clock::Day::Yesterday }), "Not their send address · paid 2 times, last yesterday");
    }
}
