use super::*;

#[tokio::test]
async fn two_wallets_without_profiles_meet_by_code_and_name_each_other() {
    let ann = Wallet::create(&dir("meet-nameless-a"), [1; 32], Network::Stagenet).unwrap();
    let bo = Wallet::create(&dir("meet-nameless-b"), [2; 32], Network::Stagenet).unwrap();
    let (ann_share, first) = ann.share_profile().await.unwrap();
    let (bo_share, _) = bo.share_profile().await.unwrap();
    assert!(ann.start_swap(ann_share.id).await.is_ok(), "touching needs no profile either");

    // Bo scans Ann's code and names her, then shows the reply.
    assert!(bo.said_scanned(&first).await.is_err(), "it needs a name first");
    let named = uri::name_meet_code(Network::Stagenet, &first, "Ann").unwrap();
    let contact = bo.add_contact_from_profile(&named, false, true).await.unwrap();
    assert_eq!((contact.name.as_str(), contact.send_address.as_deref(), contact.half), ("Ann", Some(ann_share.address.as_str()), false));
    bo.share_met(bo_share.id, "Ann").await.unwrap();

    let reply = bo.share_reply_code(bo_share.id).await.unwrap();
    assert!(!reply.contains("recipient_name"), "no name goes out");
    let named = uri::name_meet_code(Network::Stagenet, &reply, "Bo").unwrap();
    assert!(ann.said_scanned(&named).await.unwrap().code.reply);
    let contact = ann.add_contact_from_profile(&named, false, true).await.unwrap();
    assert_eq!(contact.send_address.as_deref(), Some(bo_share.address.as_str()));
    assert!(ann.profile().await.is_none() && bo.profile().await.is_none(), "neither has had to make one");
}

#[tokio::test]
async fn contacts_are_kept_in_the_wallet_file_and_name_payments() {
    let path = dir("contacts");
    let wallet = Wallet::create(&path, [1; 32], Network::Stagenet).unwrap();
    let t = now();
    let clock = Clock { now: t, utc_offset: 0, hours_24: true };
    let (sam_address, other) = (wallet.inner.keys.address(7), wallet.inner.keys.address(8));
    let photo = fob_codes::names::test_webp(400, 96);
    let sam = wallet
        .add_contact(contacts::Draft { name: "Sam".into(), note: Some("Lunch money".into()), photo: Some(photo.clone()), send_address: Some(sam_address.clone()), ..Default::default() })
        .await
        .unwrap();
    // A refused change leaves the file as it was.
    assert!(wallet.edit_contact(sam.id, contacts::Draft { name: String::new(), ..Default::default() }).await.is_err());
    {
        let mut state = wallet.inner.state.lock().await;
        state.sent.push(Sent { not_confirmed: false, released: false, tx: "lunch".into(), to: sam_address.clone(), amount: XMR / 10, fee: 1, change: 0, time: t, height: None, receipt: None, note: Some("Lunch".into()), pieces: None, back: vec![], recipient_name: None });
        state.sent.push(Sent { not_confirmed: false, released: false, tx: "else".into(), to: other.clone(), amount: XMR / 10, fee: 1, change: 0, time: t - 10, height: None, receipt: None, note: None, pieces: None, back: vec![], recipient_name: None });
        wallet.save_state(&state).unwrap();
    }
    let reopened = Wallet::open(&path, [1; 32]).unwrap();
    assert_eq!(reopened.contact(sam.id).await, Some(sam.clone()));
    assert_eq!(reopened.contact(sam.id).await.unwrap().photo, Some(photo));
    // Activity calls the payment to Sam's address "Sam".
    let lists = reopened.said_lists(clock).await;
    let days = lists.days;
    let titles: Vec<_> = days[0].rows.iter().map(|r| r.title.as_str()).collect();
    assert_eq!(titles, ["Sam", &crate::address::short_title(&other)]);
    assert_eq!(reopened.said_payment("lunch", clock).await.unwrap().party.as_deref(), Some("Sam"));
    assert_eq!(reopened.said_contacts("lunch", clock).await[0].line, "Lunch money · paid today");
    assert_eq!(lists.on_send[0].line, "Lunch · Today");
    assert_eq!(lists.unsaved.iter().map(|p| p.address.as_str()).collect::<Vec<_>>(), [other.as_str()]);
    assert_eq!(reopened.said_card(sam.id, clock).await.unwrap().paid.len(), 1);
    // Deleted, the payment stays, shown by its address again.
    reopened.delete_contact(sam.id).await.unwrap();
    assert!(reopened.said_card(sam.id, clock).await.is_none());
    let rows = Wallet::open(&path, [1; 32]).unwrap().activity().await;
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|r| r.contact.is_none()));
}

#[tokio::test]
async fn the_paid_screen_offers_to_save_whoever_a_payment_went_to() {
    let wallet = Wallet::create(&dir("save-offer"), [1; 32], Network::Stagenet).unwrap();
    let t = now();
    let (sam_address, code, henry, plain) = (wallet.inner.keys.address(7), wallet.inner.keys.address(8), wallet.inner.keys.address(9), wallet.inner.keys.address(10));
    let sam = wallet.add_contact(contacts::Draft { name: "Sam".into(), send_address: Some(sam_address.clone()), ..Default::default() }).await.unwrap();
    let sent = |tx: &str, to: &str, time: u64, name: Option<&str>| Sent {
        not_confirmed: false,
        released: false,
        tx: tx.into(),
        to: to.into(),
        amount: 1,
        fee: 1,
        change: 0,
        time,
        height: None,
        receipt: None,
        note: Some("Pizza".into()),
        pieces: None,
        back: vec![],
        recipient_name: name.map(Into::into),
    };
    {
        let mut state = wallet.inner.state.lock().await;
        state.sent.push(sent("to-sam", &sam_address, t, None));
        state.sent.push(sent("named-sam", &code, t, Some("Sam")));
        state.sent.push(sent("named-henry", &henry, t - 20, Some("Henry")));
        state.sent.push(sent("henry-again", &henry, t, None));
        state.sent.push(sent("plain", &plain, t, None));
        state.sent.push(Sent { pieces: Some(3), ..sent("split", &plain, t, None) });
    }
    assert!(wallet.said_offer("to-sam").await.is_none());
    assert!(wallet.said_offer("split").await.is_none());
    assert!(wallet.said_offer("nothing").await.is_none());
    let add = wallet.said_offer("named-sam").await.unwrap();
    assert_eq!((add.add_to.map(|c| c.id), add.button.as_str()), (Some(sam.id), "Add to Sam"));
    let plain_offer = wallet.said_offer("plain").await.unwrap();
    assert_eq!(plain_offer.draft.send_address.as_deref(), Some(plain.as_str()));
    // "Paid, not saved": an address once paid with a named code is only ever paid at.
    let henry_draft = wallet.new_contact_for(&henry).await;
    assert_eq!((henry_draft.name.as_str(), henry_draft.send_address, henry_draft.also_paid_at), ("Henry", None, vec![henry.clone()]));
    assert_eq!(wallet.new_contact_for(&plain).await.send_address.as_deref(), Some(plain.as_str()));
    // So too on Paid after paying it again without the name (from Paid before, or pasted).
    let again = wallet.said_offer("henry-again").await.unwrap();
    assert_eq!((again.draft.name.as_str(), again.draft.send_address, again.draft.also_paid_at), ("Henry", None, vec![henry.clone()]));
    assert_eq!(again.named.as_deref(), Some("to a code named “Henry”"));
    // Added to Sam, the payment is Sam's and nothing more is offered.
    wallet.add_contact_paid_at(sam.id, &add.address).await.unwrap();
    assert!(wallet.said_offer("named-sam").await.is_none());
    let rows = wallet.activity().await;
    assert_eq!(rows.iter().find(|r| r.tx == "named-sam").and_then(|r| r.contact.as_deref()), Some("Sam"));
    // A payment saved before names were kept reads without one.
    let mut old = serde_json::to_value(sent("old", &plain, t, Some("x"))).unwrap();
    old.as_object_mut().unwrap().remove("recipient_name");
    assert_eq!(serde_json::from_value::<Sent>(old).unwrap().recipient_name, None);
}

#[tokio::test]
async fn a_request_is_made_only_on_share_each_on_an_address_no_one_had() {
    let path = dir("requests");
    let wallet = Wallet::create(&path, [1; 32], Network::Stagenet).unwrap();
    let address = |i: u32| wallet.inner.keys.address(i);
    // Opening Receive and building its code makes nothing.
    let own = wallet.receive_address().await.unwrap();
    assert_eq!(own, address(1));
    wallet.receive_code(&own, Some("0.5"), false, Some("Bike")).await.unwrap();
    assert!(wallet.inner.state.lock().await.requests.live().next().is_none());
    // Splits have the next one; a request skips it.
    assert_eq!(wallet.split_address().await.unwrap(), address(2));
    let sam = wallet.add_contact(contacts::Draft { name: "Sam".into(), ..Default::default() }).await.unwrap();
    let (first, code) = wallet.make_request(Some(sam.id), Some("0.5"), false, Some(" Concert tickets ")).await.unwrap();
    assert_eq!((first.subaddress, first.address.as_str(), first.contact), (3, address(3).as_str(), Some(sam.id)));
    assert_eq!(code.uri, format!("monero:{}?tx_amount=0.5&tx_description=Concert%20tickets", address(3)));
    assert_eq!((code.headline.as_deref(), code.detail.as_deref()), (Some("0.5 XMR"), Some("Concert tickets")));
    // Receive keeps its own; its "New address" never gives a request's.
    assert_eq!((wallet.receive_address().await.unwrap(), wallet.receive_index().await), (own.clone(), 1));
    assert_eq!(wallet.new_receive_address().await.unwrap(), address(4));
    let (second, _) = wallet.make_request(None, None, false, None).await.unwrap();
    assert_eq!((second.subaddress, second.contact, second.note, second.asked), (5, None, None, None));
    assert_eq!(wallet.receive_address().await.unwrap(), address(4));
    // A deleted contact can't be asked; nothing is made.
    assert!(wallet.make_request(Some(99), None, false, None).await.is_err());
    assert!(wallet.make_request(None, Some("abc"), false, None).await.is_err());
    assert_eq!(wallet.inner.state.lock().await.requests.live().count(), 2);
    // Kept in the file, as made.
    let again = Wallet::open(&path, [1; 32]).unwrap();
    assert_eq!(again.receive_address().await.unwrap(), address(4));
    assert_eq!(again.request_code(first.id).await.unwrap(), code);
    // The same ask again while nothing came to it (Share, then Copy; a second tap): the same request.
    let (same, same_code) = again.make_request(None, None, false, None).await.unwrap();
    assert_eq!((same.id, same.subaddress), (second.id, 5));
    assert_eq!(same_code.uri, format!("monero:{}", address(5)));
    // Once something came to it, a new one.
    again.inner.state.lock().await.outputs.push(Owned { subaddress: 5, ..received("paid", now(), 1) });
    assert_eq!(again.make_request(None, None, false, None).await.unwrap().0.subaddress, 6);
    assert_eq!(again.make_request(None, None, false, Some("Bike")).await.unwrap().0.subaddress, 7);
    assert_eq!(again.inner.state.lock().await.requests.live().count(), 4);
}

#[tokio::test]
async fn a_request_asked_in_money_keeps_what_it_came_to_then() {
    let wallet = Wallet::create(&dir("request-fiat"), [1; 32], Network::Stagenet).unwrap();
    assert!(matches!(wallet.make_request(None, Some("80"), true, None).await, Err(FobError::Screen(ScreenError::NoPriceToConvert))));
    wallet.refresh_prices_from(&FakePrices::new(166_060_000, vec![]), Asked::Background, now()).await.unwrap();
    let (request, code) = wallet.make_request(None, Some("80"), true, Some("Concert tickets")).await.unwrap();
    let asked = request.asked.clone().unwrap();
    assert_eq!((asked.typed.as_str(), asked.unit.as_str(), asked.xmr, asked.fiat.as_deref()), ("80", "USD", 481_750_000_000, Some("$80.00")));
    assert_eq!((code.headline.as_deref(), code.detail.as_deref()), (Some("$80.00"), Some("0.48175 XMR · Concert tickets")));
    // A new price changes nothing about it.
    wallet.refresh_prices_from(&FakePrices::new(200_000_000, vec![]), Asked::User, now() + 120).await.unwrap();
    assert_eq!(wallet.request_code(request.id).await.unwrap(), code);
    assert_eq!(wallet.said_request(request.id, Clock { now: now(), utc_offset: 0, hours_24: true }).await.unwrap().headline, "$80.00");
}

#[tokio::test]
async fn a_request_shows_what_came_and_once_deleted_its_address_still_receives_unnamed() {
    let path = dir("request-delete");
    let wallet = Wallet::create(&path, [1; 32], Network::Stagenet).unwrap();
    let t = now();
    let clock = Clock { now: t, utc_offset: 0, hours_24: true };
    let sam = wallet.add_contact(contacts::Draft { name: "Sam".into(), ..Default::default() }).await.unwrap();
    let (request, _) = wallet.make_request(Some(sam.id), Some("0.5"), false, Some("Pizza")).await.unwrap();
    let (other, _) = wallet.make_request(Some(sam.id), None, false, None).await.unwrap();
    assert_eq!(wallet.said_request_contacts().await[0].line, "2 requests waiting");
    let to_request = |tx: &str, time: u64, amount: u64| Owned { subaddress: request.subaddress, ..received(tx, time, amount) };
    wallet.inner.state.lock().await.outputs.push(to_request("first", t - 60, XMR / 4));
    // One still in the pool counts too.
    wallet.pool_found([("pooled".into(), Some(request.subaddress), XMR / 10)], t - 30);
    let detail = wallet.said_request(request.id, clock).await.unwrap();
    assert_eq!((detail.received.as_str(), detail.payments.len()), ("0.35 XMR", 2));
    assert!(detail.payments.iter().all(|p| p.title == "From Sam"), "{:?}", detail.payments);
    assert_eq!(wallet.said_request_contacts().await[0].line, "1 request paid, 1 waiting");
    let days = wallet.said_lists(clock).await.days;
    let today: Vec<_> = days[0].rows.iter().map(|r| (r.title.clone(), r.request)).collect();
    assert!(today.contains(&("Request to Sam".into(), Some(request.id))), "{today:?}");
    assert!(today.contains(&("From Sam".into(), None)), "{today:?}");
    let card = wallet.said_card(sam.id, clock).await.unwrap();
    assert_eq!((card.requests.len(), card.paid.len(), card.paid_title.as_str()), (2, 2, "Payments with Sam"));

    // Deleted: gone from Activity and Sam's card; what came before is still Sam's, the payment still in the
    // pool too, even mined in a block whose time is after the delete.
    wallet.delete_request(request.id).await.unwrap();
    wallet.delete_request(request.id).await.unwrap();
    wallet.inner.pool.lock().unwrap().clear();
    wallet.inner.state.lock().await.outputs.push(to_request("pooled", t + 600, XMR / 10));
    assert_eq!(wallet.activity().await.iter().find(|r| r.tx == "pooled").and_then(|r| r.contact.clone()), Some("Sam".into()));
    assert!(wallet.said_request(request.id, clock).await.is_none());
    assert!(wallet.request_code(request.id).await.is_none());
    wallet.inner.state.lock().await.outputs.push(to_request("later", t + 3600, XMR));
    let reopened = Wallet::open(&path, [1; 32]).unwrap();
    reopened.inner.state.lock().await.outputs.push(to_request("later", t + 3600, XMR));
    let rows = reopened.activity().await;
    let from = |tx: &str| rows.iter().find(|r| r.tx == tx).map(|r| (r.contact.clone(), r.note.clone()));
    assert_eq!(from("first"), Some((Some("Sam".into()), Some("Pizza".into()))));
    assert_eq!(from("later"), Some((None, Some("Pizza".into()))));
    let days = reopened.said_lists(Clock { now: t + 3600, ..clock }).await.days;
    assert!(days.iter().flat_map(|d| &d.rows).all(|r| r.request != Some(request.id)));
    let card = reopened.said_card(sam.id, clock).await.unwrap();
    assert_eq!(card.requests.iter().map(|r| r.id).collect::<Vec<_>>(), [other.id]);
    assert_eq!(card.paid.iter().map(|p| p.activity.tx.as_str()).collect::<Vec<_>>(), ["first"]);
    // Its address is still scanned, and never handed out again.
    let state = reopened.inner.state.lock().await;
    assert!(state.receive_index >= request.subaddress);
    drop(state);
    assert_ne!(reopened.new_receive_address().await.unwrap(), request.address);
}

#[tokio::test]
async fn one_transaction_paying_two_requests_credits_each_its_own_part() {
    let wallet = Wallet::create(&dir("request-parts"), [1; 32], Network::Stagenet).unwrap();
    let t = now();
    let clock = Clock { now: t, utc_offset: 0, hours_24: true };
    let sam = wallet.add_contact(contacts::Draft { name: "Sam".into(), ..Default::default() }).await.unwrap();
    let jo = wallet.add_contact(contacts::Draft { name: "Jo".into(), ..Default::default() }).await.unwrap();
    let (for_sam, _) = wallet.make_request(Some(sam.id), None, false, Some("Rent")).await.unwrap();
    let (for_jo, _) = wallet.make_request(Some(jo.id), None, false, Some("Bills")).await.unwrap();
    let received_by = |id: u64| {
        let wallet = wallet.clone();
        async move { wallet.said_request(id, clock).await.unwrap().received }
    };
    // Seen in the pool, the way a pool scan keeps it: a batch payout to both, and one to the main address in it.
    wallet.pool_found(
        [("batch".into(), Some(for_sam.subaddress), XMR / 2), ("batch".into(), None, 7), ("batch".into(), Some(for_jo.subaddress), XMR / 4)],
        t - 60,
    );
    assert_eq!(received_by(for_sam.id).await, "0.5 XMR");
    assert_eq!(received_by(for_jo.id).await, "0.25 XMR");
    let row = wallet.activity().await.into_iter().find(|r| r.tx == "batch").unwrap();
    assert_eq!((row.amount, row.parts.len()), (XMR / 2 + XMR / 4 + 7, 3));
    assert!(wallet.inner.state.lock().await.requests.at(0).is_none(), "the main address is no request's");
    // Mined: the same, from its outputs.
    wallet.inner.pool.lock().unwrap().clear();
    {
        let mut state = wallet.inner.state.lock().await;
        state.outputs.push(Owned { subaddress: for_sam.subaddress, key: "a".into(), ..received("batch", t, XMR / 2) });
        state.outputs.push(Owned { subaddress: for_jo.subaddress, key: "b".into(), ..received("batch", t, XMR / 4) });
    }
    assert_eq!(received_by(for_sam.id).await, "0.5 XMR");
    assert_eq!(received_by(for_jo.id).await, "0.25 XMR");
    let detail = wallet.said_request(for_jo.id, clock).await.unwrap();
    assert_eq!(detail.payments.iter().map(|p| p.amount.as_str()).collect::<Vec<_>>(), ["+0.25"]);
    assert_eq!(wallet.said_request_contacts().await.iter().map(|c| c.line.as_str()).collect::<Vec<_>>(), ["1 request paid", "1 request paid"]);
}

#[tokio::test]
async fn each_share_of_the_profile_is_a_new_address_and_its_own_line() {
    let path = dir("profile-shares");
    let wallet = Wallet::create(&path, [1; 32], Network::Stagenet).unwrap();
    let t = now();
    let clock = Clock { now: t, utc_offset: 0, hours_24: true };
    let address = |i: u32| wallet.inner.keys.address(i);
    assert_eq!(wallet.said_profile_row().await.title, "Add a shareable Fob contact");
    wallet.set_profile("Henry", Some(webp(300))).await.unwrap();
    let own = wallet.receive_address().await.unwrap();
    let (first, code) = wallet.share_profile().await.unwrap();
    let (second, _) = wallet.share_profile().await.unwrap();
    assert_eq!((first.subaddress, second.subaddress), (2, 3));
    assert_eq!(code, uri::profile_uri(&address(2), "Henry", None), "a shown code carries no photo");
    // Receive keeps its own address, and New address never gives a share's.
    assert_eq!(wallet.receive_address().await.unwrap(), own);
    assert_eq!(wallet.new_receive_address().await.unwrap(), address(4));
    wallet.share_went_out(first.id, profile::ShareKind::LinkCopied).await.unwrap();

    // A payment to a share's address comes "Via your profile".
    wallet.inner.state.lock().await.outputs.push(Owned { subaddress: 2, ..received("via", t - 60, XMR / 4) });
    let days = wallet.said_lists(clock).await.days;
    let rows: Vec<_> = days[0].rows.iter().map(|r| (r.title.as_str(), r.share, r.amount.as_str())).collect();
    assert!(rows.iter().all(|r| !r.0.starts_with("Shared your") && !r.0.starts_with("Met")), "shares nobody took aren't in Activity: {rows:?}");
    wallet.share_met(second.id, "Sam").await.unwrap();
    let met = wallet.said_lists(clock).await.days[0].rows.iter().filter(|r| r.share == Some(second.id)).map(|r| r.title.clone()).collect::<Vec<_>>();
    assert_eq!(met, ["Met Sam"]);
    let via = days[0].rows.iter().find(|r| r.title == "Via your profile").unwrap();
    let shared = clock::time_of_day(clock::local(first.created, clock).1, clock);
    assert!(via.line.starts_with(&format!("Shared {shared} · ")), "{}", via.line);
    let detail = wallet.said_payment("via", clock).await.unwrap();
    assert_eq!(detail.party.as_deref(), Some("Address 2 · via your profile"));
    let lines = wallet.said_shares(clock).await;
    assert_eq!(lines.iter().map(|l| (l.title.as_str(), l.received.as_str())).collect::<Vec<_>>(), [("Met Sam", "nothing yet"), ("Link copied", "+0.25 XMR")]);

    // Kept in the file; a share's code again is the profile as it is now.
    let again = Wallet::open(&path, [1; 32]).unwrap();
    again.set_profile("Hen", None).await.unwrap();
    assert_eq!(again.share_code(first.id).await.unwrap().1, uri::profile_uri(&address(2), "Hen", None));
    assert_eq!(again.share_profile().await.unwrap().0.subaddress, 5);
    assert!(again.share_code(99).await.is_none());
}

#[tokio::test]
async fn touching_phones_works_without_a_name_and_the_share_is_named_once_chosen() {
    let ann = Wallet::create(&dir("touch-nameless"), [1; 32], Network::Stagenet).unwrap();
    let sam = Wallet::create(&dir("touch-named"), [2; 32], Network::Stagenet).unwrap();
    sam.set_profile("Sam", None).await.unwrap();
    let (_, ours) = share_and_swap(&ann).await;
    let (shown, theirs) = share_and_swap(&sam).await;
    assert!(matches!(touch(&ours, &theirs), swap::Step::Done(_)));

    let (_, got, _) = ann.end_swap(&ours).await.unwrap().unwrap();
    assert_eq!(got.code.name, "Sam");
    let (share, got, _) = sam.end_swap(&theirs).await.unwrap().unwrap();
    assert_eq!(got.code.name, "", "Ann's phone sent no name");
    assert_eq!(share.kind, profile::ShareKind::TouchedPhones { name: None });

    let text = uri::nameless_uri(&got.code.address, false);
    let named = uri::name_meet_code(Network::Stagenet, &text, "Ann").unwrap();
    assert_eq!(sam.add_contact_from_profile(&named, false, true).await.unwrap().name, "Ann");
    sam.share_met(shown.id, "Ann").await.unwrap();
    assert_eq!(sam.share_code(shown.id).await.unwrap().0.kind, profile::ShareKind::Met { name: "Ann".into() });
}

#[tokio::test]
async fn touching_phones_swaps_profiles_on_the_share_s_own_address() {
    let path = dir("swap-henry");
    let henry = Wallet::create(&path, [1; 32], Network::Stagenet).unwrap();
    let sam = Wallet::create(&dir("swap-sam"), [2; 32], Network::Stagenet).unwrap();
    let clock = Clock { now: now(), utc_offset: 0, hours_24: true };
    henry.set_profile("Henry", Some(webp(1200))).await.unwrap();
    sam.set_profile("Sam", None).await.unwrap();
    assert!(henry.start_swap(99).await.is_err(), "no such share");
    let own = henry.receive_address().await.unwrap();

    let (shown, ours) = share_and_swap(&henry).await;
    let (_, theirs) = share_and_swap(&sam).await;
    // The swap is on the share's address: one opening, one address.
    assert_eq!((ours.subaddress(), ours.address()), (2, shown.address.as_str()));
    assert_eq!(henry.new_receive_address().await.unwrap(), henry.inner.keys.address(3));
    assert_ne!(own, henry.inner.keys.address(2));

    // Henry's phone reads Sam's: both have the other's.
    assert!(matches!(touch(&ours, &theirs), swap::Step::Done(ref p) if p.name == "Sam"));
    let (share, scanned, sure) = henry.end_swap(&ours).await.unwrap().unwrap();
    assert!(sure, "Henry's phone read Sam's, which said it took Henry's");
    assert_eq!((share.id, share.subaddress, share.kind.clone()), (shown.id, 2, profile::ShareKind::TouchedPhones { name: Some("Sam".into()) }));
    assert_eq!((scanned.code.address.as_str(), scanned.clash.is_none()), (sam.inner.keys.address(theirs.subaddress()).as_str(), true));
    let (_, got, sure) = sam.end_swap(&theirs).await.unwrap().unwrap();
    assert!(!sure, "Sam's phone, the card, can't know its last answer arrived");
    assert_eq!((got.code.name.as_str(), got.code.photo.clone(), got.code.address.clone()), ("Henry", Some(webp(1200)), henry.inner.keys.address(2)));
    // Ended twice (success, then the screen closing): kept once.
    assert!(henry.end_swap(&ours).await.unwrap().is_none());
    assert_eq!(henry.said_shares(clock).await.iter().map(|l| l.title.as_str()).collect::<Vec<_>>(), ["Touched phones with Sam"]);
    // Adding Sam goes the way a scanned profile does, asking when he clashes.
    let text = uri::profile_uri(&got.code.address, &got.code.name, got.code.photo.as_deref());
    sam.add_contact_from_profile(&text, false, false).await.unwrap();
    let (_, again) = share_and_swap(&sam).await;
    let (_, ours) = share_and_swap(&henry).await;
    touch(&again, &ours);
    let (_, rescanned, _) = sam.end_swap(&again).await.unwrap().unwrap();
    assert_eq!(rescanned.clash.unwrap().why, profile::ClashWhy::SameName);
    assert_eq!(henry.end_swap(&ours).await.unwrap().unwrap().0.kind, profile::ShareKind::TouchedPhones { name: Some("Sam".into()) });

    // Nobody touched: the share stays "Code shown".
    let (idle_share, idle) = share_and_swap(&henry).await;
    assert!(henry.end_swap(&idle).await.unwrap().is_none());
    // Ours went out whole but theirs never came: "Touched phones".
    let (_, half) = share_and_swap(&henry).await;
    let (_, other) = share_and_swap(&sam).await;
    let mut apdu = other.reader_start().unwrap();
    while let swap::Step::Send(next) = other.reader_next(&half.card_answer(&apdu)) {
        if next[1] == 0xE2 {
            break; // gone before done
        }
        apdu = next;
    }
    half.card_lost();
    let none = henry.end_swap(&half).await.unwrap();
    assert!(none.is_none());
    let lines = henry.said_shares(clock).await;
    let titles: Vec<&str> = lines.iter().map(|l| l.title.as_str()).collect();
    assert_eq!(titles, ["Touched phones", "Code shown", "Touched phones with Sam", "Touched phones with Sam"]);
    assert_eq!(lines[1].address, idle_share.address);
    let reopened = Wallet::open(&path, [1; 32]).unwrap();
    assert_eq!(reopened.said_shares(clock).await.len(), 4);
    // Four openings, four addresses (2, 4, 5, 6; 3 was Receive's).
    assert_eq!(half.subaddress(), 6);
    assert_eq!(reopened.new_receive_address().await.unwrap(), reopened.inner.keys.address(7));
}

#[tokio::test]
async fn only_the_share_just_made_swaps_and_only_until_its_address_goes_out() {
    let henry = Wallet::create(&dir("swap-fresh"), [1; 32], Network::Stagenet).unwrap();
    henry.set_profile("Henry", None).await.unwrap();
    let can_touch = |share: &profile::Share| {
        let henry = henry.clone();
        let id = share.id;
        async move { henry.share_code(id).await.unwrap().2 }
    };
    // Swapped with Sam, then the screen closed: reopened from Your shares, it neither swaps nor offers to.
    let (first, swap) = share_and_swap(&henry).await;
    assert!(can_touch(&first).await);
    henry.end_swap(&swap).await.unwrap();
    assert!(!can_touch(&first).await);
    assert!(henry.start_swap(first.id).await.is_err(), "reopened");
    // Shown but nobody touched, then a newer share made: the older one is done with.
    let (second, _) = henry.share_profile().await.unwrap();
    let (third, _) = henry.share_profile().await.unwrap();
    assert!(henry.start_swap(second.id).await.is_err(), "an older share");
    assert!(can_touch(&third).await);
    // Its link copied: its address is with someone, so the running swap ends and no other starts.
    let swap = henry.start_swap(third.id).await.unwrap();
    henry.share_went_out(third.id, profile::ShareKind::LinkCopied).await.unwrap();
    assert_eq!(swap.card_answer(&swap::select()), swap::NOT_AVAILABLE.to_vec());
    assert_eq!(swap.reader_start(), None);
    assert!(!can_touch(&third).await);
    assert!(henry.start_swap(third.id).await.is_err());
    // A swap that ended with nothing (the screen paused) can start again while the share is fresh.
    let (fourth, swap) = share_and_swap(&henry).await;
    drop(swap);
    assert!(henry.start_swap(fourth.id).await.is_ok());
}

#[tokio::test]
async fn requests_carry_the_profile_s_name_while_the_switch_is_on() {
    let wallet = Wallet::create(&dir("profile-name-in-requests"), [1; 32], Network::Stagenet).unwrap();
    let (bare, code) = wallet.make_request(None, None, false, None).await.unwrap();
    assert!(!code.uri.contains("recipient_name"));
    assert!(wallet.set_name_in_requests(true).await.is_err(), "no profile yet");
    wallet.set_profile("Henry", None).await.unwrap();
    let (_, code) = wallet.make_request(None, Some("0.5"), false, None).await.unwrap();
    assert_eq!(code.uri, format!("monero:{}?tx_amount=0.5&recipient_name=Henry", wallet.inner.keys.address(2)));
    // A request's code again is the one that went out: no name added since, none taken away after.
    assert!(!wallet.request_code(bare.id).await.unwrap().uri.contains("recipient_name"));
    let (named, _) = wallet.make_request(None, None, false, Some("Named")).await.unwrap();
    wallet.set_profile("Hen", None).await.unwrap();
    assert!(wallet.request_code(named.id).await.unwrap().uri.contains("recipient_name=Henry"));
    wallet.set_name_in_requests(false).await.unwrap();
    assert!(wallet.request_code(named.id).await.unwrap().uri.contains("recipient_name=Henry"));
    assert!(!wallet.make_request(None, None, false, None).await.unwrap().1.uri.contains("recipient_name"));
    assert!(!wallet.profile().await.unwrap().name_in_requests);
}

#[tokio::test]
async fn a_contact_scanned_on_its_own_is_half_set_up_until_met() {
    let wallet = Wallet::create(&dir("profile-half"), [1; 32], Network::Stagenet).unwrap();
    let other = Keys::generate(Network::Stagenet).keys;
    let clock = Clock { now: now(), utc_offset: 0, hours_24: true };
    let code = uri::profile_uri(&other.address(4), "Ana", None);
    let ana = wallet.add_contact_from_profile(&code, false, false).await.unwrap();
    assert!(wallet.said_card(ana.id, clock).await.unwrap().half.is_some());
    // Meeting (a touch, or both codes) clears it, whether Meet updates or opens them.
    let newer = uri::profile_uri_with(&other.address(9), "Ana", None, true);
    assert!(uri::parse_profile(Network::Stagenet, &newer).unwrap().reply);
    wallet.update_contact_from_profile(ana.id, &newer, true).await.unwrap();
    assert!(wallet.said_card(ana.id, clock).await.unwrap().half.is_none());
    // Added by Meet, never half set up.
    let sam = wallet.add_contact_from_profile(&uri::profile_uri(&other.address(5), "Sam", None), false, true).await.unwrap();
    assert!(wallet.said_card(sam.id, clock).await.unwrap().half.is_none());
    // The mark survives the wallet file.
    let ben = wallet.add_contact_from_profile(&uri::profile_uri(&other.address(6), "Ben", None), false, false).await.unwrap();
    assert!(wallet.contact(ben.id).await.unwrap().half);
}

#[tokio::test]
async fn a_scanned_profile_adds_a_contact_or_updates_one_only_when_asked() {
    let wallet = Wallet::create(&dir("profile-scan"), [1; 32], Network::Stagenet).unwrap();
    let other = Keys::generate(Network::Stagenet).keys;
    let code = uri::profile_uri(&other.address(4), "Henry", Some(&webp(200)));
    let scanned = wallet.said_scanned(&code).await.unwrap();
    assert!(scanned.clash.is_none());
    let henry = wallet.add_contact_from_profile(&code, false, false).await.unwrap();
    assert_eq!((henry.name.as_str(), henry.send_address.clone(), henry.photo.clone()), ("Henry", Some(other.address(4)), Some(webp(200))));
    // Scanned again from a new share: the same name, a new address. Nothing changes until Update.
    let newer = uri::profile_uri(&other.address(9), "Henry", None);
    let clash = wallet.said_scanned(&newer).await.unwrap().clash.unwrap();
    assert_eq!((clash.contact.id, clash.changes.len()), (henry.id, 1));
    assert_eq!(wallet.contact(henry.id).await.unwrap().send_address, Some(other.address(4)));
    let updated = wallet.update_contact_from_profile(henry.id, &newer, false).await.unwrap();
    assert_eq!((updated.send_address, updated.photo), (Some(other.address(9)), Some(webp(200))));
    // Rust asks too, whatever the screen did: the same name only once asked; a contact's address never.
    let elsewhere = uri::profile_uri(&other.address(11), "Henry", None);
    assert!(wallet.add_contact_from_profile(&elsewhere, false, false).await.is_err());
    let lookalike = uri::profile_uri(&other.address(12), "Hen\u{200b}ry", None);
    assert!(wallet.add_contact_from_profile(&lookalike, false, false).await.is_err());
    let refused = wallet.add_contact_from_profile(&newer, true, false).await.unwrap_err().to_string();
    assert_eq!(refused, "That address is Henry's already: update Henry instead.");
    let renamed = uri::profile_uri(&other.address(9), "Someone else", None);
    assert!(wallet.add_contact_from_profile(&renamed, true, false).await.is_err(), "an address is never two contacts' send address");
    assert_eq!(wallet.said_contacts("", Clock { now: now(), utc_offset: 0, hours_24: true }).await.len(), 1);
    // Add as new contact, once asked about the name, keeps both.
    wallet.add_contact_from_profile(&elsewhere, true, false).await.unwrap();
    assert_eq!(wallet.said_contacts("henry", Clock { now: now(), utc_offset: 0, hours_24: true }).await.len(), 2);
    assert!(wallet.said_scanned(&format!("monero:{}", other.address(4))).await.is_err());
    assert!(wallet.update_contact_from_profile(99, &newer, false).await.is_err());
}
