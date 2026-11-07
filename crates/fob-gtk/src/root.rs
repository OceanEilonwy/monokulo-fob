//! The window. It takes what the engine publishes and gives each screen its
//! latest view, and shows the screen the engine's phase calls for.

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use fob_app::{Clock, Engine, Options, Phase, Update, form, views::{Prepared, PriceToast, Screens, SyncStatus, WalletEntry}};
use relm4::{
    Sender,
    adw::{self, prelude::*},
    gtk,
    prelude::*,
};

use crate::{
    bridge::{Bridge, Event, View},
    coins::Coins,
    confirm::{self, Confirm},
    contacts::{self, Contacts},
    home::{self, Home},
    manual::{self, Manual},
    meet::{self, Meet},
    node::{self, Node},
    passphrase::{self, Passphrase},
    payment::Payment,
    prices::{self, Prices},
    profile::{self, Profile},
    receive::{self, Receive},
    requests::{self, Request},
    scan::{self, Scan},
    send::{self, Send},
    settings::{self, Settings},
    split::{self, Split},
    start::{self, Start},
};

/// `monero:` links from the desktop. One can arrive before the window exists,
/// when it's what started Fob, so it waits here until the window asks.
static LINKS: Mutex<(Option<Sender<Msg>>, Vec<String>)> = Mutex::new((None, Vec::new()));

/// The desktop opened Fob with this code.
pub fn link_opened(code: String) {
    let mut links = LINKS.lock().expect("links lock");
    match &links.0 {
        Some(to) => to.emit(Msg::Open(code)),
        None => links.1.push(code),
    }
}

fn links_go_to(to: Sender<Msg>) {
    let mut links = LINKS.lock().expect("links lock");
    for code in links.1.drain(..) {
        to.emit(Msg::Open(code));
    }
    links.0 = Some(to);
}

pub struct Init {
    pub data: PathBuf,
    pub cache: PathBuf,
    pub online: bool,
    pub clock: Box<dyn Fn() -> Clock + std::marker::Send + Sync>,
}

/// A page pushed over Home.
#[derive(Debug)]
pub enum Go {
    Receive,
    Send,
    /// Send, filled from a code that was pasted or opened.
    SendFrom(String),
    /// Confirm, with what the form was filled with.
    Confirm(confirm::Filled),
    Payment(String),
    Contacts,
    Coins,
    Settings,
    /// Settings, scrolled to Backup and keys and with it marked.
    SettingsBackup,
    /// Adding a wallet while one is open.
    AddWallet,
    Request(u64),
    Split,
    /// Confirm, with what's ready to go already worked out.
    ConfirmReady(View<Prepared>),
    Profile,
    Meet,
    /// Someone's profile, from a link.
    Scanned(String),
    /// A share's code again.
    Share(u64),
    Node,
    Prices,
    Scan,
    /// A scan ended with its page showing.
    ScanOver,
    ManualSync,
    Back,
}

impl Go {
    /// The same place, for a row that can be opened more than once. Only rows make these.
    pub fn again(&self) -> Go {
        match self {
            Go::Payment(tx) => Go::Payment(tx.clone()),
            Go::Request(id) => Go::Request(*id),
            Go::Share(id) => Go::Share(*id),
            _ => Go::Back,
        }
    }
}

#[derive(Debug)]
pub enum Msg {
    Engine(Event),
    Go(Go),
    /// A code the desktop opened Fob with.
    Open(String),
    /// A short note at the foot of the window.
    Say(String),
    ToastDismissed,
    ToastRetry,
    /// A Trezor's question, from whatever screen is talking to one.
    Trezor(fob_app::TrezorRequest),
}

pub struct Root {
    engine: Engine,
    phase: Phase,
    passphrase: Controller<Passphrase>,
    start: Controller<Start>,
    add_wallet: Controller<Start>,
    home: Controller<Home>,
    receive: Controller<Receive>,
    send: Controller<Send>,
    confirm: Controller<Confirm>,
    payment: Controller<Payment>,
    contacts: Controller<Contacts>,
    coins: Controller<Coins>,
    settings: Controller<Settings>,
    request: Controller<Request>,
    split: Controller<Split>,
    profile: Controller<Profile>,
    meet: Controller<Meet>,
    node: Controller<Node>,
    prices: Controller<Prices>,
    scan: Controller<Scan>,
    manual: Controller<Manual>,
    toasts: adw::ToastOverlay,
    /// The Trezor's question on screen, while one is.
    trezor: crate::trezor::Dialogs,
    nav: adw::NavigationView,
    screens: Option<Arc<Screens>>,
    /// A code opened before a wallet was: taken once one is.
    pending: Option<String>,
    toast: Option<(u64, adw::Toast)>,
}

const PAGE_LOCK: &str = "lock";
const PAGE_START: &str = "start";
const PAGE_MAIN: &str = "main";

fn page(tag: &str, title: &str, child: &impl IsA<gtk::Widget>) -> adw::NavigationPage {
    adw::NavigationPage::builder().tag(tag).title(title).child(child).build()
}

#[relm4::component(pub)]
impl SimpleComponent for Root {
    type Init = Init;
    type Input = Msg;
    type Output = ();

    view! {
        adw::ApplicationWindow {
            set_title: Some("Fob"),
            set_default_size: (480, 760),
            set_width_request: 360,
            set_height_request: 480,

            #[local_ref]
            toasts -> adw::ToastOverlay {
                #[wrap(Some)]
                #[name = "stack"]
                set_child = &gtk::Stack {
                    set_transition_type: gtk::StackTransitionType::Crossfade,
                    add_named[Some(PAGE_LOCK)] = model.passphrase.widget(),
                    add_named[Some(PAGE_START)] = model.start.widget(),
                    #[local_ref]
                    add_named[Some(PAGE_MAIN)] = nav -> adw::NavigationView {},
                    #[watch]
                    set_visible_child_name: match &model.phase {
                        Phase::NoPassphrase | Phase::Locked => PAGE_LOCK,
                        Phase::NoWallet | Phase::Failed(_) => PAGE_START,
                        Phase::Opening | Phase::Open => PAGE_MAIN,
                    },
                },
            },
        }
    }

    fn init(init: Init, root: Self::Root, sender: ComponentSender<Self>) -> ComponentParts<Self> {
        let bridge = Bridge::new(sender.input_sender().clone(), Msg::Engine);
        let Init { data, cache, online, clock } = init;
        let engine = Engine::new(Options { dir: data, cache_dir: cache, online }, bridge, clock);

        let forward = |msg| msg;
        let passphrase = Passphrase::builder().launch(engine.clone()).forward(sender.input_sender(), forward);
        let start = Start::builder().launch(engine.clone()).forward(sender.input_sender(), forward);
        let add_wallet = Start::builder().launch(engine.clone()).forward(sender.input_sender(), forward);
        let home = Home::builder().launch(engine.clone()).forward(sender.input_sender(), forward);
        let receive = Receive::builder().launch(engine.clone()).forward(sender.input_sender(), forward);
        let send = Send::builder().launch(engine.clone()).forward(sender.input_sender(), forward);
        let confirm = Confirm::builder().launch(engine.clone()).forward(sender.input_sender(), forward);
        let payment = Payment::builder().launch(engine.clone()).forward(sender.input_sender(), forward);
        let contacts = Contacts::builder().launch(engine.clone()).forward(sender.input_sender(), forward);
        let coins = Coins::builder().launch(engine.clone()).forward(sender.input_sender(), forward);
        let settings = Settings::builder().launch(engine.clone()).forward(sender.input_sender(), forward);
        let request = Request::builder().launch(engine.clone()).forward(sender.input_sender(), forward);
        let split = Split::builder().launch(engine.clone()).forward(sender.input_sender(), forward);
        let profile = Profile::builder().launch(engine.clone()).forward(sender.input_sender(), forward);
        let meet = Meet::builder().launch(engine.clone()).forward(sender.input_sender(), forward);
        let node = Node::builder().launch(engine.clone()).forward(sender.input_sender(), forward);
        let prices = Prices::builder().launch(engine.clone()).forward(sender.input_sender(), forward);
        let scan = Scan::builder().launch(engine.clone()).forward(sender.input_sender(), forward);
        let manual = Manual::builder().launch(engine.clone()).forward(sender.input_sender(), forward);
        passphrase.emit(passphrase::Input::Phase(engine.phase()));

        let nav = adw::NavigationView::new();
        nav.add(&page("home", "Fob", home.widget()));
        let (left, left_adding) = (confirm.sender().clone(), add_wallet.sender().clone());
        nav.connect_popped(move |_, page| match page.tag().as_deref() {
            Some("confirm") => {
                let _ = left.send(confirm::Input::Left);
            }
            // Seed words typed and left behind don't wait in the form.
            Some("add-wallet") => {
                let _ = left_adding.send(start::Input::Left);
            }
            _ => {}
        });
        links_go_to(sender.input_sender().clone());
        let toasts = adw::ToastOverlay::new();
        let phase = engine.phase();
        let model = Root { engine, phase, passphrase, start, add_wallet, home, receive, send, confirm, payment, contacts, coins, settings, request, split, profile, meet, node, prices, scan, manual, toasts: toasts.clone(), trezor: crate::trezor::Dialogs::default(), nav: nav.clone(), screens: None, pending: None, toast: None };
        let toasts = &toasts;
        let nav = &nav;
        let widgets = view_output!();
        Self::lock_when_left(&root, &model.engine);
        crate::scene::run(&model.engine, sender.input_sender().clone());
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Msg, sender: ComponentSender<Self>) {
        match msg {
            Msg::Engine(Event(update)) => self.take(update, &sender),
            Msg::Go(go) => self.go(go),
            Msg::Open(code) if self.phase == Phase::Open => self.opened(code),
            Msg::Open(code) => self.pending = Some(code),
            Msg::Say(text) => self.toasts.add_toast(adw::Toast::new(&text)),
            Msg::Trezor(request) => {
                let window = self.nav.root().and_downcast::<adw::ApplicationWindow>();
                self.trezor.show(window.as_ref().map(|w| w.upcast_ref()), request);
            }
            Msg::ToastDismissed => {
                if let Some((id, _)) = self.toast.take() {
                    self.engine.dismiss_toast(id);
                }
            }
            Msg::ToastRetry => {
                let (engine, sender) = (self.engine.clone(), sender.clone());
                self.toast = None;
                relm4::spawn(async move {
                    if let Some(fob_app::views::PriceRetry::Updated { title, line }) = engine.retry_prices().await {
                        sender.input(Msg::Say(format!("{title} · {line}")));
                    }
                });
            }
        }
    }
}

impl Root {
    /// Locks after being left alone, and when the desktop does. What's on
    /// screen goes with it: a seed phrase left open mustn't stay.
    fn lock_when_left(window: &adw::ApplicationWindow, engine: &Engine) {
        let touches = gtk::EventControllerLegacy::new();
        touches.set_propagation_phase(gtk::PropagationPhase::Capture);
        let touched = engine.clone();
        touches.connect_event(move |_, _| {
            touched.touched();
            gtk::glib::Propagation::Proceed
        });
        window.add_controller(touches);
        let idle = engine.clone();
        gtk::glib::timeout_add_seconds_local(30, move || {
            idle.lock_if_idle();
            gtk::glib::ControlFlow::Continue
        });
        let desktop = engine.clone();
        relm4::main_application().connect_screensaver_active_notify(move |app| {
            if app.is_screensaver_active() {
                desktop.lock_with_the_desktop();
            }
        });
    }

    fn close_dialogs(&self) {
        let Some(window) = self.nav.root().and_downcast::<adw::ApplicationWindow>() else { return };
        while let Some(dialog) = window.visible_dialog() {
            dialog.force_close();
        }
    }

    /// A code from outside: someone's profile, or something to pay.
    fn opened(&mut self, code: String) {
        match form::take(self.engine.network(), &code) {
            form::Taken::Profile => self.go(Go::Scanned(code)),
            _ => self.go(Go::SendFrom(code)),
        }
    }

    /// Hands what the engine published to whoever draws it.
    fn take(&mut self, update: Update, sender: &ComponentSender<Self>) {
        match update {
            Update::Phase(phase) => {
                if matches!(phase, Phase::NoPassphrase | Phase::Locked | Phase::NoWallet) {
                    self.nav.pop_to_tag("home");
                    self.close_dialogs();
                }
                self.passphrase.emit(passphrase::Input::Phase(phase.clone()));
                self.start.emit(start::Input::Phase(phase.clone()));
                // Add a wallet too: locking pops it, and what was typed must go.
                self.add_wallet.emit(start::Input::Phase(phase.clone()));
                self.phase = phase;
                if self.phase == Phase::Open
                    && let Some(code) = self.pending.take()
                {
                    self.opened(code);
                }
            }
            Update::Screens(screens) => {
                self.screens = Some(screens.clone());
                self.home.emit(home::Input::Screens(View(screens.clone())));
                self.send.emit(send::Input::Screens(View(screens.clone())));
                self.contacts.emit(contacts::Input::Screens(View(screens.clone())));
                self.coins.emit(crate::coins::Input::Screens(View(screens.clone())));
                self.settings.emit(settings::Input::Screens(View(screens.clone())));
                self.prices.emit(prices::Input::Screens(View(screens.clone())));
                match self.nav.visible_page().and_then(|page| page.tag()).as_deref() {
                    Some("request") => self.request.emit(requests::Input::Refresh),
                    Some("split") => self.split.emit(split::Input::Refresh),
                    Some("profile") => self.profile.emit(profile::Input::Refresh),
                    _ => {}
                }
                self.receive.emit(receive::Input::Screens(View(screens)));
            }
            Update::Status(status) => self.show_status(status),
            Update::Wallets { all, current, network } => self.show_wallets(all, current, network),
            Update::Toast(toast) => self.show_toast(toast, sender),
        }
    }

    fn show_status(&self, status: Arc<SyncStatus>) {
        if let Some(words) = &status.notice {
            self.toasts.add_toast(adw::Toast::new(&format!("The chain changed · {words}")));
            if let Some(wallet) = self.engine.wallet() {
                wallet.clear_notice();
            }
        }
        self.scan.emit(scan::Input::Status(View(status.clone())));
        self.home.emit(home::Input::Status(View(status.clone())));
        self.settings.emit(settings::Input::Status(View(status)));
    }

    fn show_wallets(&self, all: Vec<WalletEntry>, current: Option<WalletEntry>, network: fob_app::views::Network) {
        self.receive.emit(receive::Input::Wallet(current.as_ref().map(|c| c.file.clone())));
        let wallets = Arc::new(settings::Wallets { all, current, network });
        self.home.emit(home::Input::Wallets(View(wallets.clone())));
        self.settings.emit(settings::Input::Wallets(View(wallets)));
    }

    /// A price lookup failing, or no longer. The same toast only takes newer words.
    fn show_toast(&mut self, toast: Option<Arc<PriceToast>>, sender: &ComponentSender<Self>) {
        let Some(view) = toast else {
            if let Some((_, shown)) = self.toast.take() {
                shown.dismiss();
            }
            return;
        };
        if let Some((id, shown)) = &self.toast
            && *id == view.id
        {
            shown.set_title(&view.line);
            return;
        }
        let shown = adw::Toast::builder().title(&view.line).button_label("Retry").timeout(0).build();
        let on_retry = sender.input_sender().clone();
        shown.connect_button_clicked(move |_| on_retry.emit(Msg::ToastRetry));
        let on_gone = sender.input_sender().clone();
        shown.connect_dismissed(move |_| on_gone.emit(Msg::ToastDismissed));
        self.toasts.add_toast(shown.clone());
        self.toast = Some((view.id, shown));
    }

    fn push(&self, tag: &str, title: &str, child: &impl IsA<gtk::Widget>) {
        if self.nav.find_page(tag).is_none() {
            self.nav.add(&page(tag, title, child));
        }
        self.nav.push_by_tag(tag);
    }

    fn go(&mut self, go: Go) {
        match go {
            Go::Receive => {
                self.receive.emit(receive::Input::Opened);
                self.push("receive", "Receive", self.receive.widget());
            }
            Go::Send => {
                self.send.emit(send::Input::Opened(None));
                self.push("send", "Send", self.send.widget());
            }
            Go::SendFrom(code) => {
                self.send.emit(send::Input::Opened(Some(code)));
                self.push("send", "Send", self.send.widget());
            }
            Go::Confirm(filled) => {
                self.confirm.emit(confirm::Input::Open(filled));
                self.push("confirm", "Confirm", self.confirm.widget());
            }
            Go::Payment(tx) => {
                self.payment.emit(crate::payment::Input::Open(tx, self.screens.clone().map(View)));
                self.push("payment", "Payment", self.payment.widget());
            }
            Go::Contacts => {
                self.contacts.emit(contacts::Input::Opened);
                self.push("contacts", "Contacts", self.contacts.widget());
            }
            Go::Coins => self.push("coins", "Coins", self.coins.widget()),
            Go::Settings => {
                self.settings.emit(settings::Input::Opened);
                self.push("settings", "Settings", self.settings.widget());
            }
            Go::SettingsBackup => {
                self.settings.emit(settings::Input::Opened);
                self.push("settings", "Settings", self.settings.widget());
                self.settings.emit(settings::Input::Highlight);
            }
            Go::AddWallet => {
                self.add_wallet.emit(start::Input::Adding);
                self.push("add-wallet", "Add a wallet", self.add_wallet.widget());
            }
            Go::Request(id) => {
                self.request.emit(requests::Input::Open(id));
                self.push("request", "Request", self.request.widget());
            }
            Go::Split => {
                self.split.emit(split::Input::Opened);
                self.push("split", "Split up", self.split.widget());
            }
            Go::ConfirmReady(prepared) => {
                self.nav.pop();
                self.confirm.emit(confirm::Input::Adopt(prepared));
                self.push("confirm", "Confirm", self.confirm.widget());
            }
            Go::Profile => {
                self.profile.emit(profile::Input::Opened);
                self.push("profile", "Your profile", self.profile.widget());
            }
            Go::Meet => {
                self.meet.emit(meet::Input::Opened);
                self.push("meet", "Meet", self.meet.widget());
            }
            Go::Scanned(text) => self.meet.emit(meet::Input::Scanned(text)),
            Go::Share(id) => {
                self.profile.emit(profile::Input::Opened);
                self.profile.emit(profile::Input::ShowShare(id));
                self.push("profile", "Your profile", self.profile.widget());
            }
            Go::Node => {
                self.node.emit(node::Input::Opened);
                self.push("node", "Node", self.node.widget());
            }
            Go::Prices => {
                self.prices.emit(prices::Input::Opened);
                self.push("prices", "Prices", self.prices.widget());
            }
            Go::Scan => {
                self.scan.emit(scan::Input::Opened);
                self.push("scan", "Scan", self.scan.widget());
            }
            Go::ScanOver => {
                if self.nav.visible_page().and_then(|page| page.tag()).as_deref() == Some("scan") {
                    self.nav.pop();
                }
            }
            Go::ManualSync => {
                self.manual.emit(manual::Input::Opened);
                self.push("manual", "Manual Sync", self.manual.widget());
            }
            Go::Back => {
                self.nav.pop();
            }
        }
    }
}
