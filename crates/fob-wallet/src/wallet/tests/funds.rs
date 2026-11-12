use super::*;

#[tokio::test]
async fn coins_are_found_ready_or_piece_by_piece_not_yet() {
    let wallet = Wallet::create(&dir("not-ready"), [1; 32], Network::Stagenet).unwrap();
    let owned = |tx: &str, height: u64, amount: u64, coinbase: bool| Owned {
        output: String::new(),
        tx: tx.into(),
        height,
        time: 1_000 + height,
        amount,
        subaddress: 0,
        coinbase,
        unlock_block: if coinbase { height + 60 } else { 0 },
        key: tx.into(),
        coin: Coin::default(),
        tx_key: None,
        extra_key: None,
    };
    let sent = |tx: &str, height: Option<u64>, change: u64| Sent {
        not_confirmed: false,
        released: false,
        tx: tx.into(),
        to: "86Ux3m9Pqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq7kQa".into(),
        amount: 1,
        fee: 1,
        change,
        time: 5_000,
        height,
        receipt: None,
        note: None,
        pieces: None,
        back: vec![],
        recipient_name: None,
        signed: None,
    };
    {
        let mut state = wallet.inner.state.lock().await;
        state.tip = 100;
        state.last_block_time = 4_000;
        state.outputs = vec![
            owned("in", 97, 10, false),
            owned("old", 50, 20, false),
            owned("mined", 80, 30, true),
            owned("change", 99, 40, false),
            owned("restored", 98, 50, false),
            Owned { unlock_block: 200, ..owned("locked", 40, 80, false) },
            owned("split", 60, 25, false),
        ];
        state.sent = vec![
            sent("change", Some(99), 40),
            sent("pending", None, 60),
            sent("nothing-back", None, 0),
            Sent { pieces: Some(1), ..sent("split", Some(60), 25) },
        ];
        state.spends = vec![ChainSpend { tx: "restored".into(), height: 98, time: 1_098, fee: 1 }];
    }
    wallet.pool_found([("pool".into(), None, 70)], 6_000);

    let (ready, pieces, last) = wallet.coin_pieces().await;
    let shown: Vec<_> = pieces.iter().map(|p| (p.amount, p.confirmations, p.needed, p.source.clone())).collect();
    let to = Some("86Ux … 7kQa".to_owned());
    assert_eq!(
        shown,
        [
            (10, 3, 10, Source::Received),
            (30, 20, 60, Source::Received),
            (40, 1, 10, Source::Change(to.clone())),
            (50, 2, 10, Source::Change(None)),
            (80, 60, 160, Source::Received),
            (60, 0, 10, Source::Change(to)),
            (70, 0, 10, Source::Received),
        ]
    );
    assert_eq!(last, 4_000);
    // Activity holds each payment in to the same rule: "Not ready"
    // until its outputs can be spent, not at 10 for every one.
    let lines: HashMap<String, String> = wallet.said_lists(Clock { now: 6_000, utc_offset: 0, hours_24: true }).await.days
        .into_iter()
        .flat_map(|d| d.rows)
        .map(|r| (r.activity.unwrap().tx, r.line.split(" · ").take(2).collect::<Vec<_>>().join(" · ")))
        .collect();
    assert_eq!(lines["in"], "Not ready · 3 of 10");
    assert_eq!(lines["mined"], "Not ready · 20 of 60");
    assert_eq!(lines["locked"], "Not ready · 60 of 160");
    assert!(!lines["old"].starts_with("Not ready"), "{}", lines["old"]);
    // Ready: what the balance counts as unlocked, a split's coin named as one.
    let shown: Vec<_> = ready.iter().map(|p| (p.amount, p.source.clone())).collect();
    assert_eq!(shown, [(20, Source::Received), (25, Source::Split)]);
    // The same money the balance counts as not unlocked.
    let balance = wallet.balance().await;
    assert_eq!(balance.unlocked, ready.iter().map(|p| p.amount).sum::<u64>());
    assert_eq!(balance.total - balance.unlocked, pieces.iter().map(|p| p.amount).sum::<u64>());
    // Coins ready to spend: Split up can be pressed.
    assert!(wallet.said_coins(Clock { now: 6_000, utc_offset: 0, hours_24: true }).await.can_split);
    // The Coins screen counts them all.
    let coins = wallet.said_coins(Clock { now: 6_000, utc_offset: 0, hours_24: true }).await;
    assert_eq!((coins.ready_coins.len(), coins.not_ready_coins.len()), (2, 7));
    assert_eq!((coins.ready, coins.not_ready), (balance.unlocked, balance.total - balance.unlocked));
    assert!(coins.intro.contains("is held as 9 coins"), "{}", coins.intro);
    // Home's sums: ready, not ready, and all of it, the balance.
    let summary = wallet.said_not_ready(Clock { now: 6_000, utc_offset: 0, hours_24: true }).await.unwrap();
    let sums: Vec<_> = summary.sums.iter().map(|s| (s.label.as_str(), s.atomic)).collect();
    assert_eq!(sums, [("Ready to spend", balance.unlocked), ("Not ready · 7 coins", balance.total - balance.unlocked), ("All of it", balance.total)]);
}

#[tokio::test]
async fn an_empty_wallet_has_nothing_to_show() {
    let wallet = Wallet::create(&dir("empty"), [1; 32], Network::Stagenet).unwrap();
    assert_eq!(wallet.balance().await, Balance::default());
    assert!(wallet.activity().await.is_empty());
}

#[tokio::test]
async fn notes_go_with_payments_in_and_out() {
    let wallet = Wallet::create(&dir("notes"), [1; 32], Network::Stagenet).unwrap();
    let t = now();
    wallet.remember_receive_note(1, " Coffee beans ").await.unwrap();
    {
        let mut state = wallet.inner.state.lock().await;
        state.outputs.push(received("in", t, XMR / 2));
        state.outputs.push(Owned { key: "in-2".into(), output: "2".into(), ..received("in", t, XMR / 4) });
        state.sent.push(Sent { not_confirmed: false, released: false, tx: "out".into(), to: "86Ux".into(), amount: 1, fee: 1, change: 0, time: t, height: None, receipt: None, note: Some("Rent".into()), pieces: None, back: vec![], recipient_name: None, signed: None });
    }
    let clock = Clock { now: t, utc_offset: 0, hours_24: true };
    let shown = wallet.said_payment("in", clock).await.unwrap();
    assert_eq!((shown.party.as_deref(), shown.pieces.as_deref()), (Some("Address 1 · Coffee beans"), Some("2 · 0.5 + 0.25")));
    assert!(shown.when.ends_with(" · Coffee beans"));
    assert!(wallet.said_payment("out", clock).await.unwrap().when.ends_with(" · Rent"));
    assert!(wallet.said_payment("nothing", clock).await.is_none());
    // A blank note forgets it.
    wallet.remember_receive_note(1, "").await.unwrap();
    assert_eq!(wallet.said_payment("in", clock).await.unwrap().party.as_deref(), Some("Address 1"));
}

/// Money locked to a time is kept, not dropped (audit #27), and ready once
/// the chain's blocks, not the phone's clock, are that late.
#[tokio::test]
async fn money_locked_to_a_time_is_ready_once_the_chain_is_that_late() {
    let wallet = Wallet::create(&dir("time-lock"), [1; 32], Network::Stagenet).unwrap();
    let until = 1_800_000_000;
    {
        let mut state = wallet.inner.state.lock().await;
        state.tip = 100;
        state.last_block_time = until - 3_600;
        state.outputs.push(Owned { unlock_block: until, ..received("later", 1, XMR) });
    }
    assert_eq!(wallet.balance().await, Balance { total: XMR, unlocked: 0, unsynced: 0 });
    {
        let state = wallet.inner.state.lock().await;
        // An hour of two-minute blocks past the 90 it has.
        assert_eq!(state.needed(&state.outputs[0]), 90 + 30);
    }
    wallet.inner.state.lock().await.last_block_time = until;
    assert_eq!(wallet.balance().await, Balance { total: XMR, unlocked: XMR, unsynced: 0 });
    // A block lock is still a block lock.
    wallet.inner.state.lock().await.outputs[0].unlock_block = 200;
    assert_eq!(wallet.balance().await.unlocked, 0);
}

/// Paid and the payment's screen say when the node gave no answer (audit
/// #81): only while Fob still offers the payment again, not once it's mined.
#[tokio::test]
async fn a_payment_the_node_gave_no_answer_for_says_so_until_it_is_mined() {
    let wallet = Wallet::create(&dir("funds-unanswered"), [1; 32], Network::Stagenet).unwrap();
    {
        let mut state = wallet.inner.state.lock().await;
        state.outputs.push(received("coin", 1, XMR));
        state.outputs.push(received("coin-2", 1, XMR));
    }
    let to = wallet.inner.keys.address(9);
    let (unsure, taken) = (tx_hex("unsure"), tx_hex("taken"));
    wallet.publish_and_record(tx_hash("unsure"), 30, outgoing(&to, &["coin"]), "0200".into(), async { Err(Unpublished::Unsure) }).await.unwrap();
    wallet.publish_and_record(tx_hash("taken"), 30, outgoing(&to, &["coin-2"]), "0300".into(), async { Ok(()) }).await.unwrap();
    assert!(wallet.unanswered(&unsure).await);
    assert!(!wallet.unanswered(&taken).await, "the node answered");
    assert!(!wallet.unanswered("no such payment").await);
    wallet.inner.state.lock().await.sent.iter_mut().find(|s| s.tx == unsure).unwrap().height = Some(10);
    assert!(!wallet.unanswered(&unsure).await, "mined: nothing left to offer");
}

/// Amounts come from the node, which could make some up near the most a
/// u64 holds: the balance stops there instead of wrapping to a small one.
#[tokio::test]
async fn amounts_a_node_made_up_add_up_to_no_more_than_a_u64_holds() {
    let wallet = Wallet::create(&dir("huge-amounts"), [1; 32], Network::Stagenet).unwrap();
    {
        let mut state = wallet.inner.state.lock().await;
        state.outputs.push(received("huge", 1, u64::MAX / 2 + 1));
        state.outputs.push(Owned { tx: "huge".into(), key: "huger".into(), ..received("huge", 1, u64::MAX / 2 + 1) });
    }
    assert_eq!(wallet.balance().await.total, u64::MAX);
    assert_eq!(wallet.activity().await[0].amount, u64::MAX, "one payment of both");
}
