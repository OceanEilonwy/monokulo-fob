//! Making, opening and saving a wallet, and taking it off the device.

use super::*;

impl Wallet {
    pub(super) fn new(path: &Path, file_key: [u8; 32], keys: Keys, restore_height: Option<u64>) -> Result<Wallet> {
        let state = State {
            node: keys.network.default_nodes()[0].to_owned(),
            node_self_signed: false,
            node_pin: None,
            custom_nodes: vec![],
            gpu_checked_on: None,
            gpu_check_passed: false,
            manual_sync: false,
            pull_mode: PullMode::default(),
            incoming_scan_blocks: default_incoming_scan_blocks(),
            settled_confirmations: default_settled_confirmations(),
            pull_mark_height: None,
            pull_mark_hash: None,
            zmq_endpoint: None,
            zmq_on: false,
            sync_mode: SyncMode::Node,
            light_url: None,
            light_self_signed: false,
            light_pin: None,
            light_registered: false,
            light_subaddrs_sent: 0,
            light_scanned_height: 0,
            light_blocks: vec![],
            light_history: vec![],
            restore_height,
            next_block: restore_height.unwrap_or(0),
            tip: 0,
            last_block_time: 0,
            recent: vec![],
            receive_index: 0,
            receive_own: None,
            outputs: vec![],
            transitions: std::collections::VecDeque::new(),
            spends: vec![],
            sent: vec![],
            prices: price::Settings::default(),
            price_cache: price::Cache::default(),
            price_records: HashMap::new(),
            notes: HashMap::new(),
            split_index: None,
            contacts: contacts::Book::default(),
            requests: requests::Book::default(),
            profile: profile::Book::default(),
            restored: restore_height.is_some(),
            fresh_from: None,
            backup_pending: restore_height.is_none(),
        };
        let path = path.with_extension("db");
        let db = Db::create(&path, &file_key, &StoredKeys::from(&keys))?;
        let wallet = Wallet::from_parts(&path, db, keys, state);
        wallet.save_state(&wallet.inner.state.try_lock().expect("a new wallet is not shared yet"))?;
        Ok(wallet)
    }

    pub(super) fn from_parts(path: &Path, db: Db, keys: Keys, state: State) -> Wallet {
        let light = state.sync_mode == SyncMode::Light;
        let zmq = if state.zmq_on && !light { ZmqStatus::Connecting } else { ZmqStatus::Off };
        let status = SyncStatus {
            scanned: state.next_block,
            tip: state.tip,
            start: state.restore_height.unwrap_or(state.next_block),
            up_to: state.last_block_time,
            manual_sync: state.manual_scanning(),
            light: light.then(|| catch_up::Light { host: node::address(state.light_url.as_deref().unwrap_or("")), ..Default::default() }),
            synced: state.manual_scanning(),
            zmq_endpoint: state.zmq_endpoint.clone(),
            zmq,
            ..SyncStatus::default()
        };
        Wallet {
            inner: Arc::new(Inner {
                path: path.to_owned(),
                keys,
                state: Mutex::new(state),
                rpc: Mutex::new(None),
                link: std::sync::Mutex::new(None),
                pipeline: Mutex::new(None),
                scanner: std::sync::Mutex::new(None),
                gpu_checked: AtomicBool::new(false),
                quiet_checked: std::sync::Mutex::new(None),
                scan_asked: AtomicBool::new(false),
                scan_stop: AtomicBool::new(false),
                scan_job: std::sync::Mutex::new(None),
                scan_running: Mutex::new(()),
                sending: Mutex::new(()),
                unsaved: AtomicBool::new(false),
                saved_at: std::sync::Mutex::new(Instant::now()),
                db: std::sync::Mutex::new(Some(db)),
                prepared: Mutex::new(Pending::default()),
                next_id: AtomicU64::new(1),
                status: std::sync::Mutex::new(status),
                syncing: std::sync::Mutex::new(None),
                wake: tokio::sync::Notify::new(),
                pool_wake: tokio::sync::Notify::new(),
                listener: std::sync::Mutex::new(None),
                zmq_task: std::sync::Mutex::new(None),
                rounds: tokio::sync::watch::Sender::new(0),
                pool: std::sync::Mutex::new(HashMap::new()),
                pool_seen: std::sync::Mutex::new(HashSet::new()),
                last_block: std::sync::Mutex::new(None),
                prices_task: std::sync::Mutex::new(None),
                prices_wake: tokio::sync::Notify::new(),
                prices_asked: std::sync::Mutex::new(None),
                typed_asked: std::sync::Mutex::new(0),
                price_lookups: AtomicU64::new(0),
                price_looked_up: AtomicU64::new(0),
                price_alerts: std::sync::Mutex::new(crate::price_alert::Alerts::default()),
                fee_rate: std::sync::Mutex::new(None),
                send_context: SendContext::new(),
                fresh_share: std::sync::Mutex::new(None),
                node_verdict: std::sync::Mutex::new(None),
                light: std::sync::Mutex::new(None),
                live_swap: std::sync::Mutex::new(std::sync::Weak::new()),
            }),
        }
    }

    /// A brand-new wallet with a fresh Polyseed, saved at `path`.
    pub fn create(path: &Path, file_key: [u8; 32], network: Network) -> Result<Wallet> {
        Wallet::new(path, file_key, Keys::generate(network).keys, None)
    }

    /// A wallet from seed words. A Polyseed knows its own start; for 25 words
    /// `restore_height` says where to start (block 0 if `None`).
    pub fn restore_words(path: &Path, file_key: [u8; 32], network: Network, words: &str, restore_height: Option<u64>) -> Result<Wallet> {
        let restored = Keys::from_words(network, words)?;
        let height = restore_height.or(restored.birthday.map(|b| network.height_at(b))).unwrap_or(0);
        Wallet::new(path, file_key, restored.keys, Some(height))
    }

    /// A wallet from a scanned wallet code. `restore_height` is used when the
    /// code doesn't say.
    pub fn restore_code(path: &Path, file_key: [u8; 32], network: Network, code: &str, restore_height: Option<u64>) -> Result<Wallet> {
        match uri::parse_wallet_code(network, code)? {
            WalletCode::Words { words, height } => Wallet::restore_words(path, file_key, network, &words, height.or(restore_height)),
            WalletCode::SpendKey { address, spend, view, height } => {
                let keys = Keys::from_spend_key(network, &spend, view.as_deref().map(|v| v.as_str()))?;
                if address.is_some_and(|a| a != keys.primary_address()) {
                    return Err(FobError::Vault(VaultError::BadKey));
                }
                Wallet::new(path, file_key, keys, Some(height.or(restore_height).unwrap_or(0)))
            }
            WalletCode::ViewOnly { address, view, height } => {
                Wallet::restore_view_only(path, file_key, network, &address, &view, height.or(restore_height))
            }
        }
    }

    pub fn restore_view_only(
        path: &Path,
        file_key: [u8; 32],
        network: Network,
        address: &str,
        view_hex: &str,
        restore_height: Option<u64>,
    ) -> Result<Wallet> {
        Wallet::new(path, file_key, Keys::view_only(network, address, view_hex)?, Some(restore_height.unwrap_or(0)))
    }

    /// A wallet whose keys are on the Trezor reached over `link`: the
    /// device exports the view key (asking first) and keeps the spend key.
    /// `restore_height` says where to start (block 0 if `None`): a wallet
    /// brought over may have been paid at any time, and only a wallet made
    /// here can be known to have nothing before it.
    pub fn restore_trezor(
        path: &Path,
        file_key: [u8; 32],
        network: Network,
        link: &dyn trezor::Link,
        prompts: &dyn trezor::Prompts,
        restore_height: Option<u64>,
    ) -> Result<Wallet> {
        let session = trezor::Session::open(link, prompts)?;
        let (view, address) = session.watch_key(network)?;
        // The cable could have changed the address on the way; the device
        // shows it on its own screen, for the person to compare with the
        // one Fob received, shown in full beside it, and says it again:
        // both have to agree.
        prompts.address(&address);
        if session.address_shown(network)? != address {
            return Err(FobError::Trezor("the address the Trezor shows isn't the one it sent: don't use this cable or computer".into()));
        }
        let keys = Keys::trezor(network, &address, &view)?;
        let wallet = Wallet::new(path, file_key, keys, Some(restore_height.unwrap_or(0)))?;
        {
            // Nothing to write down here: the device has its own backup.
            let mut state = wallet.inner.state.try_lock().expect("a new wallet is not shared yet");
            state.backup_pending = false;
            state.restored = true;
            wallet.save_state(&state)?;
        }
        Ok(wallet)
    }

    /// The wallet saved at `path`, its database, made first from the sealed
    /// file beside it if it's still in that. The sealed file is deleted once the
    /// database has opened whole. A photo saved before Fob read
    /// photos' headers that it wouldn't take now is dropped (the contact or
    /// profile stays): it could claim a picture too big to draw.
    pub fn open(path: &Path, file_key: [u8; 32]) -> Result<Wallet> {
        let path = path.with_extension("db");
        if !path.exists() {
            Self::migrate(&path, &file_key)?;
        }
        let mut db = Db::open(&path, &file_key)?;
        let keys = db.keys()?;
        let saved = db.load()?;
        let mut state: State = serde_json::from_value(saved).map_err(|e| FobError::storage(e.to_string()))?;
        state.coins_from_marks();
        db.loaded(Self::state_value(&state)?)?;
        let dropped = state.contacts.drop_bad_photos() | state.profile.drop_bad_photo();
        let wallet = Wallet::from_parts(&path, db, keys.into_keys()?, state);
        if dropped {
            wallet.save_state(&wallet.inner.state.try_lock().expect("an opened wallet is not shared yet"))?;
        }
        // Best effort: one that can't go now goes on a later open.
        let _ = Self::delete_sealed_file(&path);
        Ok(wallet)
    }

    /// Deletes the sealed file beside the database at `path`, once the database
    /// and its name are certain to be on the disk: a power cut can't lose both.
    pub(super) fn delete_sealed_file(path: &Path) -> Result<()> {
        if !path.with_extension("fob").exists() && !path.with_extension("tmp").exists() {
            return Ok(());
        }
        let sync = |to_sync: &Path| std::fs::File::open(to_sync).and_then(|file| file.sync_all()).map_err(|e| FobError::storage(e.to_string()));
        sync(path)?;
        sync(path.parent().ok_or_else(|| FobError::storage("a wallet's database has no folder"))?)?;
        crate::wallets::remove_sealed_file(path).map_err(Into::into)
    }

    /// Makes the database at `path` from the sealed file beside it. It takes
    /// its name only once what it reads back is the same as the file, so a
    /// failure or a crash leaves the file to try again from.
    pub(super) fn migrate(path: &Path, file_key: &[u8; 32]) -> Result<()> {
        let sealed = path.with_extension("fob");
        let file: FileIn = store::load(&sealed, file_key)?;
        let making = path.with_extension("db.migrating");
        let keys = Zeroizing::new(file.keys);
        let mut db = Db::create(&making, file_key, &keys)?;
        let expected = Self::state_value(&file.state)?;
        db.save(expected.clone())?;
        let back: State = serde_json::from_value(db.load()?).map_err(|e| FobError::storage(e.to_string()))?;
        if Self::state_value(&back)? != expected {
            return Err(FobError::storage("the wallet read back from its new database isn't the same"));
        }
        if !db.keys()?.same_as(&keys) {
            return Err(FobError::storage("the keys read back from the new database aren't the same"));
        }
        db.close()?;
        crate::db::rename(&making, path).map_err(Into::into)
    }

    pub(super) fn state_value(state: &State) -> Result<serde_json::Value> {
        serde_json::to_value(state).map_err(|e| FobError::storage(e.to_string()))
    }

    pub(super) fn save_state(&self, state: &State) -> Result<()> {
        let value = Self::state_value(state)?;
        let mut db = self.inner.db.lock().expect("db lock");
        let Some(db) = db.as_mut() else { return Ok(()) };
        db.save(value)?;
        self.inner.unsaved.store(false, Ordering::Relaxed);
        *self.inner.saved_at.lock().expect("save lock") = Instant::now();
        Ok(())
    }

    /// Saves scanning that hasn't been yet. Best effort: the chain has it.
    pub(super) async fn flush(&self) {
        if self.inner.unsaved.load(Ordering::Relaxed) {
            let state = self.inner.state.lock().await;
            let _ = self.save_state(&state);
        }
    }

    /// Takes the wallet's database, and any sealed file it came from, off the
    /// phone for good: a save still on its way doesn't bring them back. Its seed
    /// restores it.
    pub fn remove_file(&self) -> Result<()> {
        self.stop_sync();
        let mut db = self.inner.db.lock().expect("db lock");
        *db = None;
        let _ = std::fs::remove_file(self.inner.path.with_extension("db.migrating"));
        crate::wallets::remove_files(&self.inner.path).map_err(Into::into)
    }

    pub fn network(&self) -> Network {
        self.inner.keys.network
    }
}
