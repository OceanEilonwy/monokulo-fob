use super::*;

#[tokio::test]
async fn a_view_only_wallet_refuses_to_prepare_payments() {
    let full = Keys::generate(Network::Stagenet).keys;
    let watch =
        Wallet::restore_view_only(&dir("watch"), [1; 32], Network::Stagenet, &full.primary_address(), &full.view_key_hex(), Some(5)).unwrap();
    assert!(!watch.can_spend());
    // Even holding a ready coin, a view-only wallet can't split.
    watch.inner.state.lock().await.outputs = vec![Owned {
        output: String::new(),
        tx: "in".into(),
        height: 0,
        time: 0,
        amount: 10,
        subaddress: 0,
        coinbase: false,
        unlock_block: 0,
        key: "in".into(),
        coin: Coin::default(),
        tx_key: None,
        extra_key: None,
    }];
    watch.inner.state.lock().await.tip = 100;
    let coins = watch.said_coins(Clock { now: 6_000, utc_offset: 0, hours_24: true }).await;
    assert!(coins.ready > 0 && !coins.can_split);
    assert!(matches!(watch.prepare(&full.primary_address(), 1, None, None).await, Err(FobError::Vault(VaultError::ViewOnly))));
}

/// Confirm opens on a preview before the fee (round 9 › 3b): checked as
/// `prepare` checks, with no node to ask (this wallet has none).
#[tokio::test]
async fn a_payment_previews_without_a_node_and_refuses_what_prepare_refuses() {
    let wallet = Wallet::create(&dir("preview"), [1; 32], Network::Stagenet).unwrap();
    let to = Keys::generate(Network::Stagenet).keys.primary_address();
    let code = format!("monero:{to}?tx_amount=0.5&tx_description=Flat%20white");
    let p = wallet.preview_typed(&format!(" {to} "), "0.5", false, Some(&code)).await.unwrap();
    assert_eq!(p, Preview { to: to.clone(), amount: 500_000_000_000, receipt: None });
    assert!(matches!(wallet.preview_typed("86Ux", "0.5", false, None).await, Err(FobError::Monero(MoneroError::BadAddress(_)))));
    assert!(matches!(wallet.preview(&to, 0, None), Err(FobError::Monero(MoneroError::BadAmount))));
    let mainnet = Keys::generate(Network::Mainnet).keys.primary_address();
    assert!(wallet.preview(&mainnet, 1, None).is_err());
}

#[tokio::test]
async fn a_payment_built_during_the_warm_up_waits_for_it_and_the_node_is_asked_once() {
    let (url, asked) = counting_node(Duration::from_millis(300)).await;
    let wallet = wallet_for_sending("warm-overlap", &url).await;
    let ((), b, c) = tokio::join!(
        wallet.warm_up_send(),
        ready(&wallet),
        async {
            // Arrives while the first is still waiting for the tip.
            tokio::time::sleep(Duration::from_millis(100)).await;
            ready(&wallet).await
        }
    );
    let (b, c) = (b.unwrap(), c.unwrap());
    assert_eq!(b.1, 2_999_999);
    assert_eq!((b.1, c.1), (c.1, b.1), "the same tip");
    assert_eq!(asked.height.load(Ordering::Relaxed), 1, "the tip asked once");
    assert_eq!(asked.fee.load(Ordering::Relaxed), 1, "the fee asked once");
    ready(&wallet).await.unwrap();
    wallet.warm_up_send().await;
    assert_eq!((asked.height.load(Ordering::Relaxed), asked.fee.load(Ordering::Relaxed)), (1, 1), "kept, so not asked again");
}

#[tokio::test]
async fn send_s_sheet_says_what_s_ready_and_who_was_paid() {
    let wallet = Wallet::create(&dir("send-sheet"), [1; 32], Network::Stagenet).unwrap();
    let t = now();
    let clock = Clock { now: t, utc_offset: 0, hours_24: true };
    assert_eq!(wallet.ready_line(t).await, "0 XMR ready to spend");
    assert!(wallet.said_lists(clock).await.paid_before.is_empty());
    let to = "5".repeat(95);
    {
        let mut state = wallet.inner.state.lock().await;
        state.tip = 100;
        state.outputs.push(Owned { height: 20, ..received("in", t, 2 * XMR) });
        // Not ready: not counted.
        state.outputs.push(Owned { height: 95, key: "late".into(), ..received("late", t, XMR) });
        state.sent.push(Sent { not_confirmed: false, released: false, tx: "out".into(), to: to.clone(), amount: XMR / 10, fee: 1, change: 0, time: t, height: None, receipt: None, note: Some("Lunch money".into()), pieces: None, back: vec![], recipient_name: None, signed: None });
    }
    assert_eq!(wallet.ready_line(t).await, "2 XMR ready to spend");
    wallet.refresh_prices_from(&FakePrices::new(166_100_000, vec![]), Asked::Background, t).await.unwrap();
    assert_eq!(wallet.ready_line(t).await, "2 XMR ready to spend · $332.20");
    // A price that's missed refreshes says how old it is; one over a
    // day old isn't used.
    assert_eq!(wallet.ready_line(t + 2 * 3_600).await, "2 XMR ready to spend · $332.20 · 2 h old");
    assert_eq!(wallet.worth_aged(XMR, t + price::AGED).await, Some(("$166.10".into(), Some(fiat::PriceAge::Minutes(10)))));
    assert_eq!(wallet.worth_aged(XMR, t + 60).await, Some(("$166.10".into(), None)));
    assert_eq!(wallet.ready_line(t + price::SHOWN).await, "2 XMR ready to spend");
    let paid = wallet.said_lists(clock).await.paid_before;
    assert_eq!(paid.len(), 1);
    assert_eq!((paid[0].address.as_str(), paid[0].name.as_str(), paid[0].line.as_str()), (to.as_str(), "5555 … 5555", "Lunch money · Today"));
}

#[tokio::test]
async fn split_up_learns_from_payments_and_pays_its_own_address() {
    let wallet = Wallet::create(&dir("split"), [1; 32], Network::Stagenet).unwrap();
    let t = now();
    let sent = |tx: &str, amount: u64, time: u64, pieces: Option<u32>| Sent {
        not_confirmed: false,
        released: false,
        tx: tx.into(),
        to: "86Ux".into(),
        amount,
        fee: 1,
        change: 0,
        time,
        height: Some(20),
        receipt: None,
        note: None,
        pieces,
        back: vec![],
        recipient_name: None,
        signed: None,
    };
    {
        let mut state = wallet.inner.state.lock().await;
        state.tip = 100;
        state.outputs.push(received("in", t, 2 * XMR));
        // Not ready yet: not the split's to use.
        state.outputs.push(Owned { height: 95, key: "late".into(), ..received("late", t, 5 * XMR) });
        for i in 0..4 {
            state.sent.push(sent(&format!("coffee-{i}"), XMR / 10, t - 86_400 * i, None));
        }
        // Older than 30 days, and a split: neither teaches it.
        state.sent.push(sent("old", XMR, t - 40 * 86_400, None));
        state.sent.push(sent("split", XMR, t, Some(12)));
    }
    // What Auto learns from, read straight from the wallet, is what
    // Activity shows going out in those 30 days, splits aside.
    {
        let mut state = wallet.inner.state.lock().await;
        let spent = Coin::Spent { ki: KeyImage::Known([0xaa; 32]), tx: tx_hash("restored-spend"), height: 99 };
        state.outputs.push(Owned { key: "spent".into(), ..received("old-in", t - 40 * 86_400, XMR) }.with_coin(spent));
        state.outputs.push(Owned { key: "change".into(), ..received(&tx_hex("restored-spend"), t, XMR / 2) });
        state.spends.push(ChainSpend { tx: tx_hex("restored-spend"), height: 99, time: t, fee: XMR / 100 });
    }
    let since = clock_now().now - split::LOOKBACK;
    let mut learned = Wallet::split_payments(&*wallet.inner.state.lock().await, clock_now().now);
    let mut shown: Vec<u64> = wallet
        .activity()
        .await
        .into_iter()
        .filter(|a| a.direction == Direction::Out && a.split.is_none() && a.time >= since && a.amount > 0)
        .map(|a| a.amount)
        .collect();
    learned.sort();
    shown.sort();
    assert_eq!(learned, shown);
    assert_eq!(learned, [XMR / 10, XMR / 10, XMR / 10, XMR / 10, XMR / 2 - XMR / 100]);
    {
        let mut state = wallet.inner.state.lock().await;
        state.outputs.retain(|o| o.tx != "old-in" && o.tx != tx_hex("restored-spend"));
        state.spends.clear();
    }
    // No price: notes in XMR. Split up opens empty, with Auto learned
    // from the four coffees.
    let opening = wallet.said_split_view(&[], clock_now()).await;
    assert_eq!((opening.pieces, opening.is_auto, opening.button.as_str(), opening.footer.as_str()), (0, false, "Add a coin to split", ""));
    let auto = opening.auto.expect("four payments to learn from").counts;
    assert_eq!(auto, [0, 0, 15, 0, 0, 0]);
    let view = wallet.said_split_view(&auto, clock_now()).await;
    assert!(view.is_auto);
    assert_eq!((view.summary.as_deref(), view.button.as_str()), (Some("15 coins · 1.5 XMR"), "Split into 15 coins"));
    assert!(view.footer.starts_with("Network fee about 0.0001"), "{}", view.footer);
    let refused = wallet.split_add(&auto, 0).await.unwrap_err();
    assert_eq!(refused.to_string(), "15 coins is the most one split can make");
    // Only the 2 XMR ready counts: a 1 XMR piece fits once, not twice.
    let one = wallet.split_add(&[], 5).await.unwrap();
    assert!(matches!(wallet.split_add(&one, 5).await, Err(FobError::Screen(_))));
    assert_eq!(wallet.split_remove(&one, 5).await, [0; 6]);

    // The split's address: the next unused, kept, and never given out on Receive.
    assert_eq!(wallet.receive_index().await, 0);
    let split_address = wallet.split_address().await.unwrap();
    assert_eq!(split_address, wallet.inner.keys.address(1));
    assert_eq!(wallet.split_address().await.unwrap(), split_address);
    assert_eq!(wallet.receive_address().await.unwrap(), wallet.inner.keys.address(2));
    assert_eq!(wallet.new_receive_address().await.unwrap(), wallet.inner.keys.address(3));
    assert_eq!(Wallet::open(&wallet.inner.path, [1; 32]).unwrap().split_address().await.unwrap(), split_address);

    // Sent and not yet mined: all of it comes back, and reads as a split.
    {
        let mut state = wallet.inner.state.lock().await;
        state.sent.retain(|s| s.tx != "split");
        state.sent.push(Sent { change: 2 * XMR - 1, height: None, ..sent("split", 15 * XMR / 10, t, Some(15)) });
    }
    assert_eq!(wallet.balance().await.total, 2 * XMR + 5 * XMR + 2 * XMR - 1);
    let clock = Clock { now: t, utc_offset: 0, hours_24: true };
    let pieces = wallet.said_not_ready(clock).await.unwrap().pieces;
    assert!(pieces.iter().any(|p| p.source.starts_with("Split up · ")));

    // Sent with its outputs known: each one shows before it's mined,
    // the same count as after, not one lump.
    {
        let mut state = wallet.inner.state.lock().await;
        state.sent.retain(|s| s.tx != "split");
        let back = vec![XMR / 10; 15].into_iter().chain([XMR / 2 - 1]).collect::<Vec<_>>();
        state.sent.push(Sent { change: 2 * XMR - 1, height: None, back, ..sent("split", 15 * XMR / 10, t, Some(15)) });
    }
    let summary = wallet.said_not_ready(clock).await.unwrap();
    let split: Vec<_> = summary.pieces.iter().filter(|p| p.source.starts_with("Split up · ")).collect();
    assert_eq!(split.len(), 16);
    assert_eq!(split.iter().map(|p| p.atomic).sum::<u64>(), 2 * XMR - 1);
    let row = wallet.said_lists(clock).await.days.into_iter().flat_map(|d| d.rows).find(|r| r.activity.as_ref().is_some_and(|a| a.tx == "split")).unwrap();
    assert_eq!((row.title.as_str(), row.amount.as_str()), ("Split up", "−0.000000000001"));
    assert!(row.line.starts_with("Sending · 15 coins · "), "{}", row.line);
}

#[tokio::test]
async fn a_payment_published_is_recorded_even_if_its_caller_goes_away() {
    let path = dir("send-dropped");
    let wallet = Wallet::create(&path, [1; 32], Network::Stagenet).unwrap();
    wallet.inner.state.lock().await.outputs.push(received("coin", 1, XMR));
    let to = wallet.inner.keys.address(9);
    // The node takes it only after Confirm has gone (Back pressed while "Sending…", say).
    let (published, node) = tokio::sync::oneshot::channel::<()>();
    let publish = async move {
        node.await.ok();
        Ok(())
    };
    let call = wallet.publish_and_record(tx_hash("hash"), 30, outgoing(&to, &["coin"]), "signed".into(), publish);
    assert!(tokio::time::timeout(Duration::from_millis(20), call).await.is_err(), "still publishing when dropped");
    published.send(()).unwrap();
    for _ in 0..100 {
        if !wallet.inner.state.lock().await.sent.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let reopened = Wallet::open(&path, [1; 32]).unwrap();
    let state = reopened.inner.state.lock().await;
    let sent = state.sent.iter().find(|s| s.tx == tx_hex("hash")).expect("recorded, and saved");
    assert_eq!((sent.note.as_deref(), sent.recipient_name.as_deref(), sent.to.as_str()), (Some("Lunch"), Some("Sam"), to.as_str()));
    assert_eq!(sent.signed, None, "the node answered: nothing to offer again");
    assert_eq!(state.outputs[0].coin, Coin::Held { ki: KeyImage::Unknown, hold: Hold::OurPayment { tx: tx_hash("hash") } });
    drop(state);
    // Its inputs held, another payment from them is refused before anything is published.
    let never = async { panic!("not published") };
    assert!(matches!(wallet.publish_and_record(tx_hash("again"), 30, outgoing(&to, &["coin"]), String::new(), never).await, Err(FobError::Screen(ScreenError::NoSuchPayment))));
    // A node refusing it records nothing.
    wallet.inner.state.lock().await.outputs.push(received("coin-2", 1, XMR));
    let refused = async { Err(Unpublished::Refused(FobError::Node("rejected".into()))) };
    assert!(wallet.publish_and_record(tx_hash("refused"), 30, outgoing(&to, &["coin-2"]), String::new(), refused).await.is_err());
    assert!(wallet.inner.state.lock().await.sent.iter().all(|s| s.tx != tx_hex("refused")));
}

/// A node that gave no answer may have taken the payment (audit #24): it's
/// recorded as sent, its coins spent so a second try can't pick others and
/// pay twice, and the signed transaction kept, saved, to offer again.
#[tokio::test]
async fn a_payment_the_node_gave_no_answer_for_is_kept_as_sent() {
    let path = dir("send-unanswered");
    let wallet = Wallet::create(&path, [1; 32], Network::Stagenet).unwrap();
    wallet.inner.state.lock().await.outputs.push(received("coin", 1, XMR));
    let to = wallet.inner.keys.address(9);
    let timed_out = async { Err(Unpublished::Unsure) };
    assert_eq!(wallet.publish_and_record(tx_hash("hash"), 30, outgoing(&to, &["coin"]), "0200".into(), timed_out).await.unwrap(), tx_hex("hash"));
    let reopened = Wallet::open(&path, [1; 32]).unwrap();
    let state = reopened.inner.state.lock().await;
    let sent = state.sent.iter().find(|s| s.tx == tx_hex("hash")).expect("recorded, and saved");
    assert_eq!(sent.signed.as_deref(), Some("0200"));
    assert!(state.outputs[0].spent_by(&tx_hex("hash")), "held by it, as reopened");
    drop(state);
    // Released a day on, nothing is offered again.
    {
        let mut state = wallet.inner.state.lock().await;
        state.sent[0].not_confirmed = true;
    }
    wallet.release_payment(&tx_hex("hash")).await.unwrap();
    assert_eq!(wallet.inner.state.lock().await.sent[0].signed, None);
}

/// A node still syncing answers BUSY, which reaches the wallet as a rejection
/// with no reason: the payment is offered again, not taken as turned down
/// (review of audit #24).
#[test]
fn a_busy_node_hasnt_turned_a_payment_down() {
    assert!(turned_down("double spend"));
    assert!(turned_down("fee too low"));
    assert!(!turned_down(""));
    assert!(!turned_down("  "));
    assert!(!turned_down("BUSY"));
    assert!(!turned_down("daemon is busy, try again later"));
}

/// Two payments sharing a coin, sent at once, can't both pass the check that
/// its coins are unspent before either is recorded (review of audit #24).
#[tokio::test]
async fn two_payments_sent_at_once_never_spend_one_coin_twice() {
    let wallet = Wallet::create(&dir("send-together"), [1; 32], Network::Stagenet).unwrap();
    wallet.inner.state.lock().await.outputs.push(received("coin", 1, XMR));
    let to = wallet.inner.keys.address(9);
    let (answer, node) = tokio::sync::oneshot::channel::<()>();
    let first = {
        let (wallet, to) = (wallet.clone(), to.clone());
        tokio::spawn(async move {
            let unsure = async move {
                node.await.ok();
                Err(Unpublished::Unsure)
            };
            wallet.publish_and_record(tx_hash("first"), 30, outgoing(&to, &["coin"]), "0200".into(), unsure).await
        })
    };
    tokio::time::sleep(Duration::from_millis(20)).await;
    let second = {
        let wallet = wallet.clone();
        tokio::spawn(async move {
            let never = async { panic!("not published: its coin is the first's") };
            wallet.publish_and_record(tx_hash("second"), 30, outgoing(&to, &["coin"]), "0300".into(), never).await
        })
    };
    tokio::time::sleep(Duration::from_millis(20)).await;
    answer.send(()).unwrap();
    assert_eq!(first.await.unwrap().unwrap(), tx_hex("first"));
    assert!(matches!(second.await.unwrap(), Err(FobError::Screen(ScreenError::NoSuchPayment))));
    let state = wallet.inner.state.lock().await;
    assert_eq!(state.sent.iter().map(|s| s.tx.as_str()).collect::<Vec<_>>(), [tx_hex("first")]);
}

/// A payment names its coins, not their places in the list, which a rescan
/// moves (audit #25): one prepared before a rescan spends the coins it
/// chose, and one whose coin the rescan hasn't found again isn't sent.
#[tokio::test]
async fn a_rescan_between_prepare_and_send_never_spends_the_wrong_coin() {
    let wallet = Wallet::create(&dir("send-rescan"), [1; 32], Network::Stagenet).unwrap();
    let to = wallet.inner.keys.address(9);
    {
        let mut state = wallet.inner.state.lock().await;
        state.outputs.push(received("first", 1, XMR));
        state.outputs.push(received("chosen", 1, 2 * XMR));
    }
    let prepared = outgoing(&to, &["chosen"]);
    // Rescanned: found again the other way round, with another coin first.
    {
        let mut state = wallet.inner.state.lock().await;
        state.outputs.clear();
        state.outputs.push(received("newer", 1, 3 * XMR));
        state.outputs.push(received("chosen", 1, 2 * XMR));
        state.outputs.push(received("first", 1, XMR));
    }
    wallet.publish_and_record(tx_hash("hash"), 30, prepared, String::new(), async { Ok(()) }).await.unwrap();
    let spent: Vec<(String, Option<TxHash>)> = wallet.inner.state.lock().await.outputs.iter().map(|o| (o.tx.clone(), o.coin.tx())).collect();
    let spent: Vec<(&str, Option<TxHash>)> = spent.iter().map(|(tx, s)| (tx.as_str(), *s)).collect();
    assert_eq!(spent, [("newer", None), ("chosen", Some(tx_hash("hash"))), ("first", None)]);
    // A rescan still under way, its coin not found again: not sent.
    wallet.inner.state.lock().await.outputs.retain(|o| o.tx == "first");
    let never = async { panic!("not published") };
    assert!(matches!(wallet.publish_and_record(tx_hash("gone"), 30, outgoing(&to, &["chosen"]), String::new(), never).await, Err(FobError::Screen(ScreenError::NoSuchPayment))));
}

#[tokio::test(start_paused = true)]
async fn a_prepared_payment_left_behind_is_forgotten() {
    let mut pending = Pending::default();
    pending.insert(1, "left behind");
    tokio::time::advance(PREPARED_FOR - Duration::from_secs(60)).await;
    pending.insert(2, "fresh");
    assert_eq!(pending.take(2), Some("fresh"));
    assert_eq!(pending.take(2), None, "sent or cancelled once");
    pending.insert(3, "newer");
    tokio::time::advance(Duration::from_secs(61)).await;
    assert_eq!(pending.take(1), None, "too old to send");
    assert!(pending.items.len() == 1 && pending.items.contains_key(&3), "pruned as others come and go");
    assert_eq!(pending.take(3), Some("newer"));
}
