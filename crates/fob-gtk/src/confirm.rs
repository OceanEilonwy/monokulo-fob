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
    /// What a round of preparing came to; an older round's answer is dropped.
    Ready(u64, View<Prepared>),
    /// Prepared elsewhere (Split): a round of its own.
    Adopt(View<Prepared>),
    NotPrepared(u64, String),
    Failed(String),
    /// The receipt disagrees with Fob's price, and its warning was read.
    Acknowledged,
    Send,
    Typed(Secret),
    /// Left without sending: the wallet can let what it prepared go.
    Left,
    Sent(String),
    /// The node refused it: a coin may already be spent.
    Refused,
    Offer(View<SaveOffer>),
    Keep(View<SaveOffer>),
    Done,
}

pub struct Confirm {
    engine: Engine,
    /// Counts each time Confirm opens or is left: a prepare that answers for
    /// another round is cancelled, never shown, so Send only pays what's on screen.
    round: u64,
    prepared: Option<Arc<Prepared>>,
    /// The receipt's mismatch warning was acknowledged for what's prepared now.
    acknowledged: bool,
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
                            set_sensitive: model.sent.is_some() || (model.prepared.is_some() && !model.busy && !model.awaits_acknowledgement()),
                            connect_clicked => Input::Send,
                        },
                    },
                },
            },
        }
    }

    fn init(engine: Engine, root: Self::Root, sender: ComponentSender<Self>) -> ComponentParts<Self> {
        let body = gtk::Box::new(gtk::Orientation::Vertical, 18);
        let model = Confirm { engine, round: 0, prepared: None, acknowledged: false, busy: false, sent: None, error: None, body: body.clone() };
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
                self.round += 1;
                let round = self.round;
                self.acknowledged = false;
                self.busy = true;
                self.sent = None;
                self.error = None;
                self.draw_waiting(&filled);
                let (engine, sender) = (self.engine.clone(), sender.clone());
                relm4::spawn(async move {
                    let prepared = engine.read(move |wallet, _| async move { wallet.prepare(filled.to, filled.amount, filled.in_fiat, filled.note, filled.code).await }).await;
                    match prepared {
                        Some(Ok(prepared)) => sender.input(Input::Ready(round, View(prepared.into()))),
                        Some(Err(e)) => sender.input(Input::NotPrepared(round, e.to_string())),
                        None => {}
                    }
                });
            }
            Input::Ready(round, View(prepared)) => {
                // Prepared for a form since changed, or after leaving: let it
                // go, so it neither replaces the payment shown nor holds coins.
                if round != self.round {
                    self.engine.cancel(&prepared);
                    return;
                }
                if let Some(old) = self.prepared.take() {
                    self.engine.cancel(&old);
                }
                self.busy = false;
                self.sent = None;
                self.error = None;
                self.acknowledged = false;
                self.draw(&prepared, &sender);
                self.prepared = Some(prepared);
            }
            Input::Adopt(prepared) => {
                self.round += 1;
                <Self as SimpleComponent>::update(self, Input::Ready(self.round, prepared), sender);
            }
            Input::NotPrepared(round, why) => {
                if round == self.round {
                    self.busy = false;
                    self.error = Some(why);
                }
            }
            Input::Acknowledged => self.acknowledged = true,
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
                if self.prepared.is_none() || self.awaits_acknowledgement() {
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
                    // The engine wraps its copy at once; this one is wiped as it drops.
                    sender.input(match engine.send(prepared, String::clone(&typed)).await {
                        Ok(tx) => Input::Sent(tx),
                        Err(fob_app::FobError::SpendRejected { .. }) => Input::Refused,
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
            Input::Refused => {
                self.busy = false;
                let dialog = adw::AlertDialog::builder().heading("Nothing was sent").body("Your node refused this payment. A coin in it may already be spent, maybe from another device. Sync so Fob knows, then try again.").build();
                dialog.add_responses(&[("close", "_Close"), ("sync", "_Sync")]);
                dialog.set_close_response("close");
                dialog.set_response_appearance("sync", adw::ResponseAppearance::Suggested);
                let then = sender.clone();
                dialog.connect_response(Some("sync"), move |_, _| {
                    let _ = then.output(RootMsg::Go(Go::Scan));
                });
                dialog.present(Some(&self.body));
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
                // A prepare still running belongs to no screen now.
                self.round += 1;
                self.acknowledged = false;
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
    /// Whether Send waits for the receipt's warning to be acknowledged.
    fn awaits_acknowledgement(&self) -> bool {
        !self.acknowledged && self.prepared.as_ref().and_then(|p| p.receipt.as_ref()).is_some_and(|r| r.mismatch.is_some())
    }

    fn draw_waiting(&self, filled: &Filled) {
        widgets::empty(&self.body);
        self.body.append(&widgets::pattern(&filled.to));
        self.body.append(&widgets::address(&filled.to));
        self.body.append(&gtk::Spinner::builder().spinning(true).halign(gtk::Align::Center).build());
    }

    fn draw(&self, prepared: &Prepared, sender: &ComponentSender<Self>) {
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
        // The receipt's total is only the code's claim: a hostile code can put
        // a calm "$5.00" over 5 XMR. When Fob's price says the amount is worth
        // something else, that comes first, and Send waits until it's been read.
        let mismatch = prepared.receipt.as_ref().and_then(|r| r.mismatch.as_deref());
        if let Some(mismatch) = mismatch {
            let banner = gtk::Box::new(gtk::Orientation::Vertical, 8);
            banner.add_css_class("card");
            let title = widgets::heading("The receipt doesn't match the amount");
            title.add_css_class("error");
            title.set_margin_top(12);
            banner.append(&title);
            let warning = widgets::words(mismatch);
            warning.add_css_class("error");
            warning.set_margin_start(12);
            warning.set_margin_end(12);
            banner.append(&warning);
            let check = gtk::CheckButton::builder().label("I've checked the XMR amount").halign(gtk::Align::Center).margin_bottom(8).build();
            let then = sender.clone();
            check.connect_toggled(move |check| {
                if check.is_active() {
                    check.set_sensitive(false);
                    then.input(Input::Acknowledged);
                }
            });
            banner.append(&check);
            self.body.append(&banner);
        }
        // What's paid, in XMR and at Fob's own price, is the headline.
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
        // The code's own receipt follows, as what it says rather than what's paid.
        if let Some(receipt) = &prepared.receipt {
            self.body.append(&widgets::dim("The receipt, as the code gives it"));
            self.body.append(&widgets::receipt(receipt));
        }
    }
}
