//! The window. It takes what the engine publishes and gives each screen its
//! latest view, and shows the screen the engine's phase calls for.

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use fob_app::{Clock, Engine, Options, Phase, Update, views::{PriceToast, Screens, SyncStatus, WalletEntry}};
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
    passphrase::{self, Passphrase},
    payment::Payment,
    receive::{self, Receive},
    send::{self, Send},
    settings::{self, Settings},
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
    /// Adding a wallet while one is open.
    AddWallet,
    Back,
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
    toasts: adw::ToastOverlay,
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
        passphrase.emit(passphrase::Input::Phase(engine.phase()));

        let nav = adw::NavigationView::new();
        nav.add(&page("home", "Fob", home.widget()));
        let left = confirm.sender().clone();
        nav.connect_popped(move |_, page| {
            if page.tag().as_deref() == Some("confirm") {
                let _ = left.send(confirm::Input::Left);
            }
        });
        links_go_to(sender.input_sender().clone());
        let toasts = adw::ToastOverlay::new();
        let phase = engine.phase();
        let model = Root { engine, phase, passphrase, start, add_wallet, home, receive, send, confirm, payment, contacts, coins, settings, toasts: toasts.clone(), nav: nav.clone(), screens: None, pending: None, toast: None };
        let toasts = &toasts;
        let nav = &nav;
        let widgets = view_output!();
        crate::scene::run(&model.engine, sender.input_sender().clone());
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Msg, sender: ComponentSender<Self>) {
        match msg {
            Msg::Engine(Event(update)) => self.take(update, &sender),
            Msg::Go(go) => self.go(go),
            Msg::Open(code) if self.phase == Phase::Open => self.go(Go::SendFrom(code)),
            Msg::Open(code) => self.pending = Some(code),
            Msg::Say(text) => self.toasts.add_toast(adw::Toast::new(&text)),
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
    /// Hands what the engine published to whoever draws it.
    fn take(&mut self, update: Update, sender: &ComponentSender<Self>) {
        match update {
            Update::Phase(phase) => {
                if matches!(phase, Phase::NoPassphrase | Phase::Locked | Phase::NoWallet) {
                    self.nav.pop_to_tag("home");
                }
                self.passphrase.emit(passphrase::Input::Phase(phase.clone()));
                self.start.emit(start::Input::Phase(phase.clone()));
                self.phase = phase;
                if self.phase == Phase::Open
                    && let Some(code) = self.pending.take()
                {
                    self.go(Go::SendFrom(code));
                }
            }
            Update::Screens(screens) => {
                self.screens = Some(screens.clone());
                self.home.emit(home::Input::Screens(View(screens.clone())));
                self.send.emit(send::Input::Screens(View(screens.clone())));
                self.contacts.emit(contacts::Input::Screens(View(screens.clone())));
                self.coins.emit(crate::coins::Input::Screens(View(screens.clone())));
                self.settings.emit(settings::Input::Screens(View(screens.clone())));
                self.receive.emit(receive::Input::Screens(View(screens)));
            }
            Update::Status(status) => self.show_status(status),
            Update::Wallets { all, current, network } => self.show_wallets(all, current, network),
            Update::Toast(toast) => self.show_toast(toast, sender),
        }
    }

    fn show_status(&self, status: Arc<SyncStatus>) {
        self.home.emit(home::Input::Status(View(status.clone())));
        self.settings.emit(settings::Input::Status(View(status)));
    }

    fn show_wallets(&self, all: Vec<WalletEntry>, current: Option<WalletEntry>, network: fob_app::views::Network) {
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
                self.payment.emit(crate::payment::Input::Open(tx));
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
            Go::AddWallet => {
                self.add_wallet.emit(start::Input::Adding);
                self.push("add-wallet", "Add a wallet", self.add_wallet.widget());
            }
            Go::Back => {
                self.nav.pop();
            }
        }
    }
}
