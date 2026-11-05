//! A request: no edits, can be deleted. What was asked, what its address got,
//! and the code again. Every word about it is from Rust.

use fob_app::{
    Engine,
    views::{ReceiveCode, RequestDetail},
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

#[derive(Debug)]
pub enum Input {
    Open(u64),
    /// Payments may have come: read it again.
    Refresh,
    Show(u64, Option<View<RequestDetail>>),
    ShowCode,
    Code(View<ReceiveCode>),
    Delete,
    Deleted,
    Failed(String),
}

pub struct Request {
    engine: Engine,
    id: Option<u64>,
    detail: Option<View<RequestDetail>>,
    body: gtk::Box,
}

#[relm4::component(pub)]
impl SimpleComponent for Request {
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
                        gtk::Box {
                            set_halign: gtk::Align::Center,
                            set_spacing: 12,
                            gtk::Button {
                                set_label: "Show the code again",
                                add_css_class: "pill",
                                connect_clicked => Input::ShowCode,
                            },
                            gtk::Button {
                                set_label: "Delete request",
                                add_css_class: "pill",
                                add_css_class: "destructive-action",
                                connect_clicked => Input::Delete,
                            },
                        },
                    },
                },
            },
        }
    }

    fn init(engine: Engine, root: Self::Root, sender: ComponentSender<Self>) -> ComponentParts<Self> {
        let body = gtk::Box::new(gtk::Orientation::Vertical, 18);
        let model = Request { engine, id: None, detail: None, body: body.clone() };
        let body = &body;
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, input: Input, sender: ComponentSender<Self>) {
        match input {
            Input::Open(id) => {
                self.id = Some(id);
                self.read(id, &sender);
            }
            Input::Refresh => {
                if let Some(id) = self.id {
                    self.read(id, &sender);
                }
            }
            Input::Show(id, detail) => {
                if self.id != Some(id) {
                    return;
                }
                match &detail {
                    Some(View(shown)) => self.draw(shown, &sender),
                    None => widgets::empty(&self.body),
                }
                self.detail = detail;
            }
            Input::ShowCode => {
                let Some(id) = self.id else { return };
                let (engine, sender) = (self.engine.clone(), sender.clone());
                relm4::spawn(async move {
                    if let Some(Some(code)) = engine.read(move |wallet, _| async move { wallet.request_code(id).await }).await {
                        sender.input(Input::Code(View(code.into())));
                    }
                });
            }
            Input::Code(View(code)) => widgets::show_code(self.body.upcast_ref(), code.headline.as_deref().unwrap_or("Request"), code.detail.as_deref().unwrap_or(""), &code.uri),
            Input::Delete => self.ask_delete(&sender),
            Input::Deleted => {
                self.id = None;
                self.detail = None;
                let _ = sender.output(RootMsg::Go(Go::Back));
            }
            Input::Failed(why) => {
                let _ = sender.output(RootMsg::Say(why));
            }
        }
    }
}

impl Request {
    fn read(&self, id: u64, sender: &ComponentSender<Self>) {
        let (engine, sender) = (self.engine.clone(), sender.clone());
        relm4::spawn(async move {
            let detail = engine.read(move |wallet, clock| async move { wallet.request(id, clock.now, clock.utc_offset, clock.hours_24).await }).await.flatten();
            sender.input(Input::Show(id, detail.map(|d| View(d.into()))));
        });
    }

    fn draw(&self, detail: &RequestDetail, sender: &ComponentSender<Self>) {
        widgets::empty(&self.body);
        let big = gtk::Label::builder().label(&detail.headline).wrap(true).justify(gtk::Justification::Center).build();
        big.add_css_class("balance");
        self.body.append(&big);
        self.body.append(&widgets::centred(&detail.asked_line));
        self.body.append(&widgets::centred(&detail.when));

        let list = widgets::list();
        let received = widgets::row(&detail.received_label, &detail.received);
        received.add_css_class("property");
        list.append(&received);
        let address = widgets::row(&detail.address_label, &detail.address_short);
        address.add_css_class("property");
        address.add_suffix(&widgets::pattern(&detail.address));
        list.append(&address);
        self.body.append(&list);

        let payments = adw::PreferencesGroup::builder().title("Payments to it").build();
        if let Some(none) = &detail.payments_empty {
            payments.set_description(Some(gtk::glib::markup_escape_text(none).as_str()));
        }
        for row in &detail.payments {
            let then = sender.clone();
            payments.add(&widgets::activity_row(row, move |go| {
                let _ = then.output(RootMsg::Go(go));
            }));
        }
        self.body.append(&payments);
    }

    fn ask_delete(&self, sender: &ComponentSender<Self>) {
        let (Some(id), Some(View(detail))) = (self.id, &self.detail) else { return };
        let dialog = adw::AlertDialog::builder().heading(&detail.delete_title).body(&detail.delete_body).build();
        dialog.add_responses(&[("keep", "_Keep"), ("delete", "_Delete")]);
        dialog.set_close_response("keep");
        dialog.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
        let (engine, sender) = (self.engine.clone(), sender.clone());
        dialog.connect_response(Some("delete"), move |_, _| {
            let (engine, sender) = (engine.clone(), sender.clone());
            relm4::spawn(async move {
                match engine.change(move |wallet| async move { wallet.delete_request(id).await }).await {
                    Ok(()) => sender.input(Input::Deleted),
                    Err(e) => sender.input(Input::Failed(e.to_string())),
                }
            });
        });
        dialog.present(Some(&self.body));
    }
}
