//! Home: the balance, how catching up is going, and activity by day.

use fob_app::{
    Engine,
    views::{ChipLook, Network, Screens, SyncDetails, SyncStatus},
};
use relm4::{
    adw::{self, prelude::*},
    gtk,
    prelude::*,
};

use crate::{
    bridge::View,
    root::{Go, Msg as RootMsg},
    settings::Wallets,
    widgets,
};

#[derive(Debug)]
pub enum Input {
    Screens(View<Screens>),
    Status(View<SyncStatus>),
    Wallets(View<Wallets>),
    SwitchWallet(String),
    SwapBalance,
    ShowSync,
    SyncTick,
    SyncClosed,
    SyncNow,
    ScanAgain,
    /// Ask the Trezor for the key images it hasn't given.
    TrezorSync,
    Say(String),
}

pub struct Home {
    engine: Engine,
    screens: Option<View<Screens>>,
    status: Option<View<SyncStatus>>,
    title: String,
    days: gtk::Box,
    sync_card: Option<SyncCard>,
    /// The wallets under the title, with a way to add one.
    wallet_list: gtk::ListBox,
    popover: gtk::Popover,
}

impl Home {
    fn can_swap(&self) -> bool {
        self.screens.as_ref().is_some_and(|View(s)| s.balance.can_swap)
    }

    fn big(&self) -> String {
        let Some(View(s)) = &self.screens else { return "—".into() };
        match (&s.balance.fiat_text, s.balance.fiat_first) {
            (Some(fiat), true) => fiat.clone(),
            _ => format!("{} XMR", s.balance.total_text),
        }
    }

    fn small(&self) -> String {
        let Some(View(s)) = &self.screens else { return String::new() };
        match (&s.balance.fiat_line, s.balance.fiat_first) {
            (Some(_), true) => format!("{} XMR", s.balance.total_text),
            (Some(line), false) => line.clone(),
            _ => String::new(),
        }
    }

    fn ready(&self) -> String {
        self.screens.as_ref().map_or(String::new(), |View(s)| s.balance.ready_line.clone())
    }

    fn reminder(&self) -> Option<(String, String)> {
        self.screens.as_ref().and_then(|View(s)| s.reminder.as_ref()).map(|r| (r.title.clone(), r.line.clone()))
    }

    /// Money a Trezor wallet can't spend until the device has been asked.
    fn trezor_waiting(&self) -> Option<String> {
        let n = self.screens.as_ref().map_or(0, |View(s)| s.trezor_unsynced);
        (n > 0).then(|| format!("Connect your Trezor: {} come in and {} for its key image{}", if n == 1 { "a payment has".to_owned() } else { format!("{n} payments have") }, if n == 1 { "waits" } else { "wait" }, if n == 1 { "" } else { "s" }))
    }

    /// Coins kept before Fob recorded transaction keys: a rescan finds them again.
    fn trezor_frozen(&self) -> Option<String> {
        let n = self.screens.as_ref().map_or(0, |View(s)| s.trezor_frozen);
        (n > 0).then(|| format!("{} arrived before Fob kept transaction keys, so the Trezor can't be asked about {}: scan again from before", if n == 1 { "A coin".to_owned() } else { format!("{n} coins") }, if n == 1 { "it" } else { "them" }))
    }

    /// Manual Sync shows Sync in the chip's place, unless something's wrong.
    fn manual(&self) -> Option<&SyncStatus> {
        self.status.as_ref().map(|View(s)| s.as_ref()).filter(|s| s.manual_sync && s.error.is_none())
    }

    fn chip(&self) -> String {
        if let Some(status) = self.manual() {
            return status.scan.as_ref().map_or("Sync".to_owned(), |scan| format!("{}%", (scan.progress * 100.0) as u32));
        }
        self.status.as_ref().map_or(String::new(), |View(s)| s.chip_text.clone())
    }

    fn chip_look(&self) -> &'static [&'static str] {
        if let Some(status) = self.manual() {
            return if status.scan.is_some() { &["sync-chip", "sync-ring", "flat"] } else { &["pill", "suggested-action"] };
        }
        match self.status.as_ref().map(|View(s)| s.chip_look) {
            Some(ChipLook::Synced) => &["flat", "sync-chip", "sync-synced"],
            Some(ChipLook::Offline) => &["flat", "sync-chip", "sync-offline"],
            _ => &["flat", "sync-chip", "sync-ring"],
        }
    }

    fn draw_wallets(&self, wallets: &Wallets, sender: &ComponentSender<Self>) {
        widgets::clear(&self.wallet_list);
        for wallet in &wallets.all {
            let network = match wallet.network {
                Network::Mainnet => "Mainnet",
                Network::Stagenet => "Stagenet, test coins",
            };
            let row = widgets::row(&wallet.name, network);
            if wallets.current.as_ref().is_some_and(|c| c.file == wallet.file) {
                row.add_suffix(&gtk::Image::from_icon_name("object-select-symbolic"));
            } else {
                row.set_activatable(true);
                let (file, sender, popover) = (wallet.file.clone(), sender.clone(), self.popover.clone());
                row.connect_activated(move |_| {
                    popover.popdown();
                    sender.input(Input::SwitchWallet(file.clone()));
                });
            }
            self.wallet_list.append(&row);
        }
        let add = widgets::row("Add a wallet", "");
        add.add_prefix(&gtk::Image::from_icon_name("list-add-symbolic"));
        add.set_activatable(true);
        let (sender, popover) = (sender.clone(), self.popover.clone());
        add.connect_activated(move |_| {
            popover.popdown();
            let _ = sender.output(RootMsg::Go(Go::AddWallet));
        });
        self.wallet_list.append(&add);
    }

    fn draw_days(&self, screens: &Screens, sender: &ComponentSender<Self>) {
        widgets::empty(&self.days);
        if screens.activity.is_empty() {
            let none = adw::StatusPage::builder().title("Nothing yet").description("Payments in and out show here.").icon_name("view-list-symbolic").build();
            none.add_css_class("compact");
            self.days.append(&none);
        }
        for day in &screens.activity {
            let group = adw::PreferencesGroup::builder().title(gtk::glib::markup_escape_text(&day.label).as_str()).build();
            for view in &day.rows {
                let then = sender.clone();
                group.add(&widgets::activity_row(view, move |go| {
                    let _ = then.output(RootMsg::Go(go));
                }));
            }
            self.days.append(&group);
        }
    }
}

#[relm4::component(pub)]
impl SimpleComponent for Home {
    type Init = Engine;
    type Input = Input;
    type Output = RootMsg;

    view! {
        adw::ToolbarView {
            add_top_bar = &adw::HeaderBar {
                #[local_ref]
                #[wrap(Some)]
                set_title_widget = wallet_button -> gtk::MenuButton {
                    add_css_class: "flat",
                    set_always_show_arrow: true,
                    set_tooltip_text: Some("Wallets"),
                    #[watch]
                    set_label: if model.title.is_empty() { "Fob" } else { &model.title },
                },
                pack_start = &gtk::Button {
                    set_icon_name: "system-users-symbolic",
                    set_tooltip_text: Some("Contacts"),
                    connect_clicked[sender] => move |_| { let _ = sender.output(RootMsg::Go(Go::Contacts)); },
                },
                pack_end = &gtk::Button {
                    set_icon_name: "emblem-system-symbolic",
                    set_tooltip_text: Some("Settings"),
                    connect_clicked[sender] => move |_| { let _ = sender.output(RootMsg::Go(Go::Settings)); },
                },
                pack_end = &gtk::Button {
                    set_icon_name: "view-refresh-symbolic",
                    set_tooltip_text: Some("Refresh"),
                    connect_clicked => Input::SyncNow,
                },
            },
            #[wrap(Some)]
            set_content = &gtk::ScrolledWindow {
                set_hscrollbar_policy: gtk::PolicyType::Never,
                set_vexpand: true,
                #[wrap(Some)]
                set_child = &adw::Clamp {
                    set_maximum_size: 600,
                    #[wrap(Some)]
                    set_child = &gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 18,
                        set_margin_all: 16,

                        gtk::Button {
                            set_halign: gtk::Align::Center,
                            #[watch]
                            set_css_classes: model.chip_look(),
                            #[watch]
                            set_label: &model.chip(),
                            #[watch]
                            set_visible: model.status.is_some(),
                            set_tooltip_text: Some("How catching up is going"),
                            connect_clicked => Input::ShowSync,
                        },
                        gtk::Button {
                            add_css_class: "flat",
                            set_halign: gtk::Align::Center,
                            #[watch]
                            set_tooltip_text: if model.can_swap() { Some("Show the other currency first") } else { None },
                            connect_clicked => Input::SwapBalance,
                            #[wrap(Some)]
                            set_child = &gtk::Box {
                                set_orientation: gtk::Orientation::Vertical,
                                gtk::Label {
                                    add_css_class: "balance",
                                    #[watch]
                                    set_label: &model.big(),
                                },
                                gtk::Box {
                                    set_halign: gtk::Align::Center,
                                    set_spacing: 6,
                                    #[watch]
                                    set_visible: !model.small().is_empty(),
                                    gtk::Label {
                                        add_css_class: "dim-label",
                                        #[watch]
                                        set_label: &model.small(),
                                    },
                                    // Tapping the balance swaps them: the mark says so.
                                    #[local_ref]
                                    swap_mark -> gtk::DrawingArea {
                                        #[watch]
                                        set_visible: model.can_swap(),
                                    },
                                },
                            },
                        },
                        gtk::Button {
                            add_css_class: "flat",
                            set_halign: gtk::Align::Center,
                            #[watch]
                            set_label: &model.ready(),
                            #[watch]
                            set_visible: !model.ready().is_empty(),
                            set_tooltip_text: Some("Coins"),
                            connect_clicked[sender] => move |_| { let _ = sender.output(RootMsg::Go(Go::Coins)); },
                        },
                        gtk::Box {
                            set_halign: gtk::Align::Center,
                            set_spacing: 12,
                            gtk::Button {
                                set_label: "Receive",
                                add_css_class: "pill",
                                connect_clicked[sender] => move |_| { let _ = sender.output(RootMsg::Go(Go::Receive)); },
                            },
                            gtk::Button {
                                set_label: "Send",
                                add_css_class: "pill",
                                add_css_class: "suggested-action",
                                connect_clicked[sender] => move |_| { let _ = sender.output(RootMsg::Go(Go::Send)); },
                            },
                        },
                        gtk::Box {
                            set_orientation: gtk::Orientation::Horizontal,
                            set_spacing: 12,
                            add_css_class: "warning-card",
                            add_css_class: "activatable",
                            set_cursor_from_name: Some("pointer"),
                            #[watch]
                            set_visible: model.reminder().is_some(),
                            add_controller = gtk::GestureClick {
                                connect_released[sender] => move |_, _, _, _| { let _ = sender.output(RootMsg::Go(Go::SettingsBackup)); },
                            },
                            gtk::Box {
                                set_orientation: gtk::Orientation::Vertical,
                                set_spacing: 4,
                                set_hexpand: true,
                                gtk::Label {
                                    set_xalign: 0.0,
                                    add_css_class: "heading",
                                    #[watch]
                                    set_label: &model.reminder().map_or(String::new(), |(title, _)| title),
                                },
                                gtk::Label {
                                    set_xalign: 0.0,
                                    set_wrap: true,
                                    #[watch]
                                    set_label: &model.reminder().map_or(String::new(), |(_, line)| line),
                                },
                            },
                            gtk::Image {
                                set_icon_name: Some("go-next-symbolic"),
                                set_valign: gtk::Align::Center,
                            },
                        },
                        adw::Banner {
                            set_button_label: Some("Sync"),
                            #[watch]
                            set_title: &model.trezor_waiting().unwrap_or_default(),
                            #[watch]
                            set_revealed: model.trezor_waiting().is_some(),
                            // Not there at all while hidden: a banner with no height still takes the box's gap.
                            #[watch]
                            set_visible: model.trezor_waiting().is_some(),
                            connect_button_clicked => Input::TrezorSync,
                        },
                        adw::Banner {
                            set_button_label: Some("Scan"),
                            #[watch]
                            set_title: &model.trezor_frozen().unwrap_or_default(),
                            #[watch]
                            set_revealed: model.trezor_frozen().is_some(),
                            #[watch]
                            set_visible: model.trezor_frozen().is_some(),
                            connect_button_clicked[sender] => move |_| { let _ = sender.output(RootMsg::Go(Go::Scan)); },
                        },
                        #[local_ref]
                        days -> gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 18,
                        },
                    },
                },
            },
        }
    }

    fn init(engine: Engine, root: Self::Root, sender: ComponentSender<Self>) -> ComponentParts<Self> {
        let days = gtk::Box::new(gtk::Orientation::Vertical, 18);
        let wallet_button = gtk::MenuButton::new();
        let wallet_list = widgets::list();
        let popover = gtk::Popover::new();
        popover.set_child(Some(&wallet_list));
        wallet_button.set_popover(Some(&popover));
        let model = Home { engine, screens: None, status: None, title: String::new(), days: days.clone(), sync_card: None, wallet_list, popover };
        let swap_mark = widgets::swap_icon(16, None);
        let (days, wallet_button, swap_mark) = (&days, &wallet_button, &swap_mark);
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, input: Input, sender: ComponentSender<Self>) {
        match input {
            Input::Screens(view) => {
                self.draw_days(&view.0, &sender);
                self.screens = Some(view);
            }
            Input::Status(view) => self.status = Some(view),
            Input::Wallets(View(wallets)) => {
                self.title = wallets.current.as_ref().map_or(String::new(), |c| c.name.clone());
                self.draw_wallets(&wallets, &sender);
            }
            Input::SwitchWallet(file) => {
                let (engine, sender) = (self.engine.clone(), sender.clone());
                relm4::spawn(async move {
                    if let Err(e) = engine.switch_wallet(file).await {
                        sender.input(Input::Say(e.to_string()));
                    }
                });
            }
            Input::SwapBalance => {
                // Nothing to swap to: a balance stays readable, so it isn't greyed out.
                if !self.screens.as_ref().is_some_and(|View(s)| s.balance.can_swap) {
                    return;
                }
                let first = self.screens.as_ref().is_some_and(|View(s)| s.balance.fiat_first);
                let engine = self.engine.clone();
                relm4::spawn(async move {
                    let _ = engine.change(move |wallet| async move { wallet.set_fiat_first(!first).await }).await;
                });
            }
            Input::ShowSync if self.manual().is_some() => {
                let _ = sender.output(RootMsg::Go(Go::Scan));
            }
            Input::ShowSync => self.show_sync(&sender),
            Input::SyncTick => {
                if let (Some(card), Some(details)) = (&mut self.sync_card, self.engine.sync_details()) {
                    card.draw(details);
                }
            }
            Input::SyncClosed => {
                if let Some(card) = self.sync_card.take() {
                    card.ticking.remove();
                }
            }
            Input::SyncNow => {
                let (engine, sender) = (self.engine.clone(), sender.clone());
                relm4::spawn(async move {
                    match engine.sync_now().await {
                        Ok(Some(said)) => sender.input(Input::Say(format!("{} · {}", said.title, said.line))),
                        Ok(None) => {}
                        Err(e) => sender.input(Input::Say(format!("Couldn't check · {e}"))),
                    }
                });
            }
            Input::ScanAgain => {
                let (engine, sender) = (self.engine.clone(), sender.clone());
                relm4::spawn(async move {
                    if let Err(e) = engine.change(|wallet| async move { wallet.scan_again().await }).await {
                        sender.input(Input::Say(e.to_string()));
                    }
                });
            }
            Input::TrezorSync => {
                let (engine, then) = (self.engine.clone(), sender.clone());
                let prompter = crate::trezor::prompter(&sender);
                relm4::spawn(async move {
                    match engine.trezor_sync(prompter).await {
                        Ok(n) => then.input(Input::Say(if n == 1 { "One coin's key image is in".into() } else { format!("{n} coins' key images are in") })),
                        Err(e) => then.input(Input::Say(e.to_string())),
                    }
                });
            }
            Input::Say(text) => {
                let _ = sender.output(RootMsg::Say(text));
            }
        }
    }
}

impl Home {
    /// The card behind the chip, every word from Rust. It follows the wallet
    /// while it's open, as its times and counts move on their own.
    fn show_sync(&mut self, sender: &ComponentSender<Self>) {
        if self.sync_card.is_some() {
            return;
        }
        let Some(details) = self.engine.sync_details() else { return };
        let dialog = adw::AlertDialog::new(None, None);
        dialog.add_response("close", "_Close");
        let then = sender.clone();
        dialog.connect_response(Some("again"), move |_, _| then.input(Input::ScanAgain));
        let then = sender.clone();
        dialog.connect_closed(move |_| then.input(Input::SyncClosed));
        let then = sender.clone();
        let ticking = gtk::glib::timeout_add_seconds_local(1, move || {
            then.input(Input::SyncTick);
            gtk::glib::ControlFlow::Continue
        });
        let mut card = SyncCard { dialog, ticking, shown: vec![] };
        card.draw(details);
        card.dialog.present(Some(&self.days));
        self.sync_card = Some(card);
    }
}

/// The card behind the chip, while it's open.
struct SyncCard {
    dialog: adw::AlertDialog,
    ticking: gtk::glib::SourceId,
    /// What it's showing, so the same words aren't drawn over themselves.
    shown: Vec<String>,
}

impl SyncCard {
    fn draw(&mut self, details: SyncDetails) {
        let mut words = vec![details.title.clone(), details.percent.clone().unwrap_or_default(), details.note.clone().unwrap_or_default(), details.error.clone().unwrap_or_default(), details.scan_again.clone().unwrap_or_default()];
        words.extend(details.rows.iter().flat_map(|row| [row.label.clone(), row.value.clone()]));
        if self.shown == words {
            return;
        }
        self.shown = words;
        let list = widgets::list();
        for row in &details.rows {
            let view = widgets::row(&row.label, "");
            let value = gtk::Label::new(Some(&row.value));
            value.add_css_class("dim-label");
            view.add_suffix(&value);
            list.append(&view);
        }
        let body = [details.percent.as_deref(), details.note.as_deref(), details.error.as_deref()].into_iter().flatten().collect::<Vec<_>>().join("\n");
        self.dialog.set_heading(Some(&details.title));
        self.dialog.set_body(&body);
        self.dialog.set_extra_child(Some(&list));
        // Too deep a reorganisation to follow: the way out.
        match &details.scan_again {
            Some(words) if self.dialog.has_response("again") => self.dialog.set_response_label("again", words),
            Some(words) => self.dialog.add_response("again", words),
            None if self.dialog.has_response("again") => self.dialog.remove_response("again"),
            None => {}
        }
    }
}
