//! Settings: wallets and networks, the node, prices, scanning, backup and the
//! passphrase. Seeing keys or the wallet code asks for the passphrase first.

use std::sync::Arc;

use fob_app::{
    Confirmed, Engine, FobError,
    views::{BackupHub, HubItem, KeyGroup, Network, Screens, SeedPhrase, SyncStatus, WalletEntry, ZmqStatus},
};
use relm4::{
    adw::{self, prelude::*},
    gtk,
    prelude::*,
};

use crate::{
    bridge::View,
    root::{Go, Msg as RootMsg},
    widgets,
};

/// Every wallet, and the one open.
pub struct Wallets {
    pub all: Vec<WalletEntry>,
    pub current: Option<WalletEntry>,
}

#[derive(Debug)]
pub enum Input {
    Screens(View<Screens>),
    Status(View<SyncStatus>),
    Wallets(View<Wallets>),
    Opened,
    /// Scroll to Backup and keys and mark it for a moment.
    Highlight,
    Hub(View<BackupHub>),
    Switch(String),
    AddWallet,
    Rename(String, String),
    Forget,
    Zmq,
    /// Whether syncing goes through a light wallet server, and its row.
    Light(bool, Option<String>),
    /// "Sync through" chosen: 0 the node, 1 a light wallet server.
    SyncThrough(u32),
    LightServer,
    LockAfter(u32),
    ProfileLine(String),
    GpuFailed(bool),
    Lock,
    ChangePassphrase,
    /// Asks for the passphrase, then shows `HubItem`.
    Reveal(HubItem),
    Seed(View<SeedPhrase>),
    Keys(View<Vec<KeyGroup>>),
    WalletCode(String),
    BackedUp,
    Say(String),
}

pub struct Settings {
    engine: Engine,
    wallets_group: adw::PreferencesGroup,
    wallet_rows: Vec<gtk::Widget>,
    backup_group: adw::PreferencesGroup,
    backup_rows: Vec<adw::ActionRow>,
    node_line: String,
    backup_line: String,
    hub_key: String,
    currency_line: String,
    zmq_line: String,
    zmq_on: bool,
    manual_on: bool,
    /// Syncing through a light wallet server.
    light_on: bool,
    /// The Light wallet server row: "lws.example:8443 · synced".
    light_line: Option<String>,
    sync_through: adw::ComboRow,
    /// The Node card's and the Light server screen's words.
    words: fob_app::views::LightWords,
    zmq_problem: Option<String>,
    profile_line: String,
    /// The Scanning row: what's scanning, read-only.
    scan_line: String,
    /// Whether the open wallet's GPU check failed.
    gpu_refused: bool,
}

/// How long untouched before locking, as the row lists them.
const LOCK_MINUTES: [u32; 5] = [5, 15, 30, 60, 0];

#[relm4::component(pub)]
impl SimpleComponent for Settings {
    type Init = Engine;
    type Input = Input;
    type Output = RootMsg;

    view! {
        adw::ToolbarView {
            add_top_bar = &adw::HeaderBar {},
            #[wrap(Some)]
            set_content = &gtk::ScrolledWindow {
                set_hscrollbar_policy: gtk::PolicyType::Never,
                #[wrap(Some)]
                set_child = &adw::Clamp {
                    set_maximum_size: 600,
                    #[wrap(Some)]
                    set_child = &gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 18,
                        set_margin_all: 16,

                        #[local_ref]
                        wallets_group -> adw::PreferencesGroup {
                            set_title: "Wallets",
                            #[wrap(Some)]
                            set_header_suffix = &gtk::Button {
                                add_css_class: "flat",
                                set_child: Some(&widgets::icon_label(crate::icons::PLUS, "Add", false)),
                                connect_clicked => Input::AddWallet,
                            },
                        },
                        adw::PreferencesGroup {
                            add = &adw::ActionRow {
                                set_use_markup: false,
                                set_title: "Your profile",
                                set_activatable: true,
                                #[watch]
                                set_subtitle: &model.profile_line,
                                add_suffix = &gtk::Image { set_icon_name: Some("go-next-symbolic") },
                                connect_activated[sender] => move |_| { let _ = sender.output(RootMsg::Go(Go::Profile)); },
                            },
                        },
                        #[local_ref]
                        backup_group -> adw::PreferencesGroup {
                            set_title: "Backup and keys",
                            #[watch]
                            set_description: Some(gtk::glib::markup_escape_text(&model.backup_line).as_str()),
                        },
                        adw::PreferencesGroup {
                            set_title: "Connection",
                            #[local_ref]
                            add = sync_through -> adw::ComboRow {
                                set_title: &model.words.sync_through,
                                set_model: Some(&gtk::StringList::new(&[model.words.through_node.as_str(), model.words.through_server.as_str()])),
                                #[watch]
                                set_selected: u32::from(model.light_on),
                                connect_selected_notify[sender] => move |row| sender.input(Input::SyncThrough(row.selected())),
                            },
                            add = &adw::ActionRow {
                                set_use_markup: false,
                                set_title: "Node",
                                // Only the rows for the way syncing goes: the node's kept for switching back.
                                #[watch]
                                set_visible: !model.light_on,
                                set_activatable: true,
                                #[watch]
                                set_subtitle: &model.node_line,
                                add_suffix = &gtk::Image { set_icon_name: Some("go-next-symbolic") },
                                connect_activated[sender] => move |_| { let _ = sender.output(RootMsg::Go(Go::Node)); },
                            },
                            add = &adw::ActionRow {
                                set_use_markup: false,
                                set_title: &model.words.server_title,
                                #[watch]
                                set_visible: model.light_on,
                                set_activatable: true,
                                #[watch]
                                set_subtitle: model.light_line.as_deref().unwrap_or(model.words.none_chosen.as_str()),
                                add_suffix = &gtk::Image { set_icon_name: Some("go-next-symbolic") },
                                connect_activated => Input::LightServer,
                            },
                            add = &adw::ActionRow {
                                set_use_markup: false,
                                set_title: "Instant updates (ZMQ)",
                                // Manual Sync listens for nothing, and the server watches the chain itself.
                                #[watch]
                                set_visible: !model.manual_on && !model.light_on,
                                set_activatable: true,
                                connect_activated => Input::Zmq,
                                #[watch]
                                set_subtitle: &model.zmq_line,
                            },
                            add = &adw::ActionRow {
                                set_use_markup: false,
                                set_title: "Rescan",
                                set_subtitle: "Look through past blocks again",
                                // The server scans: nothing here to scan again.
                                #[watch]
                                set_visible: !model.light_on,
                                set_activatable: true,
                                add_suffix = &gtk::Image { set_icon_name: Some("go-next-symbolic") },
                                connect_activated[sender] => move |_| { let _ = sender.output(RootMsg::Go(Go::Scan)); },
                            },
                            add = &adw::ActionRow {
                                set_use_markup: false,
                                // Moot through a server, which scans regardless.
                                #[watch]
                                set_visible: !model.light_on,
                                set_title: "Manual Sync",
                                set_activatable: true,
                                #[watch]
                                set_subtitle: if model.manual_on { "On" } else { "Off" },
                                add_suffix = &gtk::Image { set_icon_name: Some("go-next-symbolic") },
                                connect_activated[sender] => move |_| { let _ = sender.output(RootMsg::Go(Go::ManualSync)); },
                            },
                            // Nothing to choose: the GPU scans when it can.
                            add = &adw::ActionRow {
                                set_use_markup: false,
                                set_title: "Scanning",
                                #[watch]
                                set_subtitle: &model.scan_line,
                            },
                        },
                        adw::PreferencesGroup {
                            set_title: "Prices",
                            add = &adw::ActionRow {
                                set_use_markup: false,
                                set_title: "Prices and currency",
                                set_activatable: true,
                                add_suffix = &gtk::Image { set_icon_name: Some("go-next-symbolic") },
                                #[watch]
                                set_subtitle: &model.currency_line,
                                connect_activated[sender] => move |_| { let _ = sender.output(RootMsg::Go(Go::Prices)); },
                            },
                        },
                        adw::PreferencesGroup {
                            set_title: "This computer",
                            add = &adw::ComboRow {
                                set_title: "Lock when left alone",
                                set_model: Some(&gtk::StringList::new(&["After 5 minutes", "After 15 minutes", "After 30 minutes", "After an hour", "Never"])),
                                set_selected: LOCK_MINUTES.iter().position(|m| *m == model.engine.lock_after()).unwrap_or(1) as u32,
                                connect_selected_notify[sender] => move |row| sender.input(Input::LockAfter(row.selected())),
                            },
                            add = &adw::ButtonRow { set_title: "Change passphrase", connect_activated => Input::ChangePassphrase },
                            add = &adw::ButtonRow { set_title: "Lock now", connect_activated => Input::Lock },
                            add = &adw::ButtonRow {
                                set_title: "Remove this wallet from this computer",
                                add_css_class: "destructive-action",
                                connect_activated => Input::Forget,
                            },
                        },
                    },
                },
            },
        }
    }

    fn init(engine: Engine, root: Self::Root, sender: ComponentSender<Self>) -> ComponentParts<Self> {
        let (wallets_group, backup_group) = (adw::PreferencesGroup::new(), adw::PreferencesGroup::new());
        let sync_through = adw::ComboRow::new();
        let scan_line = engine.scan_engine(false).line;
        let model = Settings {
            engine,
            wallets_group: wallets_group.clone(),
            wallet_rows: vec![],
            backup_group: backup_group.clone(),
            backup_rows: vec![],
            node_line: String::new(),
            backup_line: String::new(),
            hub_key: String::new(),
            currency_line: String::new(),
            zmq_line: String::new(),
            zmq_on: false,
            manual_on: false,
            light_on: false,
            light_line: None,
            sync_through: sync_through.clone(),
            words: fob_app::views::light_words(),
            zmq_problem: None,
            profile_line: String::new(),
            scan_line,
            gpu_refused: false,
        };
        let (wallets_group, backup_group, sync_through) = (&wallets_group, &backup_group, &sync_through);
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, input: Input, sender: ComponentSender<Self>) {
        match input {
            Input::Screens(View(screens)) => {
                self.node_line = screens.node_line.clone();
                self.currency_line = screens.prices.currency_text.clone();
                // Backup and keys is drawn whole before Settings opens, not on opening.
                self.fetch_hub(&sender);
            }
            Input::Status(View(status)) => {
                self.zmq_line = status.zmq_text.clone();
                self.zmq_on = !matches!(status.zmq, ZmqStatus::Off);
                self.manual_on = status.manual_sync;
                self.zmq_problem = match &status.zmq {
                    ZmqStatus::CantConnect { reason } => Some(reason.clone()),
                    _ => None,
                };
                // The GPU may have finished starting, or failed, since.
                self.scan_line = self.engine.scan_engine(self.gpu_refused).line;
                let (engine, sender) = (self.engine.clone(), sender.clone());
                relm4::spawn(async move {
                    if let Some((on, line)) = engine.read(|wallet, _| async move { (wallet.light_mode().await, wallet.light_line().await) }).await {
                        sender.input(Input::Light(on, line));
                    }
                });
            }
            Input::Light(on, line) => {
                self.light_on = on;
                self.light_line = line;
            }
            Input::SyncThrough(i) => {
                let through_server = i == 1;
                // The row following the wallet, not someone choosing.
                if through_server == self.light_on {
                    return;
                }
                // Put back until the switch is confirmed.
                self.sync_through.set_selected(u32::from(self.light_on));
                if through_server && self.light_line.is_none() {
                    self.ask_light_server(&sender);
                } else {
                    self.confirm_switch(through_server, &sender);
                }
            }
            Input::LightServer => self.ask_light_server(&sender),
            Input::Wallets(View(wallets)) => self.draw_wallets(&wallets, &sender),
            Input::Opened => {
                self.fetch_hub(&sender);
                let (engine, sender) = (self.engine.clone(), sender.clone());
                relm4::spawn(async move {
                    if let Some(row) = engine.read(|wallet, _| async move { wallet.profile_row().await }).await {
                        sender.input(Input::ProfileLine(row.name.unwrap_or(row.title)));
                    }
                    if let Some(failed) = engine.read(|wallet, _| async move { wallet.scan_gpu_check_failed().await }).await {
                        sender.input(Input::GpuFailed(failed));
                    }
                });
            }
            Input::Highlight => {
                // The scrolled window brings its focused child into view, with
                // the layout as it really is, so focus the first row once the
                // page is showing and mark the group for a moment.
                let group = self.backup_group.clone();
                let first = self.backup_rows.first().cloned();
                let mark = move || {
                    if let Some(row) = &first {
                        row.grab_focus();
                    }
                    group.add_css_class("marked");
                    let group = group.clone();
                    gtk::glib::timeout_add_local_once(std::time::Duration::from_millis(2500), move || group.remove_css_class("marked"));
                };
                // Already on screen (this runs after the page was pushed), or
                // not yet: then when it is.
                if self.backup_group.is_mapped() {
                    mark();
                } else {
                    let mark = std::cell::RefCell::new(Some(mark));
                    self.backup_group.connect_map(move |_| {
                        if let Some(mark) = mark.take() {
                            mark();
                        }
                    });
                }
            }
            Input::Hub(View(hub)) => self.draw_hub(&hub, &sender),
            Input::Switch(file) => {
                let (engine, sender) = (self.engine.clone(), sender.clone());
                relm4::spawn(async move {
                    if let Err(e) = engine.switch_wallet(file).await {
                        sender.input(Input::Say(e.to_string()));
                    }
                });
            }
            Input::AddWallet => {
                let _ = sender.output(RootMsg::Go(Go::AddWallet));
            }
            Input::Rename(file, name) => {
                if let Err(e) = self.engine.rename_wallet(file, name) {
                    sender.input(Input::Say(e.to_string()));
                }
            }
            Input::Forget => self.ask_forget(&sender),
            Input::Zmq => self.ask_zmq(&sender),
            Input::LockAfter(i) => {
                if let Err(e) = self.engine.set_lock_after(LOCK_MINUTES.get(i as usize).copied().unwrap_or(15)) {
                    sender.input(Input::Say(e.to_string()));
                }
            }
            Input::ProfileLine(line) => self.profile_line = line,
            Input::GpuFailed(failed) => {
                self.gpu_refused = failed;
                self.scan_line = self.engine.scan_engine(failed).line;
            }
            Input::Lock => self.engine.lock(),
            Input::ChangePassphrase => self.ask_new_passphrase(&sender),
            Input::Reveal(item) => {
                let (engine, then) = (self.engine.clone(), sender.clone());
                widgets::ask_passphrase(self.backup_group.upcast_ref(), "Show these?", "Anyone who sees them can use this wallet. Type your passphrase to go on.", move |typed| {
                    let Some(typed) = typed else { return };
                    let (engine, then) = (engine.clone(), then.clone());
                    relm4::spawn(async move {
                        match reveal(&engine, item, engine.confirm(String::clone(&typed)).await).await {
                            Ok(Some(shown)) => then.input(shown),
                            Ok(None) => {}
                            Err(e) => then.input(Input::Say(e.to_string())),
                        }
                    });
                });
            }
            Input::Seed(View(seed)) => self.show_seed(&seed, &sender),
            Input::Keys(View(groups)) => self.show_keys(&groups),
            Input::WalletCode(code) => self.show_wallet_code(&code),
            Input::BackedUp => {
                let engine = self.engine.clone();
                let then = sender.clone();
                relm4::spawn(async move {
                    let _ = engine.change(|wallet| async move { wallet.backup_done().await }).await;
                    then.input(Input::Opened);
                });
            }
            Input::Say(text) => {
                let _ = sender.output(RootMsg::Say(text));
            }
        }
    }
}

impl Settings {
    fn draw_wallets(&mut self, wallets: &Wallets, sender: &ComponentSender<Self>) {
        for row in self.wallet_rows.drain(..) {
            self.wallets_group.remove(&row);
        }
        for wallet in &wallets.all {
            let network = match wallet.network {
                Network::Mainnet => "Mainnet",
                Network::Stagenet => "Stagenet, test coins",
            };
            let open = wallets.current.as_ref().is_some_and(|c| c.file == wallet.file);
            let row = widgets::row(&wallet.name, network);
            if open {
                row.add_suffix(&gtk::Image::from_icon_name("object-select-symbolic"));
            } else {
                row.set_activatable(true);
                let (file, sender) = (wallet.file.clone(), sender.clone());
                row.connect_activated(move |_| sender.input(Input::Switch(file.clone())));
            }
            let rename = gtk::Button::builder().icon_name("document-edit-symbolic").valign(gtk::Align::Center).tooltip_text("Rename").css_classes(["flat"]).build();
            let (file, name, then, parent) = (wallet.file.clone(), wallet.name.clone(), sender.clone(), self.wallets_group.clone());
            rename.connect_clicked(move |_| {
                let entry = adw::EntryRow::builder().title("Name").text(&name).build();
                let holder = widgets::list();
                holder.append(&entry);
                let dialog = adw::AlertDialog::builder().heading("Rename wallet").extra_child(&holder).build();
                dialog.add_responses(&[("cancel", "_Cancel"), ("save", "_Save")]);
                dialog.set_close_response("cancel");
                dialog.set_response_appearance("save", adw::ResponseAppearance::Suggested);
                let (file, then) = (file.clone(), then.clone());
                dialog.connect_response(Some("save"), move |_, _| then.input(Input::Rename(file.clone(), entry.text().to_string())));
                dialog.present(Some(&parent));
            });
            row.add_suffix(&rename);
            self.wallets_group.add(&row);
            self.wallet_rows.push(row.upcast());
        }
    }

    fn fetch_hub(&self, sender: &ComponentSender<Self>) {
        let (engine, sender) = (self.engine.clone(), sender.clone());
        relm4::spawn(async move {
            if let Some(hub) = engine.read(|wallet, _| async move { wallet.backup_hub(false).await }).await {
                sender.input(Input::Hub(View(hub.into())));
            }
        });
    }

    fn draw_hub(&mut self, hub: &BackupHub, sender: &ComponentSender<Self>) {
        // The same hub again changes nothing, so nothing is redrawn.
        let key = format!("{}\n{}", hub.warning, hub.backup.iter().chain(&hub.other).map(|i| format!("{}|{}", i.title, i.line)).collect::<Vec<_>>().join("\n"));
        if key == self.hub_key {
            return;
        }
        self.hub_key = key;
        for row in self.backup_rows.drain(..) {
            self.backup_group.remove(&row);
        }
        self.backup_line = hub.warning.clone();
        for item in hub.backup.iter().chain(&hub.other) {
            let row = widgets::row(&item.title, &item.line);
            row.set_activatable(true);
            row.add_suffix(&gtk::Image::from_icon_name("go-next-symbolic"));
            let (what, sender) = (item.item, sender.clone());
            row.connect_activated(move |_| sender.input(Input::Reveal(what)));
            self.backup_group.add(&row);
            self.backup_rows.push(row);
        }
    }

    fn show_seed(&self, seed: &SeedPhrase, sender: &ComponentSender<Self>) {
        let grid = gtk::FlowBox::builder().selection_mode(gtk::SelectionMode::None).min_children_per_line(3).max_children_per_line(4).homogeneous(true).build();
        let phrase = String::from_utf8_lossy(&seed.phrase);
        let words: Vec<&str> = phrase.split(' ').collect();
        for (i, word) in words.iter().enumerate() {
            let label = gtk::Label::builder().label(format!("{}. {word}", i + 1)).xalign(0.0).build();
            label.add_css_class("seed-word");
            grid.insert(&label, -1);
        }
        let dialog = adw::AlertDialog::builder().heading(&seed.kind).body(fob_app::words::errors::seed_warning(words.len())).extra_child(&grid).prefer_wide_layout(true).build();
        dialog.add_responses(&[("close", "_Close"), ("done", "I've _written them down")]);
        dialog.set_close_response("close");
        dialog.set_response_appearance("done", adw::ResponseAppearance::Suggested);
        let sender = sender.clone();
        dialog.connect_response(Some("done"), move |_, _| sender.input(Input::BackedUp));
        dialog.present(Some(&self.backup_group));
    }

    fn show_keys(&self, groups: &[KeyGroup]) {
        let column = gtk::Box::new(gtk::Orientation::Vertical, 12);
        for group in groups {
            let list = adw::PreferencesGroup::builder().title(gtk::glib::markup_escape_text(&group.title).as_str()).build();
            for key in &group.keys {
                let row = widgets::row(&key.title, &String::from_utf8_lossy(&key.value));
                // Not selectable: Ctrl+C on selected text would put the key on
                // the clipboard with no secret hint and no clearing. Copy does both.
                row.set_subtitle_selectable(false);
                row.add_css_class("property");
                let copy = gtk::Button::builder().icon_name("edit-copy-symbolic").valign(gtk::Align::Center).css_classes(["flat"]).tooltip_text(&key.copy).build();
                let clip = String::from_utf8_lossy(&key.clip).into_owned();
                copy.connect_clicked(move |button| widgets::copy_secret(button, &clip));
                row.add_suffix(&copy);
                list.add(&row);
            }
            column.append(&list);
        }
        let scroll = gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).min_content_height(420).child(&column).build();
        let dialog = adw::AlertDialog::builder().heading("Keys").extra_child(&scroll).prefer_wide_layout(true).build();
        dialog.add_response("close", "_Close");
        dialog.present(Some(&self.backup_group));
    }

    fn show_wallet_code(&self, code: &str) {
        let column = gtk::Box::new(gtk::Orientation::Vertical, 12);
        if let Some(qr) = widgets::qr(code, 300) {
            column.append(&qr);
        }
        let dialog = adw::AlertDialog::builder().heading("Wallet code").body(fob_app::words::errors::wallet_code_warning()).extra_child(&column).build();
        dialog.add_response("close", "_Close");
        dialog.present(Some(&self.backup_group));
    }

    fn ask_forget(&self, sender: &ComponentSender<Self>) {
        let dialog = adw::AlertDialog::builder().heading("Remove this wallet from this computer?").body("Its money stays on the chain, but only its seed phrase or keys can bring it back. Make sure you've written them down.").build();
        dialog.add_responses(&[("cancel", "_Cancel"), ("remove", "_Remove")]);
        dialog.set_close_response("cancel");
        dialog.set_response_appearance("remove", adw::ResponseAppearance::Destructive);
        // Removing is guarded like showing keys: a wallet never backed up is
        // gone with its file, and a desktop is often left unlocked. The engine
        // checks the passphrase itself, so this dialog is only where it's typed.
        let (engine, sender, parent) = (self.engine.clone(), sender.clone(), self.backup_group.clone());
        dialog.connect_response(Some("remove"), move |_, _| {
            let (engine, sender) = (engine.clone(), sender.clone());
            widgets::ask_passphrase(parent.upcast_ref(), "Remove this wallet?", "Type your passphrase to remove it from this computer.", move |typed| {
                let Some(typed) = typed else { return };
                let (engine, sender) = (engine.clone(), sender.clone());
                relm4::spawn(async move {
                    let forgot = match engine.confirm(String::clone(&typed)).await {
                        Ok(confirmed) => engine.forget_wallet(confirmed).await,
                        Err(e) => Err(e),
                    };
                    match forgot {
                        Ok(()) => {
                            let _ = sender.output(RootMsg::Go(Go::Back));
                        }
                        Err(e) => sender.input(Input::Say(e.to_string())),
                    }
                });
            });
        });
        dialog.present(Some(&self.backup_group));
    }

    fn ask_zmq(&self, sender: &ComponentSender<Self>) {
        let (on, endpoint) = (adw::SwitchRow::builder().title("Listen for new blocks").active(self.zmq_on).build(), adw::EntryRow::builder().title("ZMQ address").build());
        let form = widgets::list();
        form.append(&on);
        form.append(&endpoint);
        let hint = format!("Your node's --zmq-pub, on the node's own host: tcp://host:port{}", self.zmq_problem.as_ref().map_or(String::new(), |why| format!("\n\n{why}")));
        let dialog = adw::AlertDialog::builder().heading("Instant updates").body(&hint).extra_child(&form).build();
        dialog.add_responses(&[("cancel", "_Cancel"), ("save", "_Save")]);
        dialog.set_close_response("cancel");
        dialog.set_response_appearance("save", adw::ResponseAppearance::Suggested);
        let (engine, shown) = (self.engine.clone(), endpoint.clone());
        gtk::glib::spawn_future_local(async move {
            if let Ok(Some(now)) = relm4::spawn(async move { engine.read(|wallet, _| async move { wallet.zmq_endpoint().await }).await }).await {
                shown.set_text(&now);
            }
        });
        let (engine, sender) = (self.engine.clone(), sender.clone());
        dialog.connect_response(Some("save"), move |_, _| {
            let (engine, sender, on, typed) = (engine.clone(), sender.clone(), on.is_active(), endpoint.text().trim().to_owned());
            relm4::spawn(async move {
                let endpoint = Some(typed).filter(|t| !t.is_empty());
                if let Err(e) = engine.change(move |wallet| async move { wallet.set_zmq(on, endpoint).await }).await {
                    sender.input(Input::Say(e.to_string()));
                }
            });
        });
        dialog.present(Some(&self.backup_group));
    }

    /// Asks before syncing through the server (its privacy words) or
    /// through the node again (where scanning picks up).
    fn confirm_switch(&self, on: bool, sender: &ComponentSender<Self>) {
        let (heading, body, yes) = if on {
            (self.words.server_title.as_str(), self.words.privacy.as_str(), self.words.use_server.as_str())
        } else {
            (self.words.through_node.as_str(), self.words.turning_off.as_str(), self.words.through_node.as_str())
        };
        let dialog = adw::AlertDialog::builder().heading(heading).body(body).build();
        dialog.add_responses(&[("cancel", "_Cancel"), ("switch", yes)]);
        dialog.set_close_response("cancel");
        dialog.set_response_appearance("switch", adw::ResponseAppearance::Suggested);
        let (engine, sender) = (self.engine.clone(), sender.clone());
        dialog.connect_response(Some("switch"), move |_, _| {
            let (engine, sender) = (engine.clone(), sender.clone());
            relm4::spawn(async move {
                if let Err(e) = engine.change(move |wallet| async move { wallet.use_light_wallet(on).await }).await {
                    sender.input(Input::Say(e.to_string()));
                }
                if let Some((on, line)) = engine.read(|wallet, _| async move { (wallet.light_mode().await, wallet.light_line().await) }).await {
                    sender.input(Input::Light(on, line));
                }
            });
        });
        dialog.present(Some(&self.backup_group));
    }

    /// The Light server screen, as a dialog: the address, the certificate
    /// switch, Test with what it found, the privacy words, and "Sync through
    /// this server", which Rust refuses with words when the server won't do.
    fn ask_light_server(&self, sender: &ComponentSender<Self>) {
        let url = adw::EntryRow::builder().title(&self.words.url_label).build();
        let self_signed = adw::SwitchRow::builder().title(&self.words.self_signed).build();
        let test = adw::ButtonRow::builder().title(&self.words.test).build();
        let found = widgets::dim(&self.words.not_tested);
        found.set_wrap(true);
        let form = widgets::list();
        form.append(&url);
        form.append(&self_signed);
        form.append(&test);
        // The server in use as it stands: the key it's held to, whether its
        // amounts are checked, its last failure.
        let standing = widgets::dim("");
        standing.set_wrap(true);
        standing.set_selectable(true);
        standing.set_visible(false);
        let column = gtk::Box::new(gtk::Orientation::Vertical, 12);
        column.append(&form);
        column.append(&standing);
        column.append(&found);
        let dialog = adw::AlertDialog::builder().heading(&self.words.server_title).body(&self.words.privacy).extra_child(&column).build();
        dialog.add_responses(&[("cancel", "_Cancel"), ("use", self.words.use_server.as_str())]);
        dialog.set_close_response("cancel");
        dialog.set_response_appearance("use", adw::ResponseAppearance::Suggested);
        let (engine, shown, switch, stands) = (self.engine.clone(), url.clone(), self_signed.clone(), standing.clone());
        gtk::glib::spawn_future_local(async move {
            if let Ok(Some((Some(now), said))) = relm4::spawn(async move { engine.read(|wallet, _| async move { (wallet.light_server().await, wallet.light_standing().await) }).await }).await {
                shown.set_text(&now.url);
                switch.set_active(now.self_signed);
                let mut lines: Vec<String> = said.rows.iter().map(|r| format!("{}: {}{}", r.label, r.value, if r.bad { " ✗" } else { "" })).collect();
                lines.extend(said.notes);
                stands.set_text(&lines.join("\n"));
                stands.set_visible(!said.rows.is_empty());
            }
        });
        let (engine, typed, ss, label) = (self.engine.clone(), url.clone(), self_signed.clone(), found.clone());
        test.connect_activated(move |_| {
            let (engine, text, ss, label) = (engine.clone(), typed.text().trim().to_owned(), ss.is_active(), label.clone());
            gtk::glib::spawn_future_local(async move {
                let report = relm4::spawn(async move { engine.read(move |wallet, _| async move { wallet.check_light_server(text, ss).await }).await }).await;
                if let Ok(Some(report)) = report {
                    let mut lines = vec![format!("{} · {}", report.address, report.verdict)];
                    lines.extend(report.rows.iter().map(|r| format!("{}: {}{}", r.label, r.value, if r.bad { " ✗" } else { "" })));
                    label.set_text(&lines.join("\n"));
                }
            });
        });
        let (engine, sender) = (self.engine.clone(), sender.clone());
        dialog.connect_response(Some("use"), move |_, _| {
            let (engine, sender, text, ss) = (engine.clone(), sender.clone(), url.text().trim().to_owned(), self_signed.is_active());
            relm4::spawn(async move {
                let chosen = engine.change(move |wallet| async move {
                    wallet.set_light_server(text, ss).await?;
                    wallet.use_light_wallet(true).await
                });
                if let Err(e) = chosen.await {
                    sender.input(Input::Say(e.to_string()));
                }
                if let Some((on, line)) = engine.read(|wallet, _| async move { (wallet.light_mode().await, wallet.light_line().await) }).await {
                    sender.input(Input::Light(on, line));
                }
            });
        });
        dialog.present(Some(&self.backup_group));
    }

    fn ask_new_passphrase(&self, sender: &ComponentSender<Self>) {
        let (old, new, again) = (adw::PasswordEntryRow::builder().title("Passphrase now").build(), adw::PasswordEntryRow::builder().title("New passphrase").build(), adw::PasswordEntryRow::builder().title("New passphrase again").build());
        let form = widgets::list();
        form.append(&old);
        form.append(&new);
        form.append(&again);
        let dialog = adw::AlertDialog::builder().heading("Change passphrase").extra_child(&form).build();
        dialog.add_responses(&[("cancel", "_Cancel"), ("change", "_Change")]);
        dialog.set_close_response("cancel");
        dialog.set_response_appearance("change", adw::ResponseAppearance::Suggested);
        let (engine, sender) = (self.engine.clone(), sender.clone());
        dialog.connect_response(Some("change"), move |_, _| {
            if new.text() != again.text() {
                sender.input(Input::Say(fob_app::words::errors::passphrases_differ()));
                return;
            }
            let (engine, sender, typed_old, typed_new) = (engine.clone(), sender.clone(), widgets::secret_text(old.text()), widgets::secret_text(new.text()));
            for entry in [&old, &new, &again] {
                entry.set_text("");
            }
            relm4::spawn(async move {
                match engine.change_passphrase(String::clone(&typed_old), String::clone(&typed_new)).await {
                    Ok(()) => sender.input(Input::Say(fob_app::words::errors::passphrase_changed())),
                    Err(e) => sender.input(Input::Say(e.to_string())),
                }
            });
        });
        dialog.present(Some(&self.backup_group));
    }
}

/// Reads `item` with the passphrase just typed, and returns what shows it.
/// The engine's proof goes straight into the read it was asked for, so it
/// never sits in a message where something else could use it.
async fn reveal(engine: &Engine, item: HubItem, confirmed: Result<Confirmed, FobError>) -> Result<Option<Input>, FobError> {
    let confirmed = confirmed?;
    Ok(match item {
        HubItem::SeedPhrase => engine.seed_phrase(confirmed)?.map(|seed| Input::Seed(View(Arc::new(seed)))),
        HubItem::WalletCode => Some(Input::WalletCode(String::from_utf8_lossy(&engine.wallet_code(confirmed).await?).into_owned())),
        HubItem::Keys => Some(Input::Keys(View(engine.keys_page(confirmed).await?.into()))),
    })
}
