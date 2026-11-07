//! Meet: both people end up with each other. You show your code, they read it
//! and show theirs back. A desktop reads a code by pasting or from a picture;
//! touching phones is a phone's way, and only a phone's.

use fob_app::{
    Engine, Met,
    views::{FobError, ProfileShare, ScannedProfile, ShareKind},
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
    Opened,
    Share(View<ProfileShare>),
    CopyLink,
    Paste,
    Picture,
    Take(String),
    /// Asks about a code that would change a contact: from Meet, or not.
    Ask(String, View<ScannedProfile>, bool),
    /// Saved, by this name. A reply means they have your code already.
    Saved(String, bool),
    Reply(String, String),
    Finished(String),
    /// A profile that came from elsewhere.
    Scanned(String),
    Found(String, View<ScannedProfile>),
    Failed(String),
}

#[derive(PartialEq)]
enum Stage {
    Show,
    /// Showing them our answer, or done: only Done is left to press.
    Over,
}

pub struct Meet {
    engine: Engine,
    share: Option<View<ProfileShare>>,
    stage: Stage,
    error: Option<String>,
    body: gtk::Box,
}

#[relm4::component(pub)]
impl SimpleComponent for Meet {
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
                        body -> gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 12,
                        },
                        gtk::Label {
                            add_css_class: "error",
                            set_wrap: true,
                            #[watch]
                            set_visible: model.error.is_some(),
                            #[watch]
                            set_label: model.error.as_deref().unwrap_or(""),
                        },
                        gtk::Box {
                            set_halign: gtk::Align::Center,
                            set_spacing: 12,
                            #[watch]
                            set_visible: model.stage == Stage::Show,
                            gtk::Button {
                                set_child: Some(&widgets::icon_label(crate::icons::LINK, "Copy my link", false)),
                                add_css_class: "pill",
                                connect_clicked => Input::CopyLink,
                            },
                            gtk::Button {
                                set_child: Some(&widgets::icon_label(crate::icons::REQUEST, "Paste their code", false)),
                                add_css_class: "pill",
                                add_css_class: "suggested-action",
                                connect_clicked => Input::Paste,
                            },
                            gtk::Button {
                                set_child: Some(&widgets::icon_label(crate::icons::PICTURE, "From a picture", false)),
                                add_css_class: "pill",
                                connect_clicked => Input::Picture,
                            },
                        },
                        gtk::Button {
                            set_child: Some(&widgets::icon_label(crate::icons::CHECK, "Done", false)),
                            add_css_class: "pill",
                            add_css_class: "suggested-action",
                            set_halign: gtk::Align::Center,
                            #[watch]
                            set_visible: model.stage == Stage::Over,
                            connect_clicked[sender] => move |_| { let _ = sender.output(RootMsg::Go(Go::Back)); },
                        },
                    },
                },
            },
        }
    }

    fn init(engine: Engine, root: Self::Root, sender: ComponentSender<Self>) -> ComponentParts<Self> {
        let body = gtk::Box::new(gtk::Orientation::Vertical, 12);
        let model = Meet { engine, share: None, stage: Stage::Show, error: None, body: body.clone() };
        let body = &body;
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, input: Input, sender: ComponentSender<Self>) {
        match input {
            Input::Opened => {
                self.stage = Stage::Show;
                self.error = None;
                self.share = None;
                widgets::empty(&self.body);
                let (engine, sender) = (self.engine.clone(), sender.clone());
                relm4::spawn(async move {
                    match engine.change(|wallet| async move { wallet.share_profile().await }).await {
                        Ok(share) => sender.input(Input::Share(View(share.into()))),
                        Err(e) => sender.input(Input::Failed(e.to_string())),
                    }
                });
            }
            Input::Share(share) => {
                self.draw_mine(&share.0);
                self.share = Some(share);
            }
            Input::CopyLink => {
                let Some(View(share)) = &self.share else { return };
                widgets::copy(&self.body, &share.code);
                let (engine, id) = (self.engine.clone(), share.id);
                relm4::spawn(async move {
                    let _ = engine.change(move |wallet| async move { wallet.share_went_out(id, ShareKind::LinkCopied).await }).await;
                });
                let _ = sender.output(RootMsg::Say("Copied".into()));
            }
            Input::Paste => {
                let then = sender.clone();
                widgets::paste(&self.body, move |text| match text {
                    Some(text) => then.input(Input::Take(text)),
                    None => then.input(Input::Failed(fob_app::words::errors::nothing_to_paste())),
                });
            }
            Input::Picture => {
                let then = sender.clone();
                widgets::read_code_from_picture(self.body.upcast_ref(), move |text| match text {
                    Some(text) => then.input(Input::Take(text)),
                    None => then.input(Input::Failed(fob_app::words::errors::no_code_in_picture())),
                });
            }
            Input::Take(text) => {
                self.error = None;
                let (engine, sender) = (self.engine.clone(), sender.clone());
                relm4::spawn(async move {
                    match engine.meet_save(text.clone()).await {
                        Ok(Met::Saved(contact, found)) => sender.input(Input::Saved(contact.name, found.reply)),
                        Ok(Met::Ask(found)) => sender.input(Input::Ask(text, View(found.into()), true)),
                        Err(e) => sender.input(Input::Failed(e.to_string())),
                    }
                });
            }
            Input::Ask(text, View(found), met) => self.ask_clash(&text, &found, met, &sender),
            Input::Saved(name, reply) => self.saved(name, reply, &sender),
            Input::Reply(name, code) => {
                self.stage = Stage::Over;
                widgets::empty(&self.body);
                self.body.append(&widgets::heading(&format!("Saved {name}. Now show them this")));
                if let Some(qr) = widgets::qr(&code, 300) {
                    self.body.append(&qr);
                }
                self.body.append(&widgets::centred("It's the same address, marked as your answer, so they know you have theirs."));
            }
            Input::Finished(name) => self.finish(&name),
            Input::Scanned(text) => {
                let (engine, sender) = (self.engine.clone(), sender.clone());
                relm4::spawn(async move {
                    let asked = text.clone();
                    match engine.read(move |wallet, _| async move { wallet.scanned_profile(asked).await }).await {
                        Some(Ok(found)) => sender.input(Input::Found(text, View(found.into()))),
                        Some(Err(e)) => sender.input(Input::Failed(e.to_string())),
                        None => {}
                    }
                });
            }
            Input::Found(text, View(found)) => self.show_found(&text, &found, &sender),
            Input::Failed(why) => {
                self.error = Some(why.clone());
                let _ = sender.output(RootMsg::Say(why));
            }
        }
    }
}

impl Meet {
    fn draw_mine(&self, share: &ProfileShare) {
        widgets::empty(&self.body);
        self.body.append(&widgets::heading("Show them your code"));
        if let Some(qr) = widgets::qr(&share.code, 300) {
            self.body.append(&qr);
        }
        self.body.append(&widgets::centred("They scan this with Fob, or you send them your link, then they show you theirs. Read a picture of it here, or paste its link."));
    }

    /// Theirs is saved: answered if they still need ours, or met.
    fn saved(&mut self, name: String, reply: bool, sender: &ComponentSender<Self>) {
        let Some(View(share)) = self.share.clone() else {
            self.finish(&name);
            return;
        };
        let (id, engine, then) = (share.id, self.engine.clone(), sender.clone());
        relm4::spawn(async move {
            let met = name.clone();
            let _ = engine.change(move |wallet| async move { wallet.share_met(id, met).await }).await;
            let answer = if reply { None } else { engine.read(move |wallet, _| async move { wallet.share_reply_code(id).await }).await.flatten() };
            then.input(match answer {
                Some(code) => Input::Reply(name, code),
                None => Input::Finished(name),
            });
        });
    }

    fn finish(&mut self, name: &str) {
        widgets::empty(&self.body);
        let page = adw::StatusPage::builder().icon_name("emblem-ok-symbolic").title(format!("Met {name}")).description("They're in your contacts, and you're in theirs.").build();
        page.add_css_class("compact");
        self.body.append(&page);
        self.stage = Stage::Over;
    }

    /// Someone's profile from a link or a code: pay them, or keep them.
    fn show_found(&self, text: &str, found: &ScannedProfile, sender: &ComponentSender<Self>) {
        let column = gtk::Box::new(gtk::Orientation::Vertical, 8);
        let avatar = widgets::avatar(64, &found.name, found.photo.as_deref());
        avatar.set_halign(gtk::Align::Center);
        column.append(&avatar);
        column.append(&widgets::pattern(&found.address));
        column.append(&widgets::address(&found.address));
        let name = if found.name.is_empty() { "A Fob profile" } else { found.name.as_str() };
        let dialog = adw::AlertDialog::builder().heading(name).body(&found.note).extra_child(&column).build();
        dialog.add_responses(&[("close", "_Close"), ("add", &found.add), ("pay", &found.pay)]);
        dialog.set_close_response("close");
        dialog.set_response_appearance("pay", adw::ResponseAppearance::Suggested);
        let (address, then, text, engine) = (found.address.clone(), sender.clone(), text.to_owned(), self.engine.clone());
        let saving = sender.clone();
        dialog.connect_response(Some("pay"), move |_, _| {
            let _ = then.output(RootMsg::Go(Go::SendFrom(address.clone())));
        });
        dialog.connect_response(Some("add"), move |_, _| {
            let (engine, sender, text) = (engine.clone(), saving.clone(), text.clone());
            relm4::spawn(async move { add_from_link(engine, sender, text).await });
        });
        dialog.present(relm4::main_application().active_window().as_ref());
    }

    /// It would change a contact, or it's a name someone has: only ever asked.
    fn ask_clash(&self, text: &str, found: &ScannedProfile, met: bool, sender: &ComponentSender<Self>) {
        let Some(clash) = &found.clash else { return };
        let list = widgets::list();
        for change in &clash.changes {
            let row = widgets::row(&change.label, &format!("{} → {}", change.now, change.new));
            row.add_css_class("property");
            list.append(&row);
        }
        let dialog = adw::AlertDialog::builder().heading(&clash.why).body(&clash.ask).extra_child(&list).build();
        dialog.add_responses(&[("keep", &clash.keep), ("update", &clash.update)]);
        dialog.set_close_response("keep");
        dialog.set_response_appearance("update", adw::ResponseAppearance::Suggested);
        if let Some(add_new) = &clash.add_new {
            dialog.add_response("new", add_new);
        }
        let (id, reply) = (clash.contact.id, found.reply);
        let (engine, then, text) = (self.engine.clone(), sender.clone(), text.to_owned());
        dialog.connect_response(None, move |_, response| {
            let as_new = match response {
                "update" => false,
                "new" => true,
                _ => return,
            };
            let (engine, then, text) = (engine.clone(), then.clone(), text.clone());
            relm4::spawn(async move {
                let saved = engine
                    .change(move |wallet| async move {
                        if as_new { wallet.add_contact_from_profile(text, true, met).await } else { wallet.update_contact_from_profile(id, text, met).await }
                    })
                    .await;
                match saved {
                    Ok(contact) if met => then.input(Input::Saved(contact.name, reply)),
                    Ok(contact) => {
                        let _ = then.output(RootMsg::Say(format!("Saved {}", contact.name)));
                    }
                    Err(e) => then.input(Input::Failed(e.to_string())),
                }
            });
        });
        dialog.present(relm4::main_application().active_window().as_ref());
    }
}

/// Adding from a link: they don't have this wallet's address, so the contact
/// is half set up. A clash is asked about.
async fn add_from_link(engine: Engine, sender: ComponentSender<Meet>, text: String) {
    let asked = text.clone();
    let added: Result<fob_app::views::Contact, FobError> = engine.change(move |wallet| async move { wallet.add_contact_from_profile(asked, false, false).await }).await;
    match added {
        Ok(contact) => {
            let _ = sender.output(RootMsg::Say(format!("Saved {}", contact.name)));
        }
        Err(refused) => {
            let again = text.clone();
            let found = engine.read(move |wallet, _| async move { wallet.scanned_profile(again).await }).await;
            match found {
                Some(Ok(found)) if found.clash.is_some() => sender.input(Input::Ask(text, View(found.into()), false)),
                _ => sender.input(Input::Failed(refused.to_string())),
            }
        }
    }
}
