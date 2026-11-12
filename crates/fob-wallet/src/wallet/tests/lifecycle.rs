use super::*;

#[test]
fn restore_points_are_heights_or_dates() {
    assert_eq!(restore_point(Network::Stagenet, " 123456 ").unwrap(), 123_456);
    let march = restore_point(Network::Mainnet, "2026-03-12").unwrap();
    let april = restore_point(Network::Mainnet, "2026-04-12").unwrap();
    assert_eq!(april - march, 31 * 720);
    assert!(restore_point(Network::Mainnet, "12/03/2026").is_err());
    assert!(restore_point(Network::Mainnet, "2026-13-01").is_err());
}

#[tokio::test]
async fn a_wallet_opens_again_with_the_same_addresses() {
    let path = dir("reopen");
    let wallet = Wallet::create(&path, [1; 32], Network::Stagenet).unwrap();
    let first = wallet.receive_address().await.unwrap();
    let second = wallet.new_receive_address().await.unwrap();
    assert_ne!(first, second);
    let again = Wallet::open(&path, [1; 32]).unwrap();
    assert_eq!(again.primary_address(), wallet.primary_address());
    assert_eq!(again.receive_address().await.unwrap(), second);
    assert!(matches!(Wallet::open(&path, [2; 32]), Err(FobError::Vault(VaultError::WrongKey))));
}

#[tokio::test]
async fn a_wallet_code_restores_the_wallet_it_came_from() {
    let wallet = Wallet::create(&dir("code-a"), [1; 32], Network::Stagenet).unwrap();
    let code = wallet.wallet_code().await;
    let preview = preview_wallet_code(Network::Stagenet, &code).unwrap();
    assert_eq!((preview.address.as_str(), preview.can_spend), (wallet.primary_address().as_str(), true));
    let restored = Wallet::restore_code(&dir("code-b"), [1; 32], Network::Stagenet, &code, None).unwrap();
    assert_eq!(restored.primary_address(), wallet.primary_address());
    assert!(restored.can_spend());
}

#[tokio::test]
async fn a_used_wallet_comes_back_from_its_database_whole() {
    let path = dir("db-whole");
    let wallet = a_used_wallet(&path).await;
    let again = Wallet::open(&path, [1; 32]).unwrap();
    assert_eq!(saved(&again).await, saved(&wallet).await);
    assert_eq!(again.seed_words().unwrap().1, wallet.seed_words().unwrap().1);
    assert_eq!(again.primary_address(), wallet.primary_address());
}

/// The migration on what matters: nothing of an old file lost, the same
/// wallet after it, the file deleted, and nothing done twice.
#[tokio::test]
async fn an_old_file_becomes_a_database_with_nothing_lost() {
    let path = dir("db-migrate");
    let wallet = a_used_wallet(&path).await;
    let state = saved(&wallet).await;
    let file = serde_json::to_value(FileOut { keys: StoredKeys::from(&wallet.inner.keys), state: &*wallet.inner.state.lock().await }).unwrap();
    old_file(&path, &file);
    let sealed = path.with_extension("fob");
    std::fs::write(path.with_extension("tmp"), b"left by a crash").unwrap();

    let migrated = Wallet::open(&sealed, [1; 32]).unwrap();
    assert_eq!(saved(&migrated).await, state);
    assert_eq!(migrated.seed_words().unwrap().1, wallet.seed_words().unwrap().1);
    assert_eq!(migrated.primary_address(), wallet.primary_address());
    assert!(path.with_extension("db").exists());
    assert!(!path.with_extension("db.migrating").exists());
    assert!(!sealed.exists(), "the old file goes once the database opens");
    assert!(!path.with_extension("tmp").exists(), "and any temporary one");

    // What's done after it stays: the database is the wallet from now on.
    migrated.remember_receive_note(1, "Newer").await.unwrap();
    let next = saved(&migrated).await;
    assert_eq!(saved(&Wallet::open(&sealed, [1; 32]).unwrap()).await, next, "not migrated again");
}

/// A wallet the build before this one migrated still has its old file.
/// Version 9 of the database kept a coin's state in two columns, the key
/// image and `spent_in`: nothing, a hash (spent on the chain, held by a
/// payment of ours, or by a pool transaction, told apart by the other
/// tables) or one of two words a device sync wrote. Version 10 reads each
/// into its Coin as the file opens, and a mark that is none of those holds
/// the coin rather than freeing it.
#[tokio::test]
async fn a_version_9_database_opens_with_each_coin_in_its_state() {
    let path = dir("v9-coins");
    let wallet = Wallet::create(&path, [1; 32], Network::Stagenet).unwrap();
    {
        let mut state = wallet.inner.state.lock().await;
        for name in ["free-unknown", "free-known", "spent", "ours", "pooled", "before-sync", "pool-at-sync", "junk"] {
            state.outputs.push(received(name, 1, XMR));
        }
        state.spends.push(ChainSpend { tx: tx_hex("spend"), height: 42, time: 1, fee: 1 });
        state.sent.push(Sent {
            not_confirmed: false,
            released: false,
            tx: tx_hex("ours"),
            to: "addr".into(),
            amount: XMR,
            fee: 30,
            change: 0,
            time: 1,
            height: None,
            receipt: None, receipt_view: None,
            note: None,
            pieces: None,
            back: vec![],
            recipient_name: None,
            signed: None,
        });
        wallet.save_state(&state).unwrap();
    }
    drop(wallet);
    // Back to version 9 by hand: the column gone, the two old ones filled.
    let connection = rusqlite::Connection::open(path.with_extension("db")).unwrap();
    connection.execute_batch(&format!("PRAGMA key = \"x'{}'\";", hex::encode([1u8; 32]))).unwrap();
    connection.execute_batch("ALTER TABLE sent DROP COLUMN receipt_view; ALTER TABLE outputs DROP COLUMN coin; PRAGMA user_version = 9;").unwrap();
    let image = hex::encode([0xaa; 32]);
    let marks: [(Option<&str>, Option<String>); 8] = [
        (None, None),
        (Some(&image), None),
        (Some(&image), Some(tx_hex("spend"))),
        (Some(&image), Some(tx_hex("ours"))),
        (Some(&image), Some(tx_hex("pooled"))),
        (Some(&image), Some("spent-before-sync".into())),
        (Some(&image), Some("spent-in-pool-at-sync".into())),
        (Some(&image), Some("not a hash".into())),
    ];
    for (pos, (key_image, spent_in)) in marks.iter().enumerate() {
        connection.execute("UPDATE outputs SET key_image = ?1, spent_in = ?2 WHERE pos = ?3", rusqlite::params![key_image, spent_in, pos as i64]).unwrap();
    }
    connection.close().unwrap();

    let reopened = Wallet::open(&path, [1; 32]).unwrap();
    let known = KeyImage::Known([0xaa; 32]);
    let held = |hold| Coin::Held { ki: known, hold };
    let coins: Vec<Coin> = reopened.inner.state.lock().await.outputs.iter().map(|o| o.coin).collect();
    assert_eq!(
        coins,
        [
            Coin::Free { ki: KeyImage::Unknown },
            Coin::Free { ki: known },
            Coin::Spent { ki: known, tx: tx_hash("spend"), height: 42 },
            held(Hold::OurPayment { tx: tx_hash("ours") }),
            held(Hold::InPool { tx: tx_hash("pooled") }),
            held(Hold::DeviceChain { asked: 0 }),
            held(Hold::DevicePool { asked: 0 }),
            held(Hold::DeviceChain { asked: 0 }),
        ]
    );
    assert_eq!(reopened.balance().await.total, 2 * XMR, "the two free coins");
    // Saved again and opened again, the same.
    reopened.save_state(&*reopened.inner.state.lock().await).unwrap();
    drop(reopened);
    let again = Wallet::open(&path, [1; 32]).unwrap();
    assert_eq!(again.inner.state.lock().await.outputs.iter().map(|o| o.coin).collect::<Vec<_>>(), coins);
}

/// The sealed file from before the database kept the same two pieces:
/// migrating one reads them into each coin's state the same way.
#[tokio::test]
async fn an_old_sealed_file_s_coins_are_read_into_their_states() {
    let path = dir("sealed-coins");
    let wallet = Wallet::create(&path, [1; 32], Network::Stagenet).unwrap();
    {
        let mut state = wallet.inner.state.lock().await;
        state.outputs.push(received("a", 1, XMR));
        state.outputs.push(received("b", 1, XMR));
        state.spends.push(ChainSpend { tx: tx_hex("spend"), height: 42, time: 1, fee: 1 });
    }
    let mut file = serde_json::to_value(FileOut { keys: StoredKeys::from(&wallet.inner.keys), state: &*wallet.inner.state.lock().await }).unwrap();
    for (output, mark) in file["state"]["outputs"].as_array_mut().unwrap().iter_mut().zip([Some(tx_hex("spend")), None]) {
        let fields = output.as_object_mut().unwrap();
        fields.remove("coin");
        fields.insert("key_image".into(), serde_json::json!(hex::encode([0xaa; 32])));
        fields.insert("spent_in".into(), serde_json::json!(mark));
    }
    drop(wallet);
    old_file(&path, &file);
    let opened = Wallet::open(&path, [1; 32]).unwrap();
    let state = opened.inner.state.lock().await;
    let known = KeyImage::Known([0xaa; 32]);
    assert_eq!(state.outputs[0].coin, Coin::Spent { ki: known, tx: tx_hash("spend"), height: 42 });
    assert_eq!(state.outputs[1].coin, Coin::Free { ki: known });
}

#[tokio::test]
async fn an_old_file_left_beside_a_database_goes_when_it_opens() {
    let path = dir("db-left-beside");
    let wallet = a_used_wallet(&path).await;
    let state = saved(&wallet).await;
    std::fs::write(path.with_extension("fob"), b"old").unwrap();
    assert_eq!(saved(&Wallet::open(&path, [1; 32]).unwrap()).await, state);
    assert!(!path.with_extension("fob").exists());
}

/// The old file stays until its database has opened: it's the wallet until then.
#[tokio::test]
async fn an_old_file_stays_while_its_database_will_not_open() {
    let path = dir("db-keeps-file");
    let wallet = a_used_wallet(&path).await;
    drop(wallet);
    std::fs::write(path.with_extension("fob"), b"old").unwrap();
    assert!(matches!(Wallet::open(&path, [2; 32]), Err(FobError::Vault(VaultError::WrongKey))));
    assert!(path.with_extension("fob").exists(), "wrong key");
    crate::db::remove_files(&path.with_extension("db")).unwrap();
    std::fs::write(path.with_extension("db"), b"not a database, and long enough to look at its header").unwrap();
    assert!(Wallet::open(&path, [1; 32]).is_err());
    assert!(path.with_extension("fob").exists(), "unreadable database");
}

/// A database from before the custom nodes table gets it, and loses nothing.
#[tokio::test]
async fn a_second_version_database_is_brought_up_to_date() {
    let path = dir("db-version-2");
    let wallet = a_used_wallet(&path).await;
    let state = saved(&wallet).await;
    drop(wallet);
    let old = rusqlite::Connection::open(path.with_extension("db")).unwrap();
    old.execute_batch(&format!(
        "PRAGMA key = \"x'{}'\"; ALTER TABLE wallet DROP COLUMN gpu_checked_on; ALTER TABLE wallet DROP COLUMN gpu_check_passed; ALTER TABLE wallet DROP COLUMN manual_sync; ALTER TABLE wallet DROP COLUMN pull_mode; ALTER TABLE wallet DROP COLUMN incoming_scan_blocks; ALTER TABLE wallet DROP COLUMN settled_confirmations; ALTER TABLE wallet DROP COLUMN pull_mark_height; ALTER TABLE wallet DROP COLUMN pull_mark_hash; ALTER TABLE sent DROP COLUMN not_confirmed; ALTER TABLE sent DROP COLUMN released; ALTER TABLE sent DROP COLUMN signed; ALTER TABLE wallet DROP COLUMN node_pin; ALTER TABLE keys DROP COLUMN device; ALTER TABLE outputs DROP COLUMN tx_key; ALTER TABLE outputs DROP COLUMN extra_key; ALTER TABLE wallet DROP COLUMN sync_mode; ALTER TABLE wallet DROP COLUMN light_url; ALTER TABLE wallet DROP COLUMN light_self_signed; ALTER TABLE wallet DROP COLUMN light_pin; ALTER TABLE wallet DROP COLUMN light_registered; ALTER TABLE wallet DROP COLUMN light_subaddrs_sent; ALTER TABLE wallet DROP COLUMN light_scanned_height; ALTER TABLE wallet DROP COLUMN light_blocks; ALTER TABLE wallet DROP COLUMN light_history; {BACK_TO_VERSION_9} PRAGMA user_version = 2;",
        hex::encode([1u8; 32])
    ))
    .unwrap();
    drop(old);
    let upgraded = Wallet::open(&path, [1; 32]).unwrap();
    assert_eq!(saved(&upgraded).await, state);
    assert!(!upgraded.gpu_check_failed().await);
    {
        let mut state = upgraded.inner.state.lock().await;
        state.gpu_checked_on = Some("A GPU 0x1".into());
        upgraded.save_state(&state).unwrap();
    }
    drop(upgraded);
    let reopened = Wallet::open(&path, [1; 32]).unwrap();
    assert_eq!(reopened.inner.state.lock().await.gpu_checked_on.as_deref(), Some("A GPU 0x1"));
    assert!(reopened.gpu_check_failed().await, "checked, and not passed");
}

#[tokio::test]
async fn a_first_version_database_is_brought_up_to_date() {
    let path = dir("db-version-1");
    let wallet = a_used_wallet(&path).await;
    wallet.remove_node("https://mine.example:18081").await.unwrap();
    let state = saved(&wallet).await;
    drop(wallet);
    let old = rusqlite::Connection::open(path.with_extension("db")).unwrap();
    old.execute_batch(&format!(
        "PRAGMA key = \"x'{}'\"; DROP TABLE custom_nodes; ALTER TABLE wallet DROP COLUMN gpu_checked_on; ALTER TABLE wallet DROP COLUMN gpu_check_passed; ALTER TABLE wallet DROP COLUMN manual_sync; ALTER TABLE wallet DROP COLUMN pull_mode; ALTER TABLE wallet DROP COLUMN incoming_scan_blocks; ALTER TABLE wallet DROP COLUMN settled_confirmations; ALTER TABLE wallet DROP COLUMN pull_mark_height; ALTER TABLE wallet DROP COLUMN pull_mark_hash; ALTER TABLE sent DROP COLUMN not_confirmed; ALTER TABLE sent DROP COLUMN released; ALTER TABLE sent DROP COLUMN signed; ALTER TABLE wallet DROP COLUMN node_pin; ALTER TABLE keys DROP COLUMN device; ALTER TABLE outputs DROP COLUMN tx_key; ALTER TABLE outputs DROP COLUMN extra_key; ALTER TABLE wallet DROP COLUMN sync_mode; ALTER TABLE wallet DROP COLUMN light_url; ALTER TABLE wallet DROP COLUMN light_self_signed; ALTER TABLE wallet DROP COLUMN light_pin; ALTER TABLE wallet DROP COLUMN light_registered; ALTER TABLE wallet DROP COLUMN light_subaddrs_sent; ALTER TABLE wallet DROP COLUMN light_scanned_height; ALTER TABLE wallet DROP COLUMN light_blocks; ALTER TABLE wallet DROP COLUMN light_history; {BACK_TO_VERSION_9} PRAGMA user_version = 1;",
        hex::encode([1u8; 32])
    ))
    .unwrap();
    drop(old);
    let upgraded = Wallet::open(&path, [1; 32]).unwrap();
    assert_eq!(saved(&upgraded).await, state);
    upgraded.add_node("node.example:18081", false).await.unwrap();
    drop(upgraded);
    assert_eq!(Wallet::open(&path, [1; 32]).unwrap().custom_nodes().await.len(), 1);
}

#[tokio::test]
async fn a_migration_that_stopped_part_way_starts_again_from_the_file() {
    let path = dir("db-interrupted");
    let wallet = a_used_wallet(&path).await;
    let state = saved(&wallet).await;
    let file = serde_json::to_value(FileOut { keys: StoredKeys::from(&wallet.inner.keys), state: &*wallet.inner.state.lock().await }).unwrap();
    old_file(&path, &file);
    std::fs::write(path.with_extension("db.migrating"), b"half written").unwrap();
    assert_eq!(saved(&Wallet::open(&path, [1; 32]).unwrap()).await, state);
}

#[tokio::test]
async fn the_wrong_key_opens_neither_a_file_nor_a_database() {
    let path = dir("db-key");
    let wallet = a_used_wallet(&path).await;
    let file = serde_json::to_value(FileOut { keys: StoredKeys::from(&wallet.inner.keys), state: &*wallet.inner.state.lock().await }).unwrap();
    assert!(matches!(Wallet::open(&path, [2; 32]), Err(FobError::Vault(VaultError::WrongKey))));
    old_file(&path, &file);
    assert!(matches!(Wallet::open(&path, [2; 32]), Err(FobError::Vault(VaultError::WrongKey))));
    assert!(!path.with_extension("db").exists(), "no database from a failed migration");
    assert!(Wallet::open(&path, [1; 32]).is_ok(), "and the file is still good");
}

/// What's on disk gives nothing away: not the seed, a name, or that it's SQLite.
#[tokio::test]
async fn nothing_in_the_database_can_be_read_without_the_key() {
    let path = dir("db-sealed");
    let wallet = a_used_wallet(&path).await;
    let words = wallet.seed_words().unwrap().1;
    let first = words.split_whitespace().next().unwrap().to_owned();
    for file in std::fs::read_dir(path.parent().unwrap()).unwrap() {
        let bytes = std::fs::read(file.unwrap().path()).unwrap();
        for secret in [first.as_bytes(), b"Neighbour", b"SQLite format 3", b"Tickets"] {
            assert!(!bytes.windows(secret.len()).any(|w| w == secret), "{:?} in the clear", String::from_utf8_lossy(secret));
        }
    }
}

#[tokio::test]
async fn a_removed_wallet_stays_removed_and_takes_its_old_file() {
    let path = dir("db-remove");
    let wallet = a_used_wallet(&path).await;
    let state = wallet.inner.state.lock().await.clone();
    std::fs::write(path.with_extension("fob"), b"old").unwrap();
    wallet.remove_file().unwrap();
    wallet.save_state(&state).unwrap();
    for name in ["db", "fob", "db-wal", "db-shm"] {
        assert!(!path.with_extension(name).exists(), "{name}");
    }
}

/// A save writes what changed, so a list that shrinks or a value that goes
/// comes back as it now is, not as it was.
#[tokio::test]
async fn saves_after_changes_and_removals_read_back_as_they_stand() {
    let path = dir("db-changes");
    let wallet = a_used_wallet(&path).await;
    {
        let mut state = wallet.inner.state.lock().await;
        state.outputs.remove(0);
        state.spends.clear();
        state.price_cache.current = None;
        state.split_index = None;
        state.notes.clear();
        state.price_records.clear();
        wallet.save_state(&state).unwrap();
    }
    wallet.delete_contact(1).await.ok();
    assert_eq!(saved(&Wallet::open(&path, [1; 32]).unwrap()).await, saved(&wallet).await);
}

#[tokio::test]
async fn a_wallet_file_from_before_requests_opens_and_keeps_receive_s_address() {
    let path = dir("requests-old-file");
    let wallet = Wallet::create(&path, [1; 32], Network::Stagenet).unwrap();
    wallet.new_receive_address().await.unwrap();
    let shown = wallet.new_receive_address().await.unwrap();
    let mut file = serde_json::to_value(FileOut { keys: StoredKeys::from(&wallet.inner.keys), state: &*wallet.inner.state.lock().await }).unwrap();
    let state = file["state"].as_object_mut().unwrap();
    assert!(state.remove("requests").is_some(), "requests are saved");
    assert!(state.remove("receive_own").is_some(), "Receive's address is saved");
    old_file(&path, &file);
    let old = Wallet::open(&path, [1; 32]).unwrap();
    // Before requests, Receive showed the last one handed out.
    let last = old.receive_address().await.unwrap();
    assert_eq!(last, shown);
    assert_eq!(last, old.inner.keys.address(2));
    let (request, _) = old.make_request(None, None, None).await.unwrap();
    assert_eq!(request.subaddress, 3);
    assert_eq!(old.receive_address().await.unwrap(), last);
}

#[tokio::test]
async fn a_wallet_file_from_before_contacts_opens_with_none() {
    let path = dir("contacts-old-file");
    let wallet = Wallet::create(&path, [1; 32], Network::Stagenet).unwrap();
    let mut file = serde_json::to_value(FileOut { keys: StoredKeys::from(&wallet.inner.keys), state: &*wallet.inner.state.lock().await }).unwrap();
    assert!(file["state"].as_object_mut().unwrap().remove("contacts").is_some(), "contacts are saved");
    old_file(&path, &file);
    let old = Wallet::open(&path, [1; 32]).unwrap();
    let clock = Clock { now: now(), utc_offset: 0, hours_24: true };
    assert!(old.said_contacts("", clock).await.is_empty());
    assert_eq!(old.add_contact(contacts::Draft { name: "Mum".into(), ..Default::default() }).await.unwrap().id, 1);
}

#[tokio::test]
async fn a_wallet_file_from_before_profiles_opens_with_none() {
    let path = dir("profile-old-file");
    let wallet = Wallet::create(&path, [1; 32], Network::Stagenet).unwrap();
    wallet.set_profile("Henry", None).await.unwrap();
    let mut file = serde_json::to_value(FileOut { keys: StoredKeys::from(&wallet.inner.keys), state: &*wallet.inner.state.lock().await }).unwrap();
    assert!(file["state"].as_object_mut().unwrap().remove("profile").is_some(), "the profile is saved");
    old_file(&path, &file);
    let old = Wallet::open(&path, [1; 32]).unwrap();
    assert!(old.profile().await.is_none());
    assert!(old.said_shares(Clock { now: now(), utc_offset: 0, hours_24: true }).await.is_empty());
}

#[tokio::test]
async fn a_photo_saved_before_headers_were_read_is_dropped_on_opening() {
    let path = dir("bad-photo");
    let wallet = Wallet::create(&path, [1; 32], Network::Stagenet).unwrap();
    let good = fob_codes::names::test_webp(300, 96);
    wallet.set_profile("Henry", Some(good.clone())).await.unwrap();
    let sam = wallet.add_contact(contacts::Draft { name: "Sam".into(), photo: Some(good.clone()), ..Default::default() }).await.unwrap();
    let jo = wallet.add_contact(contacts::Draft { name: "Jo".into(), photo: Some(good.clone()), ..Default::default() }).await.unwrap();
    // As an older Fob kept them: a header claiming 16384 × 16384, for Jo and the profile.
    let huge = fob_codes::names::test_webp(300, 16384);
    let mut file = serde_json::to_value(FileOut { keys: StoredKeys::from(&wallet.inner.keys), state: &*wallet.inner.state.lock().await }).unwrap();
    file["state"]["contacts"]["contacts"][1]["photo"] = hex::encode(&huge).into();
    file["state"]["profile"]["profile"]["photo"] = hex::encode(&huge).into();
    old_file(&path, &file);
    let opened = Wallet::open(&path, [1; 32]).unwrap();
    assert_eq!(opened.contact(sam.id).await.unwrap().photo, Some(good));
    let jo = opened.contact(jo.id).await.unwrap();
    assert_eq!((jo.name.as_str(), jo.photo), ("Jo", None));
    assert_eq!(opened.profile().await.map(|p| (p.name, p.photo)), Some(("Henry".into(), None)));
    // And saved so: opened again, it's still gone.
    assert_eq!(Wallet::open(&path, [1; 32]).unwrap().contact(jo.id).await.unwrap().photo, None);
}

/// A save written outside the state lock can finish after a newer one:
/// it's left out, not written over the newer state.
#[tokio::test(flavor = "multi_thread")]
async fn an_older_save_finishing_late_leaves_the_newer_one() {
    let path = dir("save-order");
    let wallet = Wallet::create(&path, [1; 32], Network::Stagenet).unwrap();
    let mut state = wallet.inner.state.lock().await;
    state.tip = 10;
    let older = wallet.snapshot(&state).unwrap();
    state.tip = 20;
    wallet.save_releasing(state).unwrap();
    wallet.write_state(older.0, older.1).unwrap();
    assert_eq!(Wallet::open(&path, [1; 32]).unwrap().inner.state.lock().await.tip, 20);
}
