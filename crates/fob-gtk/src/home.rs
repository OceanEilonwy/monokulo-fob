//! Home: the balance, how catching up is going, and activity by day.

use fob_app::{
    Engine,
    views::{ChipLook, Screens, SyncStatus},
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
    SwapBalance,
    ShowSync,
    SyncNow,
}

pub struct Home {
    engine: Engine,
    screens: Option<View<Screens>>,
    status: Option<View<SyncStatus>>,
    title: String,
    days: gtk::Box,
}

impl Home {
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

    fn chip(&self) -> String {
        self.status.as_ref().map_or(String::new(), |View(s)| s.chip_text.clone())
    }

    fn chip_look(&self) -> &'static [&'static str] {
        match self.status.as_ref().map(|View(s)| s.chip_look) {
            Some(ChipLook::Synced) => &["flat", "sync-chip", "sync-synced"],
            Some(ChipLook::Offline) => &["flat", "sync-chip", "sync-offline"],
            _ => &["flat", "sync-chip", "sync-ring"],
        }
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
                set_title_widget = window_title -> adw::WindowTitle {
                    set_title: "Fob",
                    #[watch]
                    set_subtitle: &model.title,
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
                    set_tooltip_text: Some("Check the node now"),
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
                            connect_clicked => Input::SwapBalance,
                            #[wrap(Some)]
                            set_child = &gtk::Box {
                                set_orientation: gtk::Orientation::Vertical,
                                gtk::Label {
                                    add_css_class: "balance",
                                    #[watch]
                                    set_label: &model.big(),
                                },
                                gtk::Label {
                                    add_css_class: "dim-label",
                                    #[watch]
                                    set_label: &model.small(),
                                    #[watch]
                                    set_visible: !model.small().is_empty(),
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
                        adw::Banner {
                            set_button_label: Some("Back up"),
                            #[watch]
                            set_title: &model.reminder().map_or(String::new(), |(title, line)| format!("{title}: {line}")),
                            #[watch]
                            set_revealed: model.reminder().is_some(),
                            connect_button_clicked[sender] => move |_| { let _ = sender.output(RootMsg::Go(Go::Settings)); },
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
        let window_title = adw::WindowTitle::new("Fob", "");
        let model = Home { engine, screens: None, status: None, title: String::new(), days: days.clone() };
        let (days, window_title) = (&days, &window_title);
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
            Input::ShowSync => self.show_sync(),
            Input::SyncNow => {
                let engine = self.engine.clone();
                relm4::spawn(async move { engine.sync_now().await });
            }
        }
    }
}

impl Home {
    /// The card behind the chip, every word from Rust.
    fn show_sync(&self) {
        let Some(details) = self.engine.sync_details() else { return };
        let list = widgets::list();
        for row in &details.rows {
            let view = widgets::row(&row.label, "");
            let value = gtk::Label::new(Some(&row.value));
            value.add_css_class("dim-label");
            view.add_suffix(&value);
            list.append(&view);
        }
        let body = [details.percent, details.note, details.error].into_iter().flatten().collect::<Vec<_>>().join("\n");
        let dialog = adw::AlertDialog::builder().heading(&details.title).body(&body).extra_child(&list).build();
        dialog.add_response("close", "_Close");
        dialog.present(Some(&self.days));
    }
}
