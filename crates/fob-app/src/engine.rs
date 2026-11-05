//! What's unlocked, which wallet is open, and what its screens show.
//!
//! Every call runs on the wallet's own runtime and returns what it came to;
//! what changes meanwhile (syncing, a price) is published to the [`Listener`].

use std::{
    future::Future,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use fob_ffi::{
    Contact, FobError, FobWallet, Network, PriceRetry, PriceToast, Prepared, ScanMode, ScannedProfile, Screens, SyncDetails, SyncObserver, SyncStatus, WalletEntry, WalletList, set_scan_mode, wallets_on,
};
use fob_wallet::passphrase;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::Clock;

/// Where the app stands, which decides the first screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Phase {
    /// First run: a passphrase has to be chosen.
    NoPassphrase,
    Locked,
    /// Unlocked, with no wallet on this network.
    NoWallet,
    /// A wallet is being opened.
    Opening,
    Open,
    /// The wallet couldn't be opened, and why.
    Failed(String),
}

/// What changed.
pub enum Update {
    Phase(Phase),
    Screens(Arc<Screens>),
    Status(Arc<SyncStatus>),
    /// Every wallet, and the one open.
    Wallets { all: Vec<WalletEntry>, current: Option<WalletEntry>, network: Network },
    /// A price failed, or stopped failing (`None`).
    Toast(Option<Arc<PriceToast>>),
}

/// Where what's published goes. Comes from any thread.
pub trait Listener: Send + Sync {
    fn on_update(&self, update: Update);
}

/// How a client differs.
pub struct Options {
    /// Where wallets are kept.
    pub dir: PathBuf,
    /// Where the GPU's compiled programs are kept.
    pub cache_dir: PathBuf,
    /// Follows the chain and asks for prices while a wallet is open. Off to
    /// look at a wallet without touching the network.
    pub online: bool,
}

#[derive(Serialize, Deserialize)]
struct Settings {
    #[serde(default)]
    mainnet: bool,
    #[serde(default)]
    scan_mode: Option<String>,
    /// Minutes untouched before locking; 0 for never.
    #[serde(default = "default_lock_after")]
    lock_after: u32,
}

fn default_lock_after() -> u32 {
    15
}

/// A profile code taken in Meet.
pub enum Met {
    /// Saved as this contact.
    Saved(Contact, ScannedProfile),
    /// It would change a contact someone should look at first.
    Ask(ScannedProfile),
}

impl Default for Settings {
    fn default() -> Self {
        Settings { mainnet: false, scan_mode: None, lock_after: default_lock_after() }
    }
}

fn scan_name(mode: &ScanMode) -> &'static str {
    match mode {
        ScanMode::Auto => "auto",
        ScanMode::Gpu => "gpu",
        ScanMode::Cpu => "cpu",
    }
}

fn scan_named(name: Option<&str>) -> ScanMode {
    match name {
        Some("gpu") => ScanMode::Gpu,
        Some("cpu") => ScanMode::Cpu,
        _ => ScanMode::Auto,
    }
}

struct Open {
    book: Arc<WalletList>,
    key: Zeroizing<[u8; 32]>,
    wallet: Option<Arc<FobWallet>>,
    current: Option<WalletEntry>,
}

struct Inner {
    options: Options,
    listener: Arc<dyn Listener>,
    clock: Box<dyn Fn() -> Clock + Send + Sync>,
    open: Mutex<Option<Open>>,
    settings: Mutex<Settings>,
    phase: Mutex<Phase>,
    /// One read of the screens at a time; whoever asks during one shares the next.
    reading: tokio::sync::Mutex<()>,
    read_queued: AtomicBool,
    /// The last toast swiped away: not shown again until Rust gives a new one.
    toast_done: AtomicU64,
    /// When someone last did something, in seconds since the epoch.
    touched: AtomicU64,
    /// Payments going out, which locking mustn't cut off.
    sending: AtomicU64,
}

/// The app behind a client's screens.
#[derive(Clone)]
pub struct Engine {
    inner: Arc<Inner>,
}

struct Observer(std::sync::Weak<Inner>);

impl SyncObserver for Observer {
    fn on_status(&self, status: SyncStatus) {
        let Some(inner) = self.0.upgrade() else { return };
        inner.listener.on_update(Update::Status(Arc::new(status)));
        let engine = Engine { inner };
        fob_wallet::runtime().spawn(async move { engine.refresh().await });
    }
}

fn key_error(e: fob_wallet::error::VaultError) -> FobError {
    fob_wallet::FobError::from(e).into()
}

impl Engine {
    pub fn new(options: Options, listener: Arc<dyn Listener>, clock: impl Fn() -> Clock + Send + Sync + 'static) -> Engine {
        let settings: Settings = std::fs::read(options.dir.join("settings.json")).ok().and_then(|text| serde_json::from_slice(&text).ok()).unwrap_or_default();
        set_scan_mode(scan_named(settings.scan_mode.as_deref()), options.cache_dir.to_string_lossy().into_owned());
        let phase = if passphrase::exists(&options.dir) { Phase::Locked } else { Phase::NoPassphrase };
        let touched = clock().now;
        Engine {
            inner: Arc::new(Inner {
                options,
                listener,
                clock: Box::new(clock),
                open: Mutex::new(None),
                settings: Mutex::new(settings),
                phase: Mutex::new(phase),
                reading: tokio::sync::Mutex::new(()),
                read_queued: AtomicBool::new(false),
                toast_done: AtomicU64::new(0),
                touched: AtomicU64::new(touched),
                sending: AtomicU64::new(0),
            }),
        }
    }

    pub fn phase(&self) -> Phase {
        self.inner.phase.lock().expect("phase lock").clone()
    }

    pub fn network(&self) -> Network {
        if self.inner.settings.lock().expect("settings lock").mainnet { Network::Mainnet } else { Network::Stagenet }
    }

    /// The wallet open, to read what its screens show. Spending goes through
    /// [`Engine::send`], which asks for the passphrase.
    pub fn wallet(&self) -> Option<Arc<FobWallet>> {
        self.inner.open.lock().expect("open lock").as_ref().and_then(|o| o.wallet.clone())
    }

    pub fn clock(&self) -> Clock {
        (self.inner.clock)()
    }

    fn dir(&self) -> &Path {
        &self.inner.options.dir
    }

    fn dir_text(&self) -> String {
        self.dir().to_string_lossy().into_owned()
    }

    fn set_phase(&self, phase: Phase) {
        *self.inner.phase.lock().expect("phase lock") = phase.clone();
        self.inner.listener.on_update(Update::Phase(phase));
    }

    fn save_settings(&self) -> Result<(), FobError> {
        let text = serde_json::to_vec_pretty(&*self.inner.settings.lock().expect("settings lock")).map_err(|e| FobError::Other { reason: e.to_string() })?;
        std::fs::create_dir_all(self.dir()).map_err(|e| FobError::Other { reason: e.to_string() })?;
        std::fs::write(self.dir().join("settings.json"), text).map_err(|e| FobError::Other { reason: e.to_string() })
    }

    /// Runs on the wallet's runtime, where its background work lives, wherever it's awaited from.
    async fn run<T: Send + 'static>(fut: impl Future<Output = T> + Send + 'static) -> T {
        match fob_wallet::runtime().spawn(fut).await {
            Ok(v) => v,
            Err(e) => std::panic::resume_unwind(e.into_panic()),
        }
    }

    // ---- The passphrase

    /// First run: chooses the passphrase, and unlocks.
    pub async fn set_up(&self, chosen: String) -> Result<(), FobError> {
        let chosen = Zeroizing::new(chosen);
        let this = self.clone();
        Self::run(async move {
            let dir = this.inner.options.dir.clone();
            let key = tokio::task::spawn_blocking(move || passphrase::create(&dir, &chosen)).await.expect("derive").map_err(key_error)?;
            this.unlocked(key).await
        })
        .await
    }

    /// Opens the vault, and the wallet last open on this network.
    pub async fn unlock(&self, typed: String) -> Result<(), FobError> {
        let typed = Zeroizing::new(typed);
        let this = self.clone();
        Self::run(async move {
            let dir = this.inner.options.dir.clone();
            let key = tokio::task::spawn_blocking(move || passphrase::unlock(&dir, &typed)).await.expect("derive").map_err(key_error)?;
            this.unlocked(key).await
        })
        .await
    }

    /// Checks the passphrase again, before showing keys or spending.
    pub async fn confirm(&self, typed: String) -> Result<(), FobError> {
        let typed = Zeroizing::new(typed);
        let dir = self.inner.options.dir.clone();
        Self::run(async move { tokio::task::spawn_blocking(move || passphrase::unlock(&dir, &typed)).await.expect("derive").map(|_| ()).map_err(key_error) }).await
    }

    pub async fn change_passphrase(&self, old: String, new: String) -> Result<(), FobError> {
        let (old, new) = (Zeroizing::new(old), Zeroizing::new(new));
        let dir = self.inner.options.dir.clone();
        Self::run(async move { tokio::task::spawn_blocking(move || passphrase::change(&dir, &old, &new)).await.expect("derive").map_err(key_error) }).await
    }

    /// Closes the wallet and forgets the key: the passphrase opens it again.
    pub fn lock(&self) {
        if let Some(wallet) = self.inner.open.lock().expect("open lock").take().and_then(|open| open.wallet) {
            Self::close(&wallet);
        }
        self.set_phase(if passphrase::exists(self.dir()) { Phase::Locked } else { Phase::NoPassphrase });
    }

    /// Someone did something: the idle clock starts again.
    pub fn touched(&self) {
        self.inner.touched.store(self.clock().now, Ordering::Relaxed);
    }

    /// Minutes untouched before locking; 0 for never.
    pub fn lock_after(&self) -> u32 {
        self.inner.settings.lock().expect("settings lock").lock_after
    }

    pub fn set_lock_after(&self, minutes: u32) -> Result<(), FobError> {
        self.inner.settings.lock().expect("settings lock").lock_after = minutes;
        self.touched();
        self.save_settings()
    }

    /// The desktop locked: this does too, unless a payment is going out.
    pub fn lock_with_the_desktop(&self) {
        if self.inner.sending.load(Ordering::SeqCst) == 0 && self.inner.open.lock().expect("open lock").is_some() {
            self.lock();
        }
    }

    /// Locks if unlocked and left long enough, unless a payment is going out.
    /// A client calls this now and then; returns whether it locked.
    pub fn lock_if_idle(&self) -> bool {
        let minutes = self.lock_after();
        if minutes == 0 || self.inner.sending.load(Ordering::SeqCst) > 0 || self.inner.open.lock().expect("open lock").is_none() {
            return false;
        }
        let idle = self.clock().now.saturating_sub(self.inner.touched.load(Ordering::Relaxed));
        if idle < u64::from(minutes) * 60 {
            return false;
        }
        self.lock();
        true
    }

    async fn unlocked(&self, key: Zeroizing<[u8; 32]>) -> Result<(), FobError> {
        std::fs::create_dir_all(self.dir()).map_err(|e| FobError::Other { reason: e.to_string() })?;
        let book = WalletList::open(self.dir_text(), key.to_vec())?;
        *self.inner.open.lock().expect("open lock") = Some(Open { book, key, wallet: None, current: None });
        self.touched();
        self.open_current().await;
        Ok(())
    }

    // ---- Which wallet

    fn parts(&self) -> Result<(Arc<WalletList>, Vec<u8>), FobError> {
        let open = self.inner.open.lock().expect("open lock");
        let open = open.as_ref().ok_or_else(|| FobError::Other { reason: "locked".into() })?;
        Ok((open.book.clone(), open.key.to_vec()))
    }

    fn publish_wallets(&self) {
        let open = self.inner.open.lock().expect("open lock");
        let Some(open) = open.as_ref() else { return };
        self.inner.listener.on_update(Update::Wallets { all: open.book.list(), current: open.current.clone(), network: self.network() });
    }

    fn close(wallet: &FobWallet) {
        wallet.stop_sync();
        wallet.stop_prices();
    }

    fn close_open(&self) {
        let mut open = self.inner.open.lock().expect("open lock");
        if let Some(open) = open.as_mut() {
            if let Some(wallet) = open.wallet.take() {
                Self::close(&wallet);
            }
            open.current = None;
        }
    }

    /// Opens the wallet last open on this network, if it has one.
    async fn open_current(&self) {
        let network = self.network();
        let Ok((book, key)) = self.parts() else { return };
        let current = if wallets_on(self.dir_text(), network) { book.current(network) } else { None };
        let Some(entry) = current else {
            self.publish_wallets();
            self.set_phase(Phase::NoWallet);
            return;
        };
        self.set_phase(Phase::Opening);
        let path = book.path(entry.file.clone());
        match FobWallet::open(path, key) {
            Ok(wallet) => self.use_wallet(wallet, entry).await,
            Err(e) => self.set_phase(Phase::Failed(e.to_string())),
        }
    }

    async fn use_wallet(&self, wallet: Arc<FobWallet>, entry: WalletEntry) {
        {
            let mut open = self.inner.open.lock().expect("open lock");
            if let Some(open) = open.as_mut() {
                if let Some(old) = open.wallet.replace(wallet.clone()) {
                    Self::close(&old);
                }
                open.current = Some(entry);
            }
        }
        self.inner.toast_done.store(0, Ordering::Relaxed);
        self.inner.listener.on_update(Update::Status(Arc::new(wallet.status())));
        self.publish_wallets();
        self.read_screens().await;
        self.set_phase(Phase::Open);
        if self.inner.options.online {
            let observer: Arc<dyn SyncObserver> = Arc::new(Observer(Arc::downgrade(&self.inner)));
            wallet.start_sync(observer.clone());
            wallet.start_prices(observer);
        }
    }

    /// Makes a wallet with `make` in a file of its own, named `name` (blank
    /// takes the next name), and opens it in place of any open.
    async fn adopt(&self, name: String, make: impl FnOnce(String, Vec<u8>, Network) -> Result<Arc<FobWallet>, FobError> + Send + 'static) -> Result<(), FobError> {
        let this = self.clone();
        Self::run(async move {
            let network = this.network();
            let (book, key) = this.parts()?;
            book.check_name(name.clone())?;
            let file = book.new_file(network);
            let wallet = make(book.path(file.clone()), key, network)?;
            let entry = book.add(file, name)?;
            this.use_wallet(wallet, entry).await;
            Ok(())
        })
        .await
    }

    pub async fn create_wallet(&self, name: String) -> Result<(), FobError> {
        self.adopt(name, FobWallet::create).await
    }

    pub async fn restore_words(&self, name: String, words: String, restore_point: Option<String>) -> Result<(), FobError> {
        self.adopt(name, move |path, key, network| FobWallet::restore_words(path, key, network, words, restore_point)).await
    }

    pub async fn restore_code(&self, name: String, code: String, restore_point: Option<String>) -> Result<(), FobError> {
        self.adopt(name, move |path, key, network| FobWallet::restore_code(path, key, network, code, restore_point)).await
    }

    pub async fn restore_view_only(&self, name: String, address: String, view_key: String, restore_point: Option<String>) -> Result<(), FobError> {
        self.adopt(name, move |path, key, network| FobWallet::restore_view_only(path, key, network, address, view_key, restore_point)).await
    }

    /// The name a new wallet gets unless it's given one.
    pub fn next_wallet_name(&self) -> Result<String, FobError> {
        Ok(self.parts()?.0.next_name())
    }

    pub fn check_wallet_name(&self, name: String) -> Result<(), FobError> {
        self.parts()?.0.check_name(name)
    }

    pub fn rename_wallet(&self, file: String, name: String) -> Result<(), FobError> {
        self.parts()?.0.rename(file.clone(), name)?;
        let list = self.parts()?.0.list();
        if let Some(open) = self.inner.open.lock().expect("open lock").as_mut()
            && open.current.as_ref().is_some_and(|c| c.file == file)
        {
            open.current = list.into_iter().find(|w| w.file == file);
        }
        self.publish_wallets();
        Ok(())
    }

    fn move_to(&self, network: Network) -> Result<(), FobError> {
        self.inner.settings.lock().expect("settings lock").mainnet = network == Network::Mainnet;
        self.save_settings()
    }

    /// Opens wallet `file` in place of this one, moving network with it if it's on the other.
    pub async fn switch_wallet(&self, file: String) -> Result<(), FobError> {
        let this = self.clone();
        Self::run(async move {
            let (book, _) = this.parts()?;
            let entry = book.list().into_iter().find(|w| w.file == file).ok_or_else(|| FobError::Other { reason: "no such wallet".into() })?;
            this.close_open();
            this.move_to(entry.network)?;
            book.select(file)?;
            this.open_current().await;
            Ok(())
        })
        .await
    }

    /// Moves to the wallet last open on the other network, if there is one.
    pub async fn switch_network(&self, network: Network) -> Result<(), FobError> {
        if network == self.network() {
            return Ok(());
        }
        let this = self.clone();
        Self::run(async move {
            this.close_open();
            this.move_to(network)?;
            this.open_current().await;
            Ok(())
        })
        .await
    }

    /// Takes the open wallet off this device; its seed restores it. Another
    /// then opens, this network's first; with none left, a new one is asked for.
    pub async fn forget_wallet(&self) -> Result<(), FobError> {
        let this = self.clone();
        Self::run(async move {
            let (book, _) = this.parts()?;
            let (wallet, current) = {
                let open = this.inner.open.lock().expect("open lock");
                let open = open.as_ref().expect("parts checked");
                (open.wallet.clone(), open.current.clone())
            };
            let wallet = wallet.ok_or_else(|| FobError::Other { reason: "no wallet open".into() })?;
            wallet.remove_file()?;
            this.close_open();
            if let Some(entry) = current {
                book.remove(entry.file)?;
            }
            let network = this.network();
            if let Some(next) = book.current(network).or_else(|| book.list().into_iter().next()) {
                this.move_to(next.network)?;
            }
            this.open_current().await;
            Ok(())
        })
        .await
    }

    // ---- What the screens show

    /// Reads everything the screens show and publishes it at once. Asked again
    /// during a read, it reads once more after it, however many asked.
    pub async fn refresh(&self) {
        let this = self.clone();
        Self::run(async move { this.read_screens().await }).await
    }

    async fn read_screens(&self) {
        if self.inner.read_queued.swap(true, Ordering::AcqRel) {
            return;
        }
        let _reading = self.inner.reading.lock().await;
        self.inner.read_queued.store(false, Ordering::Release);
        let Some(wallet) = self.wallet() else { return };
        let clock = self.clock();
        // The desktop has no screen lock to promise.
        let screens = wallet.screens(clock.now, clock.utc_offset, clock.hours_24, false).await;
        // Another wallet opened meanwhile: these answers are the last one's.
        if !self.wallet().is_some_and(|open| Arc::ptr_eq(&open, &wallet)) {
            return;
        }
        let toast = screens.price_toast.as_ref().filter(|t| t.id > self.inner.toast_done.load(Ordering::Relaxed)).map(|t| Arc::new(copy_toast(t)));
        self.inner.listener.on_update(Update::Toast(toast));
        self.inner.listener.on_update(Update::Screens(Arc::new(screens)));
    }

    /// The price toast swiped away: gone until Rust shows another.
    pub fn dismiss_toast(&self, id: u64) {
        self.inner.toast_done.store(id, Ordering::Relaxed);
        self.inner.listener.on_update(Update::Toast(None));
    }

    /// The toast's Retry: what it came to.
    pub async fn retry_prices(&self) -> Option<PriceRetry> {
        let wallet = self.wallet()?;
        let clock = self.clock();
        let this = self.clone();
        Some(
            Self::run(async move {
                let result = wallet.retry_prices(clock.now, clock.utc_offset, clock.hours_24).await;
                this.read_screens().await;
                result
            })
            .await,
        )
    }

    /// Asks the node now (and the price), and returns once that round is over.
    pub async fn sync_now(&self) {
        let Some(wallet) = self.wallet() else { return };
        let this = self.clone();
        Self::run(async move {
            wallet.sync_now().await;
            this.read_screens().await;
        })
        .await
    }

    /// The card behind the sync chip, every word from Rust.
    pub fn sync_details(&self) -> Option<SyncDetails> {
        let wallet = self.wallet()?;
        let clock = self.clock();
        Some(wallet.sync_details(clock.now, clock.utc_offset, clock.hours_24))
    }

    /// Runs `call` with the open wallet on its runtime, then reads the screens again.
    pub async fn change<T: Send + 'static, F: Future<Output = Result<T, FobError>> + Send + 'static>(&self, call: impl FnOnce(Arc<FobWallet>) -> F + Send + 'static) -> Result<T, FobError> {
        let this = self.clone();
        Self::run(async move {
            let wallet = this.wallet().ok_or_else(|| FobError::Other { reason: "no wallet open".into() })?;
            let result = call(wallet).await;
            this.read_screens().await;
            result
        })
        .await
    }

    /// Reads from the open wallet on its runtime.
    pub async fn read<T: Send + 'static, F: Future<Output = T> + Send + 'static>(&self, call: impl FnOnce(Arc<FobWallet>, Clock) -> F + Send + 'static) -> Option<T> {
        let wallet = self.wallet()?;
        let clock = self.clock();
        Some(Self::run(async move { call(wallet, clock).await }).await)
    }

    // ---- Spending

    /// Sends a prepared payment, once the passphrase has been given again. It
    /// runs to the end whatever the screen does; Rust records it either way.
    pub async fn send(&self, prepared: Arc<Prepared>, typed: String) -> Result<String, FobError> {
        let typed = Zeroizing::new(typed);
        let this = self.clone();
        Self::run(async move {
            let dir = this.inner.options.dir.clone();
            tokio::task::spawn_blocking(move || passphrase::unlock(&dir, &typed)).await.expect("derive").map_err(key_error)?;
            let wallet = this.wallet().ok_or_else(|| FobError::Other { reason: "no wallet open".into() })?;
            this.inner.sending.fetch_add(1, Ordering::SeqCst);
            let sent = wallet.send(prepared.id).await;
            this.inner.sending.fetch_sub(1, Ordering::SeqCst);
            this.touched();
            this.read_screens().await;
            sent
        })
        .await
    }

    // ---- Meeting someone

    /// Their profile, from a code: saved at once unless it would change a
    /// contact, which is asked first. It came from both people showing a code,
    /// so they have this wallet's address too.
    pub async fn meet_save(&self, text: String) -> Result<Met, FobError> {
        let wallet = self.wallet().ok_or_else(|| FobError::Other { reason: "no wallet open".into() })?;
        let asked = text.clone();
        let found = Self::run(async move { wallet.scanned_profile(asked).await }).await?;
        let contact = match &found.clash {
            None => self.change(move |wallet| async move { wallet.add_contact_from_profile(text, false, true).await }).await?,
            Some(clash) if clash.changes.is_empty() => {
                let id = clash.contact.id;
                self.change(move |wallet| async move { wallet.update_contact_from_profile(id, text, true).await }).await?
            }
            Some(_) => return Ok(Met::Ask(found)),
        };
        Ok(Met::Saved(contact, found))
    }

    /// Forgets a prepared payment left without sending.
    pub fn cancel(&self, prepared: &Prepared) {
        let Some(wallet) = self.wallet() else { return };
        let id = prepared.id;
        fob_wallet::runtime().spawn(async move { wallet.cancel(id).await });
    }

    // ---- Settings

    pub fn scan_mode(&self) -> ScanMode {
        scan_named(self.inner.settings.lock().expect("settings lock").scan_mode.as_deref())
    }

    pub fn set_scan(&self, mode: ScanMode) -> Result<(), FobError> {
        self.inner.settings.lock().expect("settings lock").scan_mode = Some(scan_name(&mode).to_owned());
        set_scan_mode(mode, self.inner.options.cache_dir.to_string_lossy().into_owned());
        self.save_settings()
    }
}

fn copy_toast(t: &PriceToast) -> PriceToast {
    PriceToast { id: t.id, title: t.title.clone(), line: t.line.clone(), retrying_title: t.retrying_title.clone(), retrying_line: t.retrying_line.clone() }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex as StdMutex;

    use super::*;

    #[derive(Default)]
    struct Heard {
        phases: StdMutex<Vec<Phase>>,
        screens: StdMutex<u32>,
        wallets: StdMutex<Vec<(Vec<WalletEntry>, Option<WalletEntry>)>>,
    }

    impl Listener for Heard {
        fn on_update(&self, update: Update) {
            match update {
                Update::Phase(phase) => self.phases.lock().unwrap().push(phase),
                Update::Screens(_) => *self.screens.lock().unwrap() += 1,
                Update::Wallets { all, current, .. } => self.wallets.lock().unwrap().push((all, current)),
                Update::Status(_) | Update::Toast(_) => {}
            }
        }
    }

    fn dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("fob-app-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn engine_at(dir: &Path) -> (Engine, Arc<Heard>) {
        let heard = Arc::new(Heard::default());
        let options = Options { dir: dir.to_owned(), cache_dir: dir.join("cache"), online: false };
        (Engine::new(options, heard.clone(), || Clock { now: 1_793_448_000, utc_offset: 0, hours_24: true }), heard)
    }

    const PASS: &str = "correct horse";

    #[tokio::test]
    async fn first_run_asks_for_a_passphrase_then_for_a_wallet_which_opens() {
        let dir = dir("first-run");
        let (engine, heard) = engine_at(&dir);
        assert_eq!(engine.phase(), Phase::NoPassphrase);
        assert!(engine.set_up("short".into()).await.is_err());
        assert_eq!(engine.phase(), Phase::NoPassphrase, "refused, still asking");
        engine.set_up(PASS.into()).await.unwrap();
        assert_eq!(engine.phase(), Phase::NoWallet);
        assert_eq!(engine.network(), Network::Stagenet, "test coins until asked");
        engine.create_wallet(String::new()).await.unwrap();
        assert_eq!(engine.phase(), Phase::Open);
        let (_, current) = heard.wallets.lock().unwrap().last().cloned().unwrap();
        assert!(!current.unwrap().name.is_empty(), "blank takes the next name");
        assert!(engine.wallet().is_some());
        assert!(*heard.screens.lock().unwrap() >= 1, "screens have something on their first frame");
        assert!(heard.phases.lock().unwrap().ends_with(&[Phase::Open]));
    }

    #[tokio::test]
    async fn the_passphrase_opens_the_last_wallet_and_only_it_does() {
        let dir = dir("unlock");
        let (engine, _) = engine_at(&dir);
        engine.set_up(PASS.into()).await.unwrap();
        engine.create_wallet("Spending".into()).await.unwrap();
        let address = engine.wallet().unwrap().primary_address();
        engine.lock();
        assert_eq!(engine.phase(), Phase::Locked);
        assert!(engine.wallet().is_none(), "nothing left open");

        let (again, _) = engine_at(&dir);
        assert_eq!(again.phase(), Phase::Locked);
        assert!(matches!(again.unlock("not the passphrase".into()).await, Err(FobError::Invalid { .. })));
        assert_eq!(again.phase(), Phase::Locked);
        again.unlock(PASS.into()).await.unwrap();
        assert_eq!(again.phase(), Phase::Open);
        assert_eq!(again.wallet().unwrap().primary_address(), address);
    }

    #[tokio::test]
    async fn wallets_switch_across_networks_and_the_choice_is_kept() {
        let dir = dir("switch");
        let (engine, heard) = engine_at(&dir);
        engine.set_up(PASS.into()).await.unwrap();
        engine.create_wallet("Test".into()).await.unwrap();
        let test = engine.wallet().unwrap().primary_address();
        engine.switch_network(Network::Mainnet).await.unwrap();
        assert_eq!(engine.phase(), Phase::NoWallet, "none on mainnet yet");
        engine.create_wallet("Real".into()).await.unwrap();
        let real = engine.wallet().unwrap().primary_address();
        assert_ne!(test, real);
        let (all, current) = heard.wallets.lock().unwrap().last().cloned().unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(current.unwrap().name, "Real");

        let other = all.iter().find(|w| w.name == "Test").unwrap().file.clone();
        engine.switch_wallet(other).await.unwrap();
        assert_eq!(engine.network(), Network::Stagenet, "moves with the wallet");
        assert_eq!(engine.wallet().unwrap().primary_address(), test);

        let (reopened, _) = engine_at(&dir);
        assert_eq!(reopened.network(), Network::Stagenet);
        reopened.unlock(PASS.into()).await.unwrap();
        assert_eq!(reopened.wallet().unwrap().primary_address(), test);
    }

    #[tokio::test]
    async fn forgetting_moves_to_another_wallet_then_asks_for_a_new_one() {
        let dir = dir("forget");
        let (engine, _) = engine_at(&dir);
        engine.set_up(PASS.into()).await.unwrap();
        engine.create_wallet("One".into()).await.unwrap();
        let one = engine.wallet().unwrap().primary_address();
        engine.create_wallet("Two".into()).await.unwrap();
        engine.forget_wallet().await.unwrap();
        assert_eq!(engine.phase(), Phase::Open);
        assert_eq!(engine.wallet().unwrap().primary_address(), one);
        engine.forget_wallet().await.unwrap();
        assert_eq!(engine.phase(), Phase::NoWallet);
        assert!(engine.wallet().is_none());
    }

    #[tokio::test]
    async fn spending_and_keys_need_the_passphrase_again() {
        let dir = dir("confirm");
        let (engine, _) = engine_at(&dir);
        engine.set_up(PASS.into()).await.unwrap();
        engine.create_wallet("Spending".into()).await.unwrap();
        assert!(engine.confirm("not the passphrase".into()).await.is_err());
        engine.confirm(PASS.into()).await.unwrap();
        let prepared = Arc::new(Prepared {
            id: 1,
            to: String::new(),
            amount_text: String::new(),
            fee_text: String::new(),
            total_text: String::new(),
            amount_fiat: None,
            fee_fiat: None,
            total_fiat: None,
            fiat_age: None,
            receipt: None,
            split: None,
        });
        let refused = engine.send(prepared, "not the passphrase".into()).await.unwrap_err();
        assert_eq!(refused.to_string(), "That isn't the passphrase", "refused before the wallet is asked");
    }

    #[tokio::test]
    async fn changing_the_passphrase_leaves_wallets_as_they_are() {
        let dir = dir("change");
        let (engine, _) = engine_at(&dir);
        engine.set_up(PASS.into()).await.unwrap();
        engine.create_wallet("Spending".into()).await.unwrap();
        let address = engine.wallet().unwrap().primary_address();
        engine.change_passphrase(PASS.into(), "battery staple".into()).await.unwrap();
        engine.lock();
        assert!(engine.unlock(PASS.into()).await.is_err());
        engine.unlock("battery staple".into()).await.unwrap();
        assert_eq!(engine.wallet().unwrap().primary_address(), address);
    }

    /// Two people on two computers, each reading the other's code.
    #[tokio::test]
    async fn two_people_meet_and_each_ends_up_a_contact() {
        let (one, two) = (dir("meet-one"), dir("meet-two"));
        let ((sam, _), (ana, _)) = (engine_at(&one), engine_at(&two));
        for (engine, name) in [(&sam, "Sam"), (&ana, "Ana")] {
            engine.set_up(PASS.into()).await.unwrap();
            engine.create_wallet(name.into()).await.unwrap();
            let name = name.to_owned();
            engine.change(move |wallet| async move { wallet.set_profile(name, None).await }).await.unwrap();
        }
        let first = sam.change(|wallet| async move { wallet.share_profile().await }).await.unwrap();

        // Ana reads Sam's first code: saved, not a reply, so Ana answers.
        let Met::Saved(contact, found) = ana.meet_save(first.code.clone()).await.ok().unwrap() else { panic!("nothing to ask") };
        assert_eq!(contact.name, "Sam");
        assert!(!found.reply);
        let shared = ana.change(|wallet| async move { wallet.share_profile().await }).await.unwrap();
        let answer = ana.read(move |wallet, _| async move { wallet.share_reply_code(shared.id).await }).await.flatten().expect("a reply code");

        // Sam reads the answer: saved, and it's a reply, so Sam is done.
        let Met::Saved(contact, found) = sam.meet_save(answer.clone()).await.ok().unwrap() else { panic!("nothing to ask") };
        assert_eq!(contact.name, "Ana");
        assert!(found.reply);

        // The same code again changes nothing, so nothing is asked.
        let Met::Saved(again, _) = sam.meet_save(answer).await.ok().unwrap() else { panic!("nothing changed") };
        assert_eq!(again.id, contact.id);
        // Something that isn't a profile is refused.
        assert!(sam.meet_save("hello".into()).await.is_err());
    }

    #[tokio::test]
    async fn an_untouched_app_locks_after_its_time_and_not_before() {
        let dir = dir("idle");
        let now = Arc::new(AtomicU64::new(1_000_000));
        let (at, heard) = (now.clone(), Arc::new(Heard::default()));
        let options = Options { dir: dir.clone(), cache_dir: dir.join("cache"), online: false };
        let engine = Engine::new(options, heard, move || Clock { now: at.load(Ordering::SeqCst), utc_offset: 0, hours_24: true });

        assert!(!engine.lock_if_idle(), "nothing's unlocked yet");
        engine.set_up(PASS.into()).await.unwrap();
        assert_eq!(engine.lock_after(), 15);

        now.fetch_add(14 * 60, Ordering::SeqCst);
        assert!(!engine.lock_if_idle());
        engine.touched();
        now.fetch_add(14 * 60, Ordering::SeqCst);
        assert!(!engine.lock_if_idle(), "being touched starts the clock again");

        engine.inner.sending.fetch_add(1, Ordering::SeqCst);
        now.fetch_add(60 * 60, Ordering::SeqCst);
        assert!(!engine.lock_if_idle(), "a payment going out isn't cut off");
        engine.inner.sending.fetch_sub(1, Ordering::SeqCst);

        engine.set_lock_after(0).unwrap();
        now.fetch_add(60 * 60, Ordering::SeqCst);
        assert!(!engine.lock_if_idle(), "never means never");

        engine.set_lock_after(5).unwrap();
        now.fetch_add(5 * 60, Ordering::SeqCst);
        assert!(engine.lock_if_idle());
        assert_eq!(engine.phase(), Phase::Locked);
        assert!(!engine.lock_if_idle(), "already locked");
    }

    #[tokio::test]
    async fn the_scan_mode_is_kept() {
        let dir = dir("scan");
        let (engine, _) = engine_at(&dir);
        assert!(matches!(engine.scan_mode(), ScanMode::Auto));
        engine.set_scan(ScanMode::Cpu).unwrap();
        let (again, _) = engine_at(&dir);
        assert!(matches!(again.scan_mode(), ScanMode::Cpu));
        again.set_scan(ScanMode::Auto).unwrap();
    }
}
