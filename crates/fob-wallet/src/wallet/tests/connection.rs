use super::*;

#[tokio::test]
async fn zmq_is_off_until_turned_on_and_follows_the_node_s_host() {
    let wallet = Wallet::create(&dir("zmq-settings"), [1; 32], Network::Stagenet).unwrap();
    assert_eq!(wallet.status().zmq, ZmqStatus::Off);
    assert_eq!(wallet.status().zmq_text, "Off");
    // Offered, not used: the node's host, stagenet's usual port.
    assert_eq!(wallet.zmq_endpoint().await, "tcp://stagenet.xmr-tw.org:38083");
    assert!(matches!(wallet.set_zmq(true, Some("tcp://elsewhere.example:38083")).await, Err(FobError::BadEndpoint(_))));
    assert_eq!(wallet.status().zmq, ZmqStatus::Off, "refused, so still off");
    wallet.set_zmq(true, None).await.unwrap();
    assert_eq!(wallet.status().zmq_text, "stagenet.xmr-tw.org:38083 · connecting");
    let path = wallet.inner.path.clone();
    drop(wallet);
    let again = Wallet::open(&path, [1; 32]).unwrap();
    assert_eq!(again.status().zmq, ZmqStatus::Connecting, "saved with the wallet");
    // Another host (set without asking the node, as set_node would):
    // instant updates go off rather than follow somewhere unchosen.
    again.inner.state.lock().await.node = "http://127.0.0.1:1".into();
    assert!(matches!(again.set_zmq(true, Some("tcp://stagenet.xmr-tw.org:38083")).await, Err(FobError::BadEndpoint(_))));
}

#[tokio::test]
async fn custom_nodes_are_kept_per_wallet_until_removed() {
    let path = dir("nodes-kept");
    let wallet = Wallet::create(&path, [1; 32], Network::Stagenet).unwrap();
    wallet.add_node("node.example:38081/", true).await.unwrap();
    wallet.add_node("http://node.example:38081", true).await.unwrap();
    let kept = node::Setting { url: "http://node.example:38081".into(), self_signed: true };
    assert_eq!(wallet.custom_nodes().await, std::slice::from_ref(&kept), "each once, as normalised");

    // One of the network's own is on the list already; an onion can't be reached.
    wallet.add_node(Network::Stagenet.default_nodes()[1], false).await.unwrap();
    assert!(matches!(wallet.add_node("abc.onion:38081", false).await, Err(FobError::BadEndpoint(_))));
    assert_eq!(wallet.custom_nodes().await.len(), 1);

    // Across a reopen, offered after the network's own; another wallet doesn't have it.
    drop(wallet);
    let wallet = Wallet::open(&path, [1; 32]).unwrap();
    let others = wallet.other_nodes().await;
    assert_eq!(others.len(), Network::Stagenet.default_nodes().len());
    assert_eq!(others.last(), Some(&kept));
    let other_wallet = Wallet::create(&dir("nodes-other"), [1; 32], Network::Stagenet).unwrap();
    assert!(other_wallet.custom_nodes().await.is_empty());

    // A different certificate choice replaces the first.
    wallet.add_node("node.example:38081", false).await.unwrap();
    assert_eq!(wallet.custom_nodes().await, [node::Setting { self_signed: false, ..kept.clone() }]);

    wallet.remove_node("node.example:38081").await.unwrap();
    drop(wallet);
    assert!(Wallet::open(&path, [1; 32]).unwrap().custom_nodes().await.is_empty());
}

/// A node stays on the list when it's refused, and a custom node from before
/// the list is kept on changing away from it.
#[tokio::test]
async fn changing_node_keeps_the_one_left_and_the_one_refused() {
    let path = dir("nodes-change");
    let wallet = Wallet::create(&path, [1; 32], Network::Stagenet).unwrap();
    let before_the_list = node::Setting { url: "http://mine.example:38081".into(), self_signed: true };
    {
        let mut state = wallet.inner.state.lock().await;
        state.node = before_the_list.url.clone();
        state.node_self_signed = true;
    }
    assert!(wallet.set_node("http://127.0.0.1:9", false).await.is_err(), "nothing answers there");
    let refused = node::Setting { url: "http://127.0.0.1:9".into(), self_signed: false };
    assert_eq!(wallet.custom_nodes().await, [before_the_list.clone(), refused.clone()]);
    assert_eq!(wallet.node().await, before_the_list, "still on the one it had");
    drop(wallet);
    assert_eq!(Wallet::open(&path, [1; 32]).unwrap().custom_nodes().await, [before_the_list, refused]);
}

/// A node with a self-signed certificate is switched to only when
/// marked to accept it, and the mark is kept in the wallet file (off in
/// a file from before it), with the node's checks and Settings' line.
#[tokio::test]
async fn a_self_signed_node_is_kept_with_its_mark() {
    let path = dir("self-signed");
    let wallet = Wallet::create(&path, [1; 32], Network::Stagenet).unwrap();
    // From before the mark: opens, unmarked.
    let mut file = serde_json::to_value(FileOut { keys: StoredKeys::from(&wallet.inner.keys), state: &*wallet.inner.state.lock().await }).unwrap();
    assert!(file["state"].as_object_mut().unwrap().remove("node_self_signed").is_some(), "the mark is saved");
    old_file(&path, &file);
    let wallet = Wallet::open(&path, [1; 32]).unwrap();
    assert!(!wallet.node().await.self_signed);
    assert_eq!(wallet.node_line().await, "stagenet.xmr-tw.org:38081");

    let port = crate::node::canned::canned_node(true).await;
    let url = format!("https://127.0.0.1:{port}");
    let refused = wallet.set_node(&url, false).await.unwrap_err();
    assert_eq!(refused.to_string(), "the node didn't answer: self-signed, not accepted");
    wallet.set_node(&url, true).await.unwrap();
    let reopened = Wallet::open(&path, [1; 32]).unwrap();
    assert_eq!(reopened.node().await, node::Setting { url: url.clone(), self_signed: true });

    // Checking the node in use names it on Settings' line.
    let report = reopened.check_node(&reopened.node().await, &|_| {}).await;
    assert_eq!(report.rows[0].result, "self-signed, accepted");
    assert_eq!(reopened.node_line().await, format!("Doesn't send blocks · 127.0.0.1:{port}"));
    // Syncing failing since, the line says so, as Home's chip, over the
    // check (and over having synced before).
    reopened.set_status(|s| { s.synced = true; s.connected = true; });
    let offline = FobError::Node(format!("{:?}", InterfaceError::InterfaceError("dns error: failed to lookup address".into())));
    reopened.report(&Quiet, &mut catch_up::Pace::default(), Some(&offline)).await;
    assert_eq!(reopened.status().error.as_deref(), Some("Address not found"));
    assert_eq!(reopened.node_line().await, format!("Can't reach node · 127.0.0.1:{port}"));
    // A check after the failure is newer: it's what the line says.
    reopened.check_node(&reopened.node().await, &|_| {}).await;
    assert_eq!(reopened.node_line().await, format!("Doesn't send blocks · 127.0.0.1:{port}"));
    reopened.set_status(|s| s.error = None);
    assert_eq!(reopened.node_line().await, format!("Doesn't send blocks · 127.0.0.1:{port}"));
    // Failing to save the wallet isn't the node's doing.
    reopened.report(&Quiet, &mut catch_up::Pace::default(), Some(&FobError::Storage("disk full".into()))).await;
    assert!(reopened.status().error_local);
    assert_eq!(reopened.node_line().await, format!("Doesn't send blocks · 127.0.0.1:{port}"));
    *reopened.inner.node_verdict.lock().unwrap() = None;
    reopened.set_status(|s| { s.synced = true; s.connected = true; });
    assert_eq!(reopened.node_line().await, format!("Working · 127.0.0.1:{port}"));
    // The list leaves out the node in use, and a custom one comes once.
    reopened.add_node("node.example:38081/", true).await.unwrap();
    reopened.add_node("node.example:38081", true).await.unwrap();
    let others = reopened.other_nodes().await;
    assert_eq!(others.last(), Some(&node::Setting { url: "http://node.example:38081".into(), self_signed: true }));
    assert_eq!(others.iter().filter(|n| n.url == "http://node.example:38081").count(), 1);
    assert!(!others.iter().any(|n| n.url == format!("https://127.0.0.1:{port}")));
}

/// Switching to a working node while syncing fails stops the chip and
/// Node line saying the old node's failure at once, and the sync loop
/// tries the new node then, not at the end of its backoff.
#[tokio::test]
async fn switching_node_clears_the_old_failure_and_tries_the_new_one_at_once() {
    struct Heard(std::sync::Mutex<Vec<Option<String>>>);
    impl SyncListener for Heard {
        fn on_status(&self, status: SyncStatus) {
            self.0.lock().unwrap().push(status.error);
        }
    }
    let wallet = unreachable("switch-clears").await;
    let heard = Arc::new(Heard(std::sync::Mutex::new(vec![])));
    let mut rounds = wallet.inner.rounds.subscribe();
    wallet.start_sync(heard.clone());
    tokio::time::timeout(Duration::from_secs(5), rounds.changed()).await.unwrap().unwrap();
    assert_eq!(wallet.status().error.as_deref(), Some("Refused to connect"));
    assert_eq!(wallet.status().chip().text, "Can't reach node");
    let port = crate::node::canned::canned_node(false).await;
    rounds.mark_unchanged();
    let before = heard.0.lock().unwrap().len();
    wallet.set_node(&format!("127.0.0.1:{port}"), false).await.unwrap();
    // Told at once (the woken loop may already be reporting the new node).
    assert_eq!(heard.0.lock().unwrap().get(before), Some(&None));
    // The loop's 2 s backoff is cut short: a round against the new node now.
    tokio::time::timeout(Duration::from_millis(1500), rounds.changed()).await.expect("a round within the backoff").unwrap();
    wallet.stop_sync();
}

/// The connection kept is for the node and its mark: one made while the
/// mark was on is never used once it's off, whatever order a switch
/// and a connection being made happen in.
#[tokio::test]
async fn a_connection_is_kept_for_the_node_and_its_mark() {
    let wallet = Wallet::create(&dir("connection-mark"), [1; 32], Network::Stagenet).unwrap();
    let port = crate::node::canned::canned_node(true).await;
    {
        let mut state = wallet.inner.state.lock().await;
        state.node = format!("https://127.0.0.1:{port}");
        state.node_self_signed = true;
    }
    let (marked, ..) = wallet.connection().await.unwrap();
    assert!(marked.requests() > 0, "made and used");
    assert_eq!(wallet.connection().await.unwrap().0.requests(), marked.requests(), "kept while nothing changed");
    // The mark goes off without set_node dropping the connection (its
    // save landing between a connection's read of the setting and its
    // keeping one): a new connection, with the usual check.
    wallet.inner.state.lock().await.node_self_signed = false;
    let err = wallet.connection().await.err().expect("the usual check refuses it");
    assert!(format!("{err}").contains("certificate"), "{err}");
}

/// Fob has no Tor: an onion node is refused when chosen, and one
/// already in a wallet file is never looked up or connected to.
#[tokio::test]
async fn an_onion_node_is_refused_and_never_reached() {
    let wallet = Wallet::create(&dir("onion"), [1; 32], Network::Stagenet).unwrap();
    let before = wallet.node().await;
    let refused = wallet.set_node("xmrnode.onion:38081", false).await.unwrap_err();
    assert!(matches!(refused, FobError::BadEndpoint(_)));
    assert_eq!(refused.to_string(), "Fob can't reach .onion nodes yet");
    assert_eq!(wallet.node().await, before, "unchanged");
    // An older file with one: syncing fails on it, saying why.
    wallet.inner.state.lock().await.node = "http://xmrnode.onion:38081".into();
    let failed = wallet.sync_step().await.unwrap_err().to_string();
    assert!(failed.contains("Fob can't reach .onion nodes yet"), "{failed}");
    // And instant updates can't point at it either.
    assert!(matches!(wallet.set_zmq(true, Some("tcp://xmrnode.onion:38083")).await, Err(FobError::BadEndpoint(e)) if e == crate::node::NO_ONION));
}
