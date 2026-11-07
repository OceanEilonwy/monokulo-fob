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
    time::{Duration, Instant},
};

use fob_ffi::{
    Contact, FobError, FobWallet, KeyGroup, Network, PriceRetry, PriceToast, Prepared, PullOutcome, ScanMode, ScannedProfile, Screens, SeedPhrase, SyncDetails, SyncObserver, SyncStatus, WalletEntry, WalletList, set_scan_mode,
    wallets_on,
};
use fob_wallet::{files, passphrase};
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

/// Where what's published goes. Comes from any thread, and mustn't call back
/// into the engine: a phase is published with the engine's lock on it held, so
/// that a lock and an open finishing at once can't arrive in the wrong order.
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

/// How long a [`Confirmed`] holds: long enough to read what was asked for,
/// short enough that one left behind by a bug is soon worthless.
const CONFIRMED_FOR: Duration = Duration::from_secs(60);

/// Proof that the passphrase was just typed again, from [`Engine::confirm`],
/// which the calls that show secrets or spend take. It isn't `Clone` and each
/// of them takes it by value, so one passphrase does one thing. It holds for
/// [`CONFIRMED_FOR`], and only while the wallet open when it was given stays
/// open: a lock, an unlock or another wallet opening makes it worthless.
pub struct Confirmed {
    generation: u64,
    at: Instant,
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
    /// Moves on at every lock, unlock and wallet open, always with `open`
    /// held. An open that finishes after it moved on was overtaken, by a lock
    /// or by another wallet, and closes what it opened instead of keeping it;
    /// a [`Confirmed`] from before it moved on is refused.
    generation: AtomicU64,
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
    fn on_status(&self, status: SyncStatus) -> Result<(), FobError> {
        let Some(inner) = self.0.upgrade() else { return Ok(()) };
        inner.listener.on_update(Update::Status(Arc::new(status)));
        let engine = Engine { inner };
        fob_wallet::runtime().spawn(async move { engine.refresh().await });
        Ok(())
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
                generation: AtomicU64::new(0),
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

    /// The wallet open, for what its screens show and change. What shows
    /// secrets or spends goes through the engine, which asks for a
    /// [`Confirmed`]: [`Engine::seed_phrase`], [`Engine::wallet_code`],
    /// [`Engine::keys_page`], [`Engine::send`] and [`Engine::forget_wallet`].
    /// `FobWallet` still has those methods, as Android calls them itself, so
    /// the desktop client's clippy.toml refuses them there.
    pub fn wallet(&self) -> Option<Arc<FobWallet>> {
        self.inner.open.lock().expect("open lock").as_ref().and_then(|o| o.wallet.clone())
    }

    fn open_wallet(&self) -> Result<Arc<FobWallet>, FobError> {
        self.wallet().ok_or_else(|| FobError::Other { reason: "no wallet open".into() })
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

    /// Publishes with the phase's lock held, so phases arrive in the order they're set.
    fn set_phase(&self, phase: Phase) {
        let mut now = self.inner.phase.lock().expect("phase lock");
        *now = phase.clone();
        self.inner.listener.on_update(Update::Phase(phase));
    }

    /// Publishes `phase` for the open of `generation`, unless a lock or
    /// another open has moved on since, which publishes its own. It's checked
    /// under the phase's lock, so a lock that moves on just after the check
    /// publishes after this, and its phase is the one left.
    fn set_phase_for(&self, generation: u64, phase: Phase) -> bool {
        let mut now = self.inner.phase.lock().expect("phase lock");
        if !self.current(generation) {
            return false;
        }
        *now = phase.clone();
        self.inner.listener.on_update(Update::Phase(phase));
        true
    }

    fn current(&self, generation: u64) -> bool {
        self.inner.generation.load(Ordering::SeqCst) == generation
    }

    /// Moves the generation on for an open about to start, and returns it;
    /// `None` when locked, with nothing to open.
    fn next_generation(&self) -> Option<u64> {
        let open = self.inner.open.lock().expect("open lock");
        open.as_ref()?;
        Some(self.inner.generation.fetch_add(1, Ordering::SeqCst) + 1)
    }

    fn save_settings(&self) -> Result<(), FobError> {
        let text = serde_json::to_vec_pretty(&*self.inner.settings.lock().expect("settings lock")).map_err(|e| FobError::Other { reason: e.to_string() })?;
        // Only this user's: which network and how soon it locks are nobody else's business.
        files::write_private(&self.dir().join("settings.json"), &text).map_err(|e| FobError::Other { reason: e.to_string() })
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

    /// Checks the passphrase again, before showing keys or spending: what it
    /// gives is what those calls take.
    pub async fn confirm(&self, typed: String) -> Result<Confirmed, FobError> {
        let typed = Zeroizing::new(typed);
        let dir = self.inner.options.dir.clone();
        // Taken before the check, so a lock while the key is derived (a second
        // or so) already makes the proof worthless.
        let generation = self.inner.generation.load(Ordering::SeqCst);
        Self::run(async move { tokio::task::spawn_blocking(move || passphrase::unlock(&dir, &typed)).await.expect("derive").map(|_| ()).map_err(key_error) }).await?;
        Ok(Confirmed { generation, at: Instant::now() })
    }

    /// Spends `confirmed`, refusing it once it's too old or from before a
    /// lock, an unlock or another wallet opening.
    fn check(&self, confirmed: Confirmed) -> Result<(), FobError> {
        if !self.current(confirmed.generation) || confirmed.at.elapsed() >= CONFIRMED_FOR {
            return Err(FobError::Invalid { reason: "Type your passphrase again".into() });
        }
        Ok(())
    }

    pub async fn change_passphrase(&self, old: String, new: String) -> Result<(), FobError> {
        let (old, new) = (Zeroizing::new(old), Zeroizing::new(new));
        let dir = self.inner.options.dir.clone();
        Self::run(async move { tokio::task::spawn_blocking(move || passphrase::change(&dir, &old, &new)).await.expect("derive").map_err(key_error) }).await
    }

    /// Closes the wallet and forgets the key: the passphrase opens it again.
    /// An open still under way when it locks finds the generation moved on and
    /// closes what it opened itself.
    pub fn lock(&self) {
        let taken = {
            let mut open = self.inner.open.lock().expect("open lock");
            self.inner.generation.fetch_add(1, Ordering::SeqCst);
            open.take()
        };
        if let Some(wallet) = taken.and_then(|open| open.wallet) {
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
        files::private_dir(self.dir()).map_err(|e| FobError::Other { reason: e.to_string() })?;
        let book = WalletList::open(self.dir_text(), Self::key_for_ffi(&key))?;
        let replaced = {
            let mut open = self.inner.open.lock().expect("open lock");
            self.inner.generation.fetch_add(1, Ordering::SeqCst);
            open.replace(Open { book, key, wallet: None, current: None })
        };
        // Unlocked while already unlocked: the wallet open is closed, not left
        // syncing and saving beside the one about to open on the same file.
        if let Some(wallet) = replaced.and_then(|open| open.wallet) {
            Self::close(&wallet);
        }
        self.touched();
        self.open_current().await;
        Ok(())
    }

    // ---- Which wallet

    fn parts(&self) -> Result<(Arc<WalletList>, Zeroizing<[u8; 32]>), FobError> {
        let open = self.inner.open.lock().expect("open lock");
        let open = open.as_ref().ok_or_else(|| FobError::Other { reason: "locked".into() })?;
        Ok((open.book.clone(), open.key.clone()))
    }

    /// The file key as `fob-ffi` takes it, by value as a `Vec` because that's
    /// what crosses to Kotlin too. The copy is made only at the call, and `fob-ffi`
    /// drops it there, where its allocator wipes freed memory; everywhere else
    /// in here the key stays `Zeroizing`.
    fn key_for_ffi(key: &Zeroizing<[u8; 32]>) -> Vec<u8> {
        key.to_vec()
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
        let Some(generation) = self.next_generation() else { return };
        let Ok((book, key)) = self.parts() else { return };
        let current = if wallets_on(self.dir_text(), network) { book.current(network) } else { None };
        let Some(entry) = current else {
            self.publish_wallets();
            self.set_phase_for(generation, Phase::NoWallet);
            return;
        };
        if !self.set_phase_for(generation, Phase::Opening) {
            return;
        }
        let path = book.path(entry.file.clone());
        match FobWallet::open(path, Self::key_for_ffi(&key)) {
            Ok(wallet) => self.use_wallet(wallet, entry, generation).await,
            Err(e) => {
                self.set_phase_for(generation, Phase::Failed(e.to_string()));
            }
        }
    }

    /// Keeps `wallet`, opened for `generation`, as the one open, unless a lock
    /// or another open overtook it meanwhile. Then nobody could stop or see
    /// it: left alone it would keep its keys in memory after "Locked", and
    /// save its whole state over the next one opened on the same file. So it's
    /// closed and dropped here, and the phase is left to whoever moved on.
    async fn use_wallet(&self, wallet: Arc<FobWallet>, entry: WalletEntry, generation: u64) {
        let kept = {
            let mut open = self.inner.open.lock().expect("open lock");
            match open.as_mut().filter(|_| self.current(generation)) {
                Some(open) => {
                    if let Some(old) = open.wallet.replace(wallet.clone()) {
                        Self::close(&old);
                    }
                    open.current = Some(entry);
                    true
                }
                None => false,
            }
        };
        if !kept {
            Self::close(&wallet);
            return;
        }
        self.inner.toast_done.store(0, Ordering::Relaxed);
        self.inner.listener.on_update(Update::Status(Arc::new(wallet.status())));
        self.publish_wallets();
        self.read_screens().await;
        {
            // Started with `open` held and the generation checked: a lock from
            // here on takes this wallet and stops what's started, and one
            // before has already closed it, so nothing starts on it.
            let _open = self.inner.open.lock().expect("open lock");
            if !self.current(generation) {
                return;
            }
            if self.inner.options.online {
                let observer: Arc<dyn SyncObserver> = Arc::new(Observer(Arc::downgrade(&self.inner)));
                wallet.start_sync(observer.clone());
                wallet.start_prices(observer);
            }
        }
        self.set_phase_for(generation, Phase::Open);
    }

    /// Makes a wallet with `make` in a file of its own, named `name` (blank
    /// takes the next name), and opens it in place of any open.
    async fn adopt(&self, name: String, make: impl FnOnce(String, Vec<u8>, Network) -> Result<Arc<FobWallet>, FobError> + Send + 'static) -> Result<(), FobError> {
        let this = self.clone();
        Self::run(async move {
            let network = this.network();
            let (book, key) = this.parts()?;
            book.check_name(name.clone())?;
            let generation = this.next_generation().ok_or_else(|| FobError::Other { reason: "locked".into() })?;
            let file = book.new_file(network);
            let wallet = make(book.path(file.clone()), Self::key_for_ffi(&key), network)?;
            let entry = book.add(file, name)?;
            this.use_wallet(wallet, entry, generation).await;
            Ok(())
        })
        .await
    }

    pub async fn create_wallet(&self, name: String) -> Result<(), FobError> {
        self.adopt(name, FobWallet::create).await
    }

    /// The words are wiped here if the wallet is never made; `fob-ffi` takes
    /// its own copy by value, as Kotlin's bindings need.
    pub async fn restore_words(&self, name: String, words: Zeroizing<String>, restore_point: Option<String>) -> Result<(), FobError> {
        self.adopt(name, move |path, key, network| FobWallet::restore_words(path, key, network, String::clone(&words), restore_point)).await
    }

    pub async fn restore_code(&self, name: String, code: Zeroizing<String>, restore_point: Option<String>) -> Result<(), FobError> {
        self.adopt(name, move |path, key, network| FobWallet::restore_code(path, key, network, String::clone(&code), restore_point)).await
    }

    pub async fn restore_view_only(&self, name: String, address: String, view_key: String, restore_point: Option<String>) -> Result<(), FobError> {
        self.adopt(name, move |path, key, network| FobWallet::restore_view_only(path, key, network, address, view_key, restore_point)).await
    }

    // ---- A Trezor

    /// Opens the Trezor on the USB port and runs `with` on it, the device's
    /// questions going through `prompter`; the device is let go after, and
    /// the prompter told the exchange is over, whatever happened.
    async fn with_trezor<T: Send + 'static>(
        prompter: Arc<crate::trezor::Prompter>,
        with: impl FnOnce(Arc<crate::trezor::Usb>, Arc<crate::trezor::Prompter>) -> std::pin::Pin<Box<dyn Future<Output = Result<T, FobError>> + Send>> + Send + 'static,
    ) -> Result<T, FobError> {
        let link = match crate::trezor::Usb::open() {
            Ok(link) => link,
            Err(e) => {
                prompter.over();
                return Err(e);
            }
        };
        prompter.hold(link.clone());
        let result = Self::run(with(link.clone(), prompter.clone())).await;
        link.close();
        prompter.over();
        result
    }

    /// A wallet whose keys are on the Trezor plugged in: the device gives the
    /// view key (asking first, and showing the address to compare).
    pub async fn restore_trezor(&self, name: String, restore_point: Option<String>, prompter: Arc<crate::trezor::Prompter>) -> Result<(), FobError> {
        let this = self.clone();
        Self::with_trezor(prompter, move |link, prompter| {
            Box::pin(async move {
                let (link, prompts): (Arc<dyn fob_ffi::TrezorLink>, Arc<dyn fob_ffi::TrezorPrompts>) = (link, prompter);
                this.adopt(name, move |path, key, network| FobWallet::restore_trezor(path, key, network, link, prompts, restore_point)).await
            })
        })
        .await
    }

    /// Asks the Trezor for the key images it hasn't given; how many outputs that covered.
    pub async fn trezor_sync(&self, prompter: Arc<crate::trezor::Prompter>) -> Result<u32, FobError> {
        let this = self.clone();
        Self::with_trezor(prompter, move |link, prompter| {
            Box::pin(async move {
                let wallet = this.open_wallet()?;
                let synced = wallet.trezor_sync(link, prompter).await;
                this.read_screens().await;
                synced
            })
        })
        .await
    }

    /// Sends a prepared payment signed on the Trezor, which confirms it on
    /// its own screen: no passphrase here. Like [`send`](Engine::send), it
    /// runs to the end whatever the screen does.
    pub async fn send_with_trezor(&self, prepared: Arc<Prepared>, prompter: Arc<crate::trezor::Prompter>) -> Result<String, FobError> {
        let this = self.clone();
        Self::with_trezor(prompter, move |link, prompter| {
            Box::pin(async move {
                let wallet = this.open_wallet()?;
                this.inner.sending.fetch_add(1, Ordering::SeqCst);
                let sent = wallet.send_with_trezor(prepared.id, link, prompter).await;
                this.inner.sending.fetch_sub(1, Ordering::SeqCst);
                this.touched();
                this.read_screens().await;
                sent
            })
        })
        .await
    }

    /// Whether the open wallet's spend key is on a Trezor.
    pub fn is_trezor(&self) -> bool {
        self.wallet().is_some_and(|w| w.is_trezor())
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
    /// It takes the passphrase again first, as showing keys does: a wallet
    /// whose seed was never written down is lost with its file, and a desktop
    /// is often left unlocked.
    pub async fn forget_wallet(&self, confirmed: Confirmed) -> Result<(), FobError> {
        self.check(confirmed)?;
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
    pub async fn sync_now(&self) -> Result<Option<PullOutcome>, FobError> {
        let Some(wallet) = self.wallet() else { return Ok(None) };
        let this = self.clone();
        Self::run(async move {
            // With Manual Sync, what a refresh does is the wallet's own setting.
            let said = if wallet.manual_settings().await.on {
                wallet.pull_refresh().await
            } else {
                wallet.sync_now().await;
                Ok(None)
            };
            this.read_screens().await;
            said
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
    pub async fn send(&self, prepared: Arc<Prepared>, confirmed: Confirmed) -> Result<String, FobError> {
        self.check(confirmed)?;
        let this = self.clone();
        Self::run(async move {
            let wallet = this.open_wallet()?;
            this.inner.sending.fetch_add(1, Ordering::SeqCst);
            let sent = wallet.send(prepared.id).await;
            this.inner.sending.fetch_sub(1, Ordering::SeqCst);
            this.touched();
            this.read_screens().await;
            sent
        })
        .await
    }

    // ---- Secrets

    /// The seed phrase, once the passphrase has been given again; `None` for
    /// a wallet restored from keys, which has none.
    pub fn seed_phrase(&self, confirmed: Confirmed) -> Result<Option<SeedPhrase>, FobError> {
        self.check(confirmed)?;
        Ok(self.open_wallet()?.seed_phrase())
    }

    /// The wallet code's text, once the passphrase has been given again.
    pub async fn wallet_code(&self, confirmed: Confirmed) -> Result<Vec<u8>, FobError> {
        self.check(confirmed)?;
        let wallet = self.open_wallet()?;
        Ok(Self::run(async move { wallet.wallet_code().await }).await)
    }

    /// The keys, once the passphrase has been given again.
    pub async fn keys_page(&self, confirmed: Confirmed) -> Result<Vec<KeyGroup>, FobError> {
        self.check(confirmed)?;
        let wallet = self.open_wallet()?;
        Ok(Self::run(async move { wallet.keys_page().await }).await)
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
        let two = engine.wallet().unwrap().primary_address();
        let refused = engine.confirm("not the passphrase".into()).await.err().unwrap();
        assert_eq!(refused.to_string(), "That isn't the passphrase");
        assert_eq!(engine.wallet().unwrap().primary_address(), two, "nothing removed without it");
        engine.forget_wallet(engine.confirm(PASS.into()).await.unwrap()).await.unwrap();
        assert_eq!(engine.phase(), Phase::Open);
        assert_eq!(engine.wallet().unwrap().primary_address(), one);
        engine.forget_wallet(engine.confirm(PASS.into()).await.unwrap()).await.unwrap();
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
        let seed = engine.seed_phrase(engine.confirm(PASS.into()).await.unwrap()).unwrap().expect("a new wallet has a seed");
        assert!(!seed.phrase.is_empty());
        assert!(!engine.keys_page(engine.confirm(PASS.into()).await.unwrap()).await.unwrap().is_empty());
        assert!(!engine.wallet_code(engine.confirm(PASS.into()).await.unwrap()).await.unwrap().is_empty());
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
        // A minute after the passphrase, it no longer counts.
        let generation = engine.inner.generation.load(Ordering::SeqCst);
        let stale = Confirmed { generation, at: Instant::now().checked_sub(CONFIRMED_FOR).unwrap() };
        let refused = engine.send(prepared, stale).await.unwrap_err();
        assert_eq!(refused.to_string(), "Type your passphrase again", "refused before the wallet is asked");
        let stale = Confirmed { generation, at: Instant::now().checked_sub(CONFIRMED_FOR).unwrap() };
        assert!(engine.seed_phrase(stale).is_err());
    }

    #[tokio::test]
    async fn the_passphrase_counts_only_for_the_wallet_open_when_it_was_typed() {
        let dir = dir("confirm-generation");
        let (engine, _) = engine_at(&dir);
        engine.set_up(PASS.into()).await.unwrap();
        engine.create_wallet("One".into()).await.unwrap();

        let before_lock = engine.confirm(PASS.into()).await.unwrap();
        engine.lock();
        engine.unlock(PASS.into()).await.unwrap();
        assert_eq!(engine.seed_phrase(before_lock).err().unwrap().to_string(), "Type your passphrase again");

        let before_switch = engine.confirm(PASS.into()).await.unwrap();
        engine.create_wallet("Two".into()).await.unwrap();
        assert!(engine.keys_page(before_switch).await.is_err(), "given for another wallet");
        assert!(engine.forget_wallet(engine.confirm(PASS.into()).await.unwrap()).await.is_ok());
    }

    /// Once armed, locks from another thread the moment an open starts, and
    /// returns only once that lock has moved the generation on, so the open
    /// finishes after it. The listener can't lock itself: phases are published
    /// with the phase's lock held.
    #[derive(Default)]
    struct LockWhileOpening {
        engine: std::sync::OnceLock<std::sync::Weak<Inner>>,
        armed: AtomicBool,
        locker: StdMutex<Option<std::thread::JoinHandle<()>>>,
        phases: StdMutex<Vec<Phase>>,
    }

    impl Listener for LockWhileOpening {
        fn on_update(&self, update: Update) {
            let Update::Phase(phase) = update else { return };
            let opening = phase == Phase::Opening;
            self.phases.lock().unwrap().push(phase);
            if !opening || !self.armed.swap(false, Ordering::SeqCst) {
                return;
            }
            let Some(inner) = self.engine.get().and_then(std::sync::Weak::upgrade) else { return };
            let before = inner.generation.load(Ordering::SeqCst);
            let engine = Engine { inner: inner.clone() };
            *self.locker.lock().unwrap() = Some(std::thread::spawn(move || engine.lock()));
            while inner.generation.load(Ordering::SeqCst) == before {
                std::thread::yield_now();
            }
        }
    }

    #[tokio::test]
    async fn locking_while_a_wallet_opens_leaves_it_locked_with_nothing_open() {
        let dir = dir("lock-opening");
        let heard = Arc::new(LockWhileOpening::default());
        let options = Options { dir: dir.clone(), cache_dir: dir.join("cache"), online: false };
        let engine = Engine::new(options, heard.clone(), || Clock { now: 1_793_448_000, utc_offset: 0, hours_24: true });
        heard.engine.set(Arc::downgrade(&engine.inner)).unwrap();
        engine.set_up(PASS.into()).await.unwrap();
        engine.create_wallet("One".into()).await.unwrap();
        let address = engine.wallet().unwrap().primary_address();
        engine.lock();
        heard.phases.lock().unwrap().clear();

        heard.armed.store(true, Ordering::SeqCst);
        engine.unlock(PASS.into()).await.unwrap();
        heard.locker.lock().unwrap().take().expect("locked while opening").join().unwrap();
        assert_eq!(engine.phase(), Phase::Locked);
        assert_eq!(*heard.phases.lock().unwrap(), [Phase::Opening, Phase::Locked], "never shown open");
        assert!(engine.wallet().is_none(), "the wallet opened meanwhile was closed, not kept");
        assert!(engine.inner.open.lock().unwrap().is_none(), "nor its key");

        // Unlocking again opens it, and changes to it stick.
        engine.unlock(PASS.into()).await.unwrap();
        assert_eq!(engine.phase(), Phase::Open);
        assert_eq!(engine.wallet().unwrap().primary_address(), address);
        engine.change(|wallet| async move { wallet.set_profile("Sam".into(), None).await }).await.unwrap();
        engine.lock();
        engine.unlock(PASS.into()).await.unwrap();
        assert_eq!(engine.read(|wallet, _| async move { wallet.profile().await }).await.flatten().unwrap().name, "Sam");
    }

    #[tokio::test]
    async fn changing_the_passphrase_leaves_wallets_as_they_are() {
        let dir = dir("change");
        let (engine, _) = engine_at(&dir);
        engine.set_up(PASS.into()).await.unwrap();
        engine.create_wallet("Spending".into()).await.unwrap();
        let address = engine.wallet().unwrap().primary_address();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            engine.set_scan(ScanMode::Auto).unwrap();
            let mode = |name: &str| std::fs::metadata(dir.join(name)).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode("settings.json"), 0o600, "only this user's");
            assert_eq!(mode("vault.json"), 0o600);
        }
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

    /// Off the network, a refresh with Manual Sync on and refreshing disabled says nothing and asks no one.
    #[tokio::test]
    async fn refresh_follows_the_wallet_s_sync_mode() {
        let dir = dir("refresh");
        let (engine, _) = engine_at(&dir);
        engine.set_up(PASS.into()).await.unwrap();
        engine.create_wallet("One".into()).await.unwrap();
        engine.change(|wallet| async move { wallet.set_manual_sync(true).await }).await.unwrap();
        engine.change(|wallet| async move { wallet.set_pull_mode(fob_ffi::PullMode::Disabled).await }).await.unwrap();
        assert!(engine.sync_now().await.unwrap().is_none());
        let settings = engine.read(|wallet, _| async move { wallet.manual_settings().await }).await.unwrap();
        assert!(settings.on);
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
