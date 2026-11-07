//! Your profile: the name and photo others get, and each time it went out.

use fob_app::{
    Engine,
    views::{Profile as ProfileView, ProfileShare, ShareKind, ShareLine},
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
    /// A share may have got a payment.
    Refresh,
    Profile(Option<View<ProfileView>>),
    Shares(View<Vec<ShareLine>>),
    Save,
    NameInRequests(bool),
    ChoosePhoto,
    Photo(Option<Vec<u8>>),
    Share,
    ShowShare(u64),
    Shared(View<ProfileShare>),
    Failed(String),
}

pub struct Profile {
    engine: Engine,
    photo: Option<Vec<u8>>,
    has_profile: bool,
    filling: bool,
    name: adw::EntryRow,
    in_requests: adw::SwitchRow,
    face: gtk::Box,
    shares: gtk::ListBox,
}

#[relm4::component(pub)]
impl SimpleComponent for Profile {
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
                        gtk::Button {
                            add_css_class: "flat",
                            add_css_class: "circular",
                            set_halign: gtk::Align::Center,
                            set_tooltip_text: Some("Choose a photo"),
                            connect_clicked => Input::ChoosePhoto,
                            #[local_ref]
                            #[wrap(Some)]
                            set_child = face -> gtk::Box {},
                        },
                        adw::PreferencesGroup {
                            #[local_ref]
                            add = name -> adw::EntryRow {
                                set_title: "Your name",
                                set_show_apply_button: true,
                                connect_apply => Input::Save,
                            },
                            #[local_ref]
                            add = in_requests -> adw::SwitchRow {
                                set_title: "Name in payment requests",
                                connect_active_notify[sender] => move |row| sender.input(Input::NameInRequests(row.is_active())),
                            },
                        },
                        gtk::Box {
                            set_halign: gtk::Align::Center,
                            set_spacing: 12,
                            gtk::Button {
                                set_child: Some(&widgets::icon_label(crate::icons::MEET, "Meet someone", false)),
                                add_css_class: "pill",
                                add_css_class: "suggested-action",
                                #[watch]
                                set_sensitive: model.has_profile,
                                connect_clicked[sender] => move |_| { let _ = sender.output(RootMsg::Go(Go::Meet)); },
                            },
                            gtk::Button {
                                set_child: Some(&widgets::icon_label(crate::icons::SHARE, "Share my profile", false)),
                                add_css_class: "pill",
                                #[watch]
                                set_sensitive: model.has_profile,
                                connect_clicked => Input::Share,
                            },
                        },
                        adw::PreferencesGroup {
                            set_title: "Your shares",
                            #[local_ref]
                            add = shares -> gtk::ListBox {
                                add_css_class: "boxed-list",
                                set_selection_mode: gtk::SelectionMode::None,
                            },
                        },
                    },
                },
            },
        }
    }

    fn init(engine: Engine, root: Self::Root, sender: ComponentSender<Self>) -> ComponentParts<Self> {
        let (name, in_requests) = (adw::EntryRow::new(), adw::SwitchRow::new());
        let (face, shares) = (gtk::Box::new(gtk::Orientation::Horizontal, 0), gtk::ListBox::new());
        let model = Profile { engine, photo: None, has_profile: false, filling: false, name: name.clone(), in_requests: in_requests.clone(), face: face.clone(), shares: shares.clone() };
        let (name, in_requests, face, shares) = (&name, &in_requests, &face, &shares);
        let widgets = view_output!();
        model.draw_face();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, input: Input, sender: ComponentSender<Self>) {
        match input {
            Input::Opened => self.read(&sender),
            Input::Refresh => self.read_shares(&sender),
            Input::Profile(profile) => {
                self.filling = true;
                self.has_profile = profile.is_some();
                self.photo = profile.as_ref().and_then(|View(p)| p.photo.clone());
                self.name.set_text(profile.as_ref().map_or("", |View(p)| p.name.as_str()));
                self.in_requests.set_active(profile.as_ref().is_some_and(|View(p)| p.name_in_requests));
                self.in_requests.set_sensitive(profile.is_some());
                self.filling = false;
                self.draw_face();
            }
            Input::Shares(View(lines)) => self.draw_shares(&lines, &sender),
            Input::Save => self.save(&sender),
            Input::NameInRequests(_) if self.filling => {}
            Input::NameInRequests(on) => {
                let (engine, sender) = (self.engine.clone(), sender.clone());
                relm4::spawn(async move {
                    if let Err(e) = engine.change(move |wallet| async move { wallet.set_name_in_requests(on).await }).await {
                        sender.input(Input::Failed(e.to_string()));
                    }
                });
            }
            Input::ChoosePhoto => self.ask_photo(&sender),
            Input::Photo(photo) => {
                self.photo = photo;
                self.draw_face();
                if !self.name.text().trim().is_empty() {
                    self.save(&sender);
                }
            }
            Input::Share => {
                let (engine, sender) = (self.engine.clone(), sender.clone());
                relm4::spawn(async move {
                    match engine.change(|wallet| async move { wallet.share_profile().await }).await {
                        Ok(share) => sender.input(Input::Shared(View(share.into()))),
                        Err(e) => sender.input(Input::Failed(e.to_string())),
                    }
                });
            }
            Input::ShowShare(id) => {
                let (engine, sender) = (self.engine.clone(), sender.clone());
                relm4::spawn(async move {
                    if let Some(Some(share)) = engine.read(move |wallet, _| async move { wallet.share_code(id).await }).await {
                        sender.input(Input::Shared(View(share.into())));
                    }
                });
            }
            Input::Shared(View(share)) => self.show_share(&share, &sender),
            Input::Failed(why) => {
                let _ = sender.output(RootMsg::Say(why));
            }
        }
    }
}

impl Profile {
    fn read(&self, sender: &ComponentSender<Self>) {
        let (engine, then) = (self.engine.clone(), sender.clone());
        relm4::spawn(async move {
            if let Some(profile) = engine.read(|wallet, _| async move { wallet.profile().await }).await {
                then.input(Input::Profile(profile.map(|p| View(p.into()))));
            }
        });
        self.read_shares(sender);
    }

    fn read_shares(&self, sender: &ComponentSender<Self>) {
        let (engine, sender) = (self.engine.clone(), sender.clone());
        relm4::spawn(async move {
            if let Some(lines) = engine.read(|wallet, clock| async move { wallet.profile_shares(clock.now, clock.utc_offset, clock.hours_24).await }).await {
                sender.input(Input::Shares(View(lines.into())));
            }
        });
    }

    fn save(&self, sender: &ComponentSender<Self>) {
        let (engine, name, photo, sender) = (self.engine.clone(), self.name.text().to_string(), self.photo.clone(), sender.clone());
        relm4::spawn(async move {
            match engine.change(move |wallet| async move { wallet.set_profile(name, photo).await }).await {
                Ok(saved) => sender.input(Input::Profile(Some(View(saved.into())))),
                Err(e) => sender.input(Input::Failed(e.to_string())),
            }
        });
    }

    fn draw_face(&self) {
        widgets::empty(&self.face);
        self.face.append(&widgets::avatar(96, &self.name.text(), self.photo.as_deref()));
    }

    fn draw_shares(&self, lines: &[ShareLine], sender: &ComponentSender<Self>) {
        widgets::clear(&self.shares);
        if lines.is_empty() {
            self.shares.append(&widgets::row("Not shared yet", "Each time it goes out shows here, with what came to it."));
        }
        for line in lines {
            let row = widgets::row(&line.title, &line.when);
            let received = gtk::Label::new(Some(&line.received));
            if !line.any {
                received.add_css_class("dim-label");
            }
            row.add_suffix(&received);
            row.set_activatable(true);
            let (id, then) = (line.id, sender.clone());
            row.connect_activated(move |_| then.input(Input::ShowShare(id)));
            self.shares.append(&row);
        }
    }

    fn ask_photo(&self, sender: &ComponentSender<Self>) {
        if self.photo.is_none() {
            return self.pick_photo(sender);
        }
        let dialog = adw::AlertDialog::builder().heading("Your photo").build();
        dialog.add_responses(&[("cancel", "_Cancel"), ("remove", "_Remove"), ("choose", "_Choose another")]);
        dialog.set_close_response("cancel");
        dialog.set_response_appearance("remove", adw::ResponseAppearance::Destructive);
        let (then, face, gone) = (sender.clone(), self.face.clone(), sender.clone());
        dialog.connect_response(Some("choose"), move |_, _| pick(&face, &then));
        dialog.connect_response(Some("remove"), move |_, _| gone.input(Input::Photo(None)));
        dialog.present(Some(&self.face));
    }

    fn pick_photo(&self, sender: &ComponentSender<Self>) {
        pick(&self.face, sender);
    }

    fn show_share(&self, share: &ProfileShare, sender: &ComponentSender<Self>) {
        let column = gtk::Box::new(gtk::Orientation::Vertical, 12);
        if let Some(qr) = widgets::qr(&share.code, 300) {
            column.append(&qr);
        }
        let dialog = adw::AlertDialog::builder().heading("Your profile").body("Anyone with this code can pay you at this new address and add you as a contact.").extra_child(&column).build();
        dialog.add_responses(&[("close", "_Close"), ("copy", "_Copy link")]);
        dialog.set_close_response("close");
        let (engine, id, code, face, shown_engine) = (self.engine.clone(), share.id, share.code.clone(), self.face.clone(), self.engine.clone());
        let then = sender.clone();
        dialog.connect_response(Some("copy"), move |_, _| {
            widgets::copy(&face, &code);
            went_out(&engine, id, ShareKind::LinkCopied);
            let _ = then.output(RootMsg::Say("Copied".into()));
        });
        dialog.present(Some(&self.face));
        went_out(&shown_engine, id, ShareKind::CodeShown);
    }
}

fn pick(parent: &gtk::Box, sender: &ComponentSender<Profile>) {
    let then = sender.clone();
    widgets::choose_photo_then(parent.upcast_ref(), move |picked| match picked {
        Ok(photo) => then.input(Input::Photo(Some(photo))),
        Err(why) => then.input(Input::Failed(why)),
    });
}

fn went_out(engine: &Engine, id: u64, kind: ShareKind) {
    let engine = engine.clone();
    relm4::spawn(async move {
        let _ = engine.change(move |wallet| async move { wallet.share_went_out(id, kind).await }).await;
    });
}
