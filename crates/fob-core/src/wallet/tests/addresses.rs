use super::*;

#[tokio::test]
async fn a_restored_wallet_hands_out_past_the_other_phone_s_addresses_and_watches_far_enough() {
    let words = Wallet::create(&dir("restore-from"), [1; 32], Network::Stagenet).unwrap().seed_words().unwrap().1;
    let restored = Wallet::restore_words(&dir("restore-to"), [1; 32], Network::Stagenet, &words, Some(0)).unwrap();
    // The other phone was paid at its request on 60, found so far; it may have handed out more, unpaid.
    restored.inner.state.lock().await.outputs.push(Owned { subaddress: 60, ..received("old", 1, XMR) });
    restored.inner.state.lock().await.receive_index = 60;
    let (first, _) = restored.make_request(None, None, false, Some("Rent")).await.unwrap();
    assert_eq!(first.subaddress, 60 + RESTORE_SKIP + 1);
    let (share_free, _) = restored.make_request(None, None, false, Some("Bills")).await.unwrap();
    assert_eq!(share_free.subaddress, first.subaddress + 1);
    // Scanning on finds another of the other phone's, paid at 80: the next new one goes past it too.
    restored.inner.state.lock().await.outputs.push(Owned { subaddress: 80, ..received("old-80", 1, XMR) });
    let (after, _) = restored.make_request(None, None, false, Some("Gas")).await.unwrap();
    assert_eq!(after.subaddress, 80 + RESTORE_SKIP + 1);
    // A payment to one of the other phone's old request addresses (say 70) is no new request's.
    restored.inner.state.lock().await.outputs.push(Owned { subaddress: 70, ..received("to-old-request", 2, XMR) });
    let row = restored.activity().await.into_iter().find(|r| r.tx == "to-old-request").unwrap();
    assert_eq!((row.note, row.contact), (None, None));
    // Paid at what this wallet handed out: no further jump.
    restored.inner.state.lock().await.outputs.push(Owned { subaddress: after.subaddress, ..received("mine", 3, XMR) });
    assert_eq!(restored.make_request(None, None, false, Some("Water")).await.unwrap().0.subaddress, after.subaddress + 1);
    // Nothing found: new ones start past the skip anyway.
    let empty = Wallet::restore_words(&dir("restore-empty"), [1; 32], Network::Stagenet, &words, Some(0)).unwrap();
    assert_eq!(empty.make_request(None, None, false, None).await.unwrap().0.subaddress, RESTORE_SKIP + 1);
    // A wallet made here skips nothing.
    let made = Wallet::create(&dir("restore-none"), [1; 32], Network::Stagenet).unwrap();
    assert_eq!(made.make_request(None, None, false, None).await.unwrap().0.subaddress, 1);
    // Scanning watches 200 past the highest handed out or paid: 150 unpaid requests or shares in a row past the
    // last paid one are still found by a restore, and a payment found moves the edge on.
    assert_eq!(Wallet::watched(60), 260);
    assert!(Wallet::watched(0) >= 150);
    // A file from before restores were marked reads as a wallet made here.
    let mut file = serde_json::to_value(FileOut { keys: StoredKeys::from(&made.inner.keys), state: &*made.inner.state.lock().await }).unwrap();
    file["state"].as_object_mut().unwrap().remove("restored");
    assert!(!serde_json::from_value::<FileIn>(file).unwrap().state.restored);
}

/// Reading Receive's address before Receive opens (the screens' refresh,
/// as a restore starts and before any block is scanned) hands nothing
/// out and saves nothing: the scan still finds the other phone's
/// payments past the skip as its own, and the address Receive then
/// hands out goes past them.
#[tokio::test]
async fn reading_a_restored_wallet_s_receive_address_hands_nothing_out() {
    let words = Wallet::create(&dir("restore-read-from"), [1; 32], Network::Stagenet).unwrap().seed_words().unwrap().1;
    let path = dir("restore-read-to");
    let restored = Wallet::restore_words(&path, [1; 32], Network::Stagenet, &words, Some(0)).unwrap();
    let marks = |s: &State| (s.receive_index, s.receive_own, s.fresh_from);
    let before = marks(&*restored.inner.state.lock().await);
    let path = restored.inner.path.clone();
    let file = std::fs::read(&path).unwrap();
    assert_eq!(restored.shown_receive_address().await, None, "Receive hands one out as it opens");
    assert_eq!(marks(&*restored.inner.state.lock().await), before);
    assert_eq!(std::fs::read(&path).unwrap(), file, "nothing saved");
    // The scan finds the other phone's payment at 60 (past the skip of 50): still not one to show.
    {
        let mut state = restored.inner.state.lock().await;
        state.outputs.push(Owned { subaddress: 60, ..received("theirs", 1, XMR) });
        state.receive_index = 60;
    }
    assert_eq!(restored.shown_receive_address().await, None, "60 is the other phone's");
    // Receive opens: past what they handed out after it.
    let own = 60 + RESTORE_SKIP + 1;
    assert_eq!(restored.receive_address().await.unwrap(), restored.inner.keys.address(own));
    assert_eq!(restored.shown_receive_address().await, Some((restored.inner.keys.address(own), own)));
    assert_eq!(restored.make_request(None, None, false, None).await.unwrap().0.subaddress, own + 1);
    // A wallet made here reads none until Receive first opens, then that one.
    let made = Wallet::create(&dir("restore-read-made"), [1; 32], Network::Stagenet).unwrap();
    assert_eq!(made.shown_receive_address().await, None);
    let first = made.receive_address().await.unwrap();
    assert_eq!(made.shown_receive_address().await, Some((first, 1)));
}

#[tokio::test]
async fn a_new_wallet_asks_to_be_backed_up_until_its_words_are_written_down() {
    let path = dir("backup");
    let wallet = Wallet::create(&path, [1; 32], Network::Stagenet).unwrap();
    assert_eq!(wallet.backup_reminder().await, Some(keys::reminder(16)));
    // Fob closed during the backup step: opened again, it still asks.
    let reopened = Wallet::open(&path, [1; 32]).unwrap();
    assert!(reopened.backup_reminder().await.is_some());
    reopened.backup_done().await.unwrap();
    reopened.backup_done().await.unwrap();
    assert!(Wallet::open(&path, [1; 32]).unwrap().backup_reminder().await.is_none());
    // Restored ones, and files from before the reminder, never ask.
    let words = wallet.seed_words().unwrap().1;
    assert!(Wallet::restore_words(&dir("backup-restored"), [1; 32], Network::Stagenet, &words, Some(0)).unwrap().backup_reminder().await.is_none());
    let mut file = serde_json::to_value(FileOut { keys: StoredKeys::from(&wallet.inner.keys), state: &*wallet.inner.state.lock().await }).unwrap();
    file["state"].as_object_mut().unwrap().remove("backup_pending");
    assert!(!serde_json::from_value::<FileIn>(file).unwrap().state.backup_pending);
}

#[tokio::test]
async fn backup_and_keys_says_whether_the_words_are_written_down_and_shows_the_restore_height() {
    let path = dir("hub");
    let wallet = Wallet::create(&path, [1; 32], Network::Stagenet).unwrap();
    assert_eq!(wallet.backup_hub(true).await.backup[0].line, "16 words that restore everything · not written down yet");
    wallet.backup_done().await.unwrap();
    let hub = Wallet::open(&path, [1; 32]).unwrap().backup_hub(true).await;
    assert_eq!(hub.backup[0].line, "16 words that restore everything · backed up");
    assert_eq!(hub.settings_line, "Seed phrase, wallet code, keys");
    // Restored from 25 words: no word on whether they're written down.
    let classic = keys::Keys::from_spend_key(Network::Stagenet, &wallet.inner.keys.spend_key_hex().unwrap(), None).unwrap();
    let words = classic.seed_words().unwrap().1;
    let restored = Wallet::restore_words(&dir("hub-restored"), [1; 32], Network::Stagenet, &words, Some(2_200_000)).unwrap();
    assert_eq!(restored.backup_hub(true).await.backup[0].line, "25 words that restore everything");
    let page = restored.keys_page().await;
    let height = &page[1].keys[1];
    assert_eq!(height.title, "Restore height");
    assert_eq!(*height.value, format!("2200000 · {}", restore_date(Network::Stagenet, 2_200_000).unwrap()));
    assert_eq!(page[0].keys[0].value, wallet.inner.keys.spend_key_hex().unwrap());
    assert_eq!(*page[1].keys[0].value, wallet.primary_address());
    // A view-only wallet: Wallet code and Keys, and Keys without the
    // secret spend key.
    let watch = Wallet::restore_view_only(&dir("hub-watch"), [1; 32], Network::Stagenet, &wallet.primary_address(), &wallet.inner.keys.view_key_hex(), Some(0)).unwrap();
    let hub = watch.backup_hub(true).await;
    assert_eq!(hub.backup.iter().map(|r| r.item).collect::<Vec<_>>(), [keys::HubItem::WalletCode]);
    assert_eq!(hub.settings_line, "Wallet code, keys");
    let page = watch.keys_page().await;
    assert_eq!(page[0].keys.iter().map(|k| k.title.as_str()).collect::<Vec<_>>(), ["Secret view key"]);
    assert_eq!(*page[1].keys[1].value, "0");
}

#[test]
fn the_restore_height_is_dated_only_when_the_date_can_be_named() {
    let near = Network::Mainnet.height_at(1_788_307_200); // 2 Sep 2026, 00:00 UTC
    assert_eq!(restore_date(Network::Mainnet, near).as_deref(), Some("2 Sep 2026"));
    assert_eq!(restore_date(Network::Mainnet, 1_009_827), None);
    assert_eq!(restore_date(Network::Stagenet, 0), None);
}
