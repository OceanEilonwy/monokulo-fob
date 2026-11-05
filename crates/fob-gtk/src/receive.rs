//! Receive: this wallet's address as a code, with an amount and note if asked.

use fob_app::{Engine, views::{ReceiveCode, Screens}};
use relm4::{
    adw::{self, prelude::*},
    gtk,
    prelude::*,
};

use crate::{
    bridge::View,
    root::Msg as RootMsg,
    widgets,
};

#[derive(Debug)]
pub enum Input {
    Screens(View<Screens>),
    Opened,
    NewAddress,
    Copy,
    Asked,
    Code(View<ReceiveCode>),
    Failed(String),
}

pub struct Receive {
    engine: Engine,
    address: Option<String>,
    uri: Option<String>,
    headline: Option<String>,
    error: Option<String>,
    code: gtk::Box,
    amount: adw::EntryRow,
    note: adw::EntryRow,
}

#[relm4::component(pub)]
impl SimpleComponent for Receive {
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
                    set_maximum_size: 520,
                    #[wrap(Some)]
                    set_child = &gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 18,
                        set_margin_all: 16,
                        #[local_ref]
                        code -> gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 12,
                        },
                        gtk::Label {
                            add_css_class: "title-4",
                            set_wrap: true,
                            #[watch]
                            set_visible: model.headline.is_some(),
                            #[watch]
                            set_label: model.headline.as_deref().unwrap_or(""),
                        },
                        gtk::Box {
                            set_halign: gtk::Align::Center,
                            set_spacing: 12,
                            gtk::Button {
                                set_label: "Copy",
                                add_css_class: "pill",
                                add_css_class: "suggested-action",
                                connect_clicked => Input::Copy,
                            },
                            gtk::Button {
                                set_label: "New address",
                                add_css_class: "pill",
                                connect_clicked => Input::NewAddress,
                            },
                        },
                        adw::PreferencesGroup {
                            set_title: "Ask for an amount",
                            #[local_ref]
                            add = amount -> adw::EntryRow {
                                set_title: "Amount in XMR (optional)",
                                set_input_purpose: gtk::InputPurpose::Number,
                                connect_changed => Input::Asked,
                            },
                            #[local_ref]
                            add = note -> adw::EntryRow {
                                set_title: "Note (optional)",
                                connect_changed => Input::Asked,
                            },
                        },
                        gtk::Label {
                            add_css_class: "error",
                            set_wrap: true,
                            #[watch]
                            set_visible: model.error.is_some(),
                            #[watch]
                            set_label: model.error.as_deref().unwrap_or(""),
                        },
                    },
                },
            },
        }
    }

    fn init(engine: Engine, root: Self::Root, sender: ComponentSender<Self>) -> ComponentParts<Self> {
        let code = gtk::Box::new(gtk::Orientation::Vertical, 12);
        let (amount, note) = (adw::EntryRow::new(), adw::EntryRow::new());
        let model = Receive { engine, address: None, uri: None, headline: None, error: None, code: code.clone(), amount: amount.clone(), note: note.clone() };
        let (code, amount, note) = (&code, &amount, &note);
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, input: Input, sender: ComponentSender<Self>) {
        match input {
            Input::Screens(View(screens)) => {
                let shown = screens.receive.as_ref().map(|r| r.address.clone());
                if shown.is_some() && shown != self.address {
                    self.address = shown;
                    self.ask(&sender);
                }
            }
            // Rust hands one out only now, never on a refresh, so a restored
            // wallet hands none out before its scan has found the others.
            Input::Opened if self.address.is_none() => {
                let (engine, sender) = (self.engine.clone(), sender.clone());
                relm4::spawn(async move {
                    if let Err(e) = engine.change(|wallet| async move { wallet.receive_address().await.map(|_| ()) }).await {
                        sender.input(Input::Failed(e.to_string()));
                    }
                });
            }
            Input::Opened => self.ask(&sender),
            Input::NewAddress => {
                let (engine, sender) = (self.engine.clone(), sender.clone());
                relm4::spawn(async move {
                    if let Err(e) = engine.change(|wallet| async move { wallet.new_receive_address().await.map(|_| ()) }).await {
                        sender.input(Input::Failed(e.to_string()));
                    }
                });
            }
            Input::Copy => {
                if let Some(text) = self.uri.as_ref().or(self.address.as_ref()) {
                    widgets::copy(&self.code, text);
                    let _ = sender.output(RootMsg::Say("Copied".into()));
                }
            }
            Input::Asked => self.ask(&sender),
            Input::Code(View(code)) => {
                self.error = None;
                self.headline = code.headline.clone();
                self.draw(&code.uri);
                self.uri = Some(code.uri.clone());
            }
            Input::Failed(why) => self.error = Some(why),
        }
    }
}

impl Receive {
    /// Asks Rust for the code with what's typed.
    fn ask(&self, sender: &ComponentSender<Self>) {
        let Some(address) = self.address.clone() else { return };
        let amount = Some(self.amount.text().to_string()).filter(|a| !a.trim().is_empty());
        let note = Some(self.note.text().to_string()).filter(|n| !n.trim().is_empty());
        let (engine, sender) = (self.engine.clone(), sender.clone());
        relm4::spawn(async move {
            let code = engine.read(move |wallet, _| async move { wallet.receive_code(address, amount, false, note).await }).await;
            match code {
                Some(Ok(code)) => sender.input(Input::Code(View(code.into()))),
                Some(Err(e)) => sender.input(Input::Failed(e.to_string())),
                None => {}
            }
        });
    }

    fn draw(&self, uri: &str) {
        widgets::empty(&self.code);
        if let Some(qr) = widgets::qr(uri, 280) {
            self.code.append(&qr);
        }
        if let Some(address) = &self.address {
            self.code.append(&widgets::pattern(address));
            self.code.append(&widgets::address(address));
        }
    }
}
