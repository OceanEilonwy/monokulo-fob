//! One payment's own screen, every word from Rust.

use fob_app::{Engine, views::{PaymentDetail, Screens}};
use relm4::{
    adw::{self, prelude::*},
    gtk,
    prelude::*,
};

use crate::{bridge::View, root::Msg as RootMsg, widgets};

#[derive(Debug)]
pub enum Input {
    Open(String, Option<View<Screens>>),
    Show(Option<View<PaymentDetail>>),
    CopyTx,
    AskRelease,
    Released,
    Failed(String),
}

pub struct Payment {
    engine: Engine,
    tx: Option<String>,
    screens: Option<View<Screens>>,
    body: gtk::Box,
}

#[relm4::component(pub)]
impl SimpleComponent for Payment {
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
                    #[local_ref]
                    #[wrap(Some)]
                    set_child = body -> gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 18,
                        set_margin_all: 16,
                    },
                },
            },
        }
    }

    fn init(engine: Engine, root: Self::Root, _sender: ComponentSender<Self>) -> ComponentParts<Self> {
        let body = gtk::Box::new(gtk::Orientation::Vertical, 18);
        let model = Payment { engine, tx: None, screens: None, body: body.clone() };
        let body = &body;
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, input: Input, sender: ComponentSender<Self>) {
        match input {
            Input::Open(tx, screens) => {
                self.tx = Some(tx.clone());
                self.screens = screens;
                let (engine, sender) = (self.engine.clone(), sender.clone());
                relm4::spawn(async move {
                    let detail = engine.read(move |wallet, clock| async move { wallet.payment(tx, clock.now, clock.utc_offset, clock.hours_24).await }).await.flatten();
                    sender.input(Input::Show(detail.map(|d| View(d.into()))));
                });
            }
            Input::Show(None) => widgets::empty(&self.body),
            Input::Show(Some(View(detail))) => self.draw(&detail, &sender),
            Input::AskRelease => self.ask_release(&sender),
            Input::Released => {
                if let Some(tx) = self.tx.clone() {
                    sender.input(Input::Open(tx, self.screens.clone()));
                }
            }
            Input::Failed(why) => {
                let _ = sender.output(RootMsg::Say(why));
            }
            Input::CopyTx => {
                if let Some(tx) = &self.tx {
                    widgets::copy(&self.body, tx);
                    let _ = sender.output(RootMsg::Say("Copied".into()));
                }
            }
        }
    }
}

impl Payment {
    fn ask_release(&self, sender: &ComponentSender<Self>) {
        let Some(tx) = self.tx.clone() else { return };
        let dialog = adw::AlertDialog::builder().heading("Release these coins?").body("You can spend them again. If this payment does go through after all, Fob shows it at its next scan, and a later payment using the same coins is refused. Check with whoever you were paying first.").build();
        dialog.add_responses(&[("cancel", "_Cancel"), ("release", "_Release")]);
        dialog.set_close_response("cancel");
        dialog.set_response_appearance("release", adw::ResponseAppearance::Destructive);
        let (engine, sender) = (self.engine.clone(), sender.clone());
        dialog.connect_response(Some("release"), move |_, _| {
            let (engine, tx, sender) = (engine.clone(), tx.clone(), sender.clone());
            relm4::spawn(async move {
                match engine.change(move |wallet| async move { wallet.release_payment(tx).await }).await {
                    Ok(()) => sender.input(Input::Released),
                    Err(e) => sender.input(Input::Failed(e.to_string())),
                }
            });
        });
        dialog.present(Some(&self.body));
    }

    fn draw(&self, detail: &PaymentDetail, sender: &ComponentSender<Self>) {
        widgets::empty(&self.body);
        let big = gtk::Label::builder().label(&detail.amount_text).selectable(true).build();
        big.add_css_class("balance");
        self.body.append(&big);
        self.body.append(&widgets::dim(&detail.when));
        let list = widgets::list();
        let row = |title: &str, value: &str| {
            if value.is_empty() {
                return;
            }
            let view = widgets::row(title, value);
            view.add_css_class("property");
            list.append(&view);
        };
        row("Status", &detail.status);
        row("Exact amount", &detail.amount_exact);
        if detail.priced {
            row("Worth now", &detail.worth_now);
            row(&detail.then_label, &detail.worth_then);
            row("Since then", detail.change.as_deref().unwrap_or(""));
        }
        row(&detail.party_label, detail.party.as_deref().unwrap_or(""));
        row(&detail.pieces_label, detail.pieces.as_deref().unwrap_or(""));
        row("Fee", detail.fee.as_deref().unwrap_or(""));
        let tx = widgets::row("Transaction", &detail.tx_short);
        tx.set_activatable(true);
        tx.add_css_class("property");
        tx.add_suffix(&gtk::Image::from_icon_name("edit-copy-symbolic"));
        let then = sender.clone();
        tx.connect_activated(move |_| then.input(Input::CopyTx));
        list.append(&tx);
        self.body.append(&list);
        if let Some(View(screens)) = &self.screens
            && let Some(receipt) = screens.activity.iter().flat_map(|day| &day.rows).filter_map(|row| row.activity.as_ref()).find(|a| a.tx == detail.tx).and_then(|a| a.receipt.as_ref())
        {
            self.body.append(&widgets::receipt(receipt));
        }
        if detail.can_release {
            let group = adw::PreferencesGroup::builder().title("Not confirmed").description("Your node has no record of this payment a day after it was sent, and its coins aren't spent on the chain as far as it says. It may not have gone through, but a node can be wrong: ask whoever you paid before paying again. Its coins stay held until you release them.").build();
            let release = gtk::Button::builder().label("Release coins").halign(gtk::Align::Center).css_classes(["pill"]).margin_top(12).build();
            let then = sender.clone();
            release.connect_clicked(move |_| then.input(Input::AskRelease));
            self.body.append(&group);
            self.body.append(&release);
        }
        if let Some(footer) = &detail.footer {
            self.body.append(&widgets::dim(footer));
        }
    }
}
