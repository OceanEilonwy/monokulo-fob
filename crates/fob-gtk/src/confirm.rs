//! Confirm: what's paid, to whom and the fee. It opens at once from what's
//! typed; the fee comes in when the node answers. Sending asks for the passphrase.

use std::sync::Arc;

use fob_app::{Engine, views::{Prepared, SaveOffer}};
use relm4::{
    adw::{self, prelude::*},
    gtk,
    prelude::*,
};

use crate::{
    bridge::{Secret, View},
    root::{Go, Msg as RootMsg},
    widgets,
};

/// What the form was filled with.
#[derive(Clone, Debug)]
pub struct Filled {
    pub to: String,
    pub amount: String,
    pub in_fiat: bool,
    pub note: Option<String>,
    pub code: Option<String>,
}

#[derive(Debug)]
pub enum Input {
    Open(Filled),
    Ready(View<Prepared>),
    Failed(String),
    Send,
    Typed(Secret),
    /// Left without sending: the wallet can let what it prepared go.
    Left,
    Sent(String),
    Offer(View<SaveOffer>),
    Keep(View<SaveOffer>),
    Done,
}

pub struct Confirm {
    engine: Engine,
    prepared: Option<Arc<Prepared>>,
    busy: bool,
    sent: Option<String>,
    error: Option<String>,
    body: gtk::Box,
}

#[relm4::component(pub)]
impl SimpleComponent for Confirm {
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
                    set_maximum_size: 560,
                    #[wrap(Some)]
                    set_child = &gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 18,
                        set_margin_all: 16,
                        #[local_ref]
                        body -> gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 18,
                        },
                        gtk::Label {
                            add_css_class: "error",
                            set_wrap: true,
                            #[watch]
                            set_visible: model.error.is_some(),
                            #[watch]
                            set_label: model.error.as_deref().unwrap_or(""),
                        },
                        gtk::Button {
                            add_css_class: "suggested-action",
                            add_css_class: "pill",
                            set_halign: gtk::Align::Center,
                            #[watch]
                            set_label: if model.sent.is_some() { "Done" } else if model.busy { "Working…" } else { "Send" },
                            #[watch]
                            set_sensitive: model.sent.is_some() || (model.prepared.is_some() && !model.busy),
                            connect_clicked => Input::Send,
                        },
                    },
                },
            },
        }
    }

    fn init(engine: Engine, root: Self::Root, sender: ComponentSender<Self>) -> ComponentParts<Self> {
        let body = gtk::Box::new(gtk::Orientation::Vertical, 18);
        let model = Confirm { engine, prepared: None, busy: false, sent: None, error: None, body: body.clone() };
        let body = &body;
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, input: Input, sender: ComponentSender<Self>) {
        match input {
            Input::Open(filled) => {
                if let Some(old) = self.prepared.take() {
                    self.engine.cancel(&old);
                }
                self.busy = true;
                self.sent = None;
                self.error = None;
                self.draw_waiting(&filled);
                let (engine, sender) = (self.engine.clone(), sender.clone());
                relm4::spawn(async move {
                    let prepared = engine.read(move |wallet, _| async move { wallet.prepare(filled.to, filled.amount, filled.in_fiat, filled.note, filled.code).await }).await;
                    match prepared {
                        Some(Ok(prepared)) => sender.input(Input::Ready(View(prepared.into()))),
                        Some(Err(e)) => sender.input(Input::Failed(e.to_string())),
                        None => {}
                    }
                });
            }
            Input::Ready(View(prepared)) => {
                if let Some(old) = self.prepared.take() {
                    self.engine.cancel(&old);
                }
                self.busy = false;
                self.sent = None;
                self.error = None;
                self.draw(&prepared);
                self.prepared = Some(prepared);
            }
            Input::Failed(why) => {
                self.busy = false;
                self.error = Some(why);
            }
            Input::Send => {
                if self.sent.is_some() {
                    self.sent = None;
                    let _ = sender.output(RootMsg::Go(Go::Back));
                    let _ = sender.output(RootMsg::Go(Go::Back));
                    return;
                }
                let then = sender.clone();
                widgets::ask_passphrase(self.body.upcast_ref(), "Send this payment?", "Type your passphrase to send it. It can't be taken back.", move |typed| match typed {
                    Some(typed) => then.input(Input::Typed(Secret(typed))),
                    None => then.input(Input::Done),
                });
            }
            Input::Typed(Secret(typed)) => {
                let Some(prepared) = self.prepared.clone() else { return };
                self.busy = true;
                self.error = None;
                let (engine, sender) = (self.engine.clone(), sender.clone());
                relm4::spawn(async move {
                    sender.input(match engine.send(prepared, typed).await {
                        Ok(tx) => Input::Sent(tx),
                        Err(e) => Input::Failed(e.to_string()),
                    });
                });
            }
            Input::Sent(tx) => {
                self.busy = false;
                let done = self.prepared.take().and_then(|p| p.split.as_ref().map(|s| s.done.clone()));
                widgets::empty(&self.body);
                let page = adw::StatusPage::builder().icon_name("emblem-ok-symbolic").title(done.as_deref().unwrap_or("Sent")).description(&tx).build();
                self.body.append(&page);
                let (engine, then, asked) = (self.engine.clone(), sender.clone(), tx.clone());
                relm4::spawn(async move {
                    if let Some(Some(offer)) = engine.read(move |wallet, _| async move { wallet.save_offer(asked).await }).await {
                        then.input(Input::Offer(View(offer.into())));
                    }
                });
                self.sent = Some(tx);
            }
            Input::Offer(offer) => {
                let button = gtk::Button::builder().label(&offer.0.button).halign(gtk::Align::Center).css_classes(["pill"]).build();
                let (then, kept) = (sender.clone(), offer.clone());
                button.connect_clicked(move |button| {
                    button.set_sensitive(false);
                    then.input(Input::Keep(kept.clone()));
                });
                self.body.append(&button);
                if let Some(named) = &offer.0.named {
                    self.body.append(&widgets::centred(named));
                }
                if let Some(footnote) = &offer.0.footnote {
                    self.body.append(&widgets::centred(footnote));
                }
            }
            Input::Keep(View(offer)) => {
                let (engine, then) = (self.engine.clone(), sender.clone());
                let existing = offer.add_to.as_ref().map(|c| c.id);
                let (address, draft) = (offer.address.clone(), copy_draft(&offer.draft));
                relm4::spawn(async move {
                    let saved = engine
                        .change(move |wallet| async move {
                            match existing {
                                Some(id) => wallet.add_contact_paid_at(id, address).await,
                                None => wallet.add_contact(draft).await,
                            }
                        })
                        .await;
                    match saved {
                        Ok(contact) => {
                            let _ = then.output(RootMsg::Say(format!("Saved {}", contact.name)));
                        }
                        Err(e) => then.input(Input::Failed(e.to_string())),
                    }
                });
            }
            Input::Done => self.busy = false,
            Input::Left => {
                if let Some(prepared) = self.prepared.take() {
                    self.engine.cancel(&prepared);
                }
                widgets::empty(&self.body);
            }
        }
    }
}

fn copy_draft(draft: &fob_app::views::ContactDraft) -> fob_app::views::ContactDraft {
    fob_app::views::ContactDraft { name: draft.name.clone(), note: draft.note.clone(), photo: draft.photo.clone(), send_address: draft.send_address.clone(), also_paid_at: draft.also_paid_at.clone() }
}

impl Confirm {
    fn draw_waiting(&self, filled: &Filled) {
        widgets::empty(&self.body);
        self.body.append(&widgets::pattern(&filled.to));
        self.body.append(&widgets::address(&filled.to));
        self.body.append(&gtk::Spinner::builder().spinning(true).halign(gtk::Align::Center).build());
    }

    fn draw(&self, prepared: &Prepared) {
        widgets::empty(&self.body);
        match &prepared.split {
            // Paid back to this wallet: there's no one else's address to check.
            Some(split) => {
                self.body.append(&widgets::heading(&split.title));
                self.body.append(&widgets::centred(&split.to));
            }
            None => {
                self.body.append(&widgets::pattern(&prepared.to));
                self.body.append(&widgets::address(&prepared.to));
            }
        }
        if let Some(receipt) = &prepared.receipt {
            self.body.append(&widgets::receipt(receipt));
        }
        let list = widgets::list();
        let row = |title: &str, value: &str, fiat: Option<&str>| {
            let view = widgets::row(title, fiat.unwrap_or(""));
            let text = gtk::Label::new(Some(value));
            text.add_css_class("money");
            view.add_suffix(&text);
            list.append(&view);
        };
        row("Amount", &format!("{} XMR", prepared.amount_text), prepared.amount_fiat.as_deref());
        row("Fee", &format!("{} XMR", prepared.fee_text), prepared.fee_fiat.as_deref());
        row("Total", &format!("{} XMR", prepared.total_text), prepared.total_fiat.as_deref());
        self.body.append(&list);
        if let Some(split) = &prepared.split {
            self.body.append(&widgets::dim(&split.pieces_text));
        }
        if let Some(age) = &prepared.fiat_age {
            self.body.append(&widgets::dim(age));
        }
    }
}
