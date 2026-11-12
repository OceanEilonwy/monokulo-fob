//! Contacts: everyone by name, their card, and adding or changing one.

use fob_app::{
    Engine,
    views::{Contact, ContactCard, ContactDraft, ContactRow, PaidBefore, Screens},
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
    Screens(View<Screens>),
    Opened,
    Search,
    Found(View<Vec<ContactRow>>),
    Open(u64),
    Card(u64, View<ContactCard>),
    Add,
    Edit(u64),
    Editor(View<Contact>),
    /// New, filled as a payment to this address would.
    Unsaved(String),
    Draft(View<ContactDraft>),
    Save(Option<u64>, View<ContactDraft>),
    ChoosePhoto(Option<u64>, View<ContactDraft>),
    Delete(u64),
    Pay(String),
    Request(u64),
    Failed(String),
}

pub struct Contacts {
    engine: Engine,
    search: gtk::SearchEntry,
    list: gtk::ListBox,
    unsaved: gtk::ListBox,
}

#[relm4::component(pub)]
impl SimpleComponent for Contacts {
    type Init = Engine;
    type Input = Input;
    type Output = RootMsg;

    view! {
        adw::ToolbarView {
            add_top_bar = &adw::HeaderBar {
                pack_start = &gtk::Button {
                    set_child: Some(&widgets::icon_label(crate::icons::MEET, "Meet", false)),
                    set_tooltip_text: Some("Meet someone: both of you end up with each other"),
                    connect_clicked[sender] => move |_| { let _ = sender.output(RootMsg::Go(Go::Meet)); },
                },
                pack_end = &gtk::Button {
                    set_icon_name: "list-add-symbolic",
                    set_tooltip_text: Some("Add a contact"),
                    connect_clicked => Input::Add,
                },
            },
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
                        search -> gtk::SearchEntry {
                            set_placeholder_text: Some("Search names and notes"),
                            connect_search_changed => Input::Search,
                        },
                        #[local_ref]
                        list -> gtk::ListBox {
                            add_css_class: "boxed-list",
                            set_selection_mode: gtk::SelectionMode::None,
                            set_valign: gtk::Align::Start,
                        },
                        adw::PreferencesGroup {
                            #[watch]
                            set_visible: model.unsaved.first_child().is_some(),
                            set_title: "Paid, not saved",
                            #[local_ref]
                            add = unsaved -> gtk::ListBox {
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
        let (search, list, unsaved) = (gtk::SearchEntry::new(), gtk::ListBox::new(), gtk::ListBox::new());
        let model = Contacts { engine, search: search.clone(), list: list.clone(), unsaved: unsaved.clone() };
        let (search, list, unsaved) = (&search, &list, &unsaved);
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, input: Input, sender: ComponentSender<Self>) {
        match input {
            Input::Screens(View(screens)) => {
                if self.search.text().is_empty() {
                    self.draw(&screens.contacts, &sender);
                }
                self.draw_unsaved(&screens.unsaved, &sender);
            }
            Input::Opened | Input::Search => self.ask(&sender),
            Input::Found(View(rows)) => self.draw(&rows, &sender),
            Input::Open(id) => {
                let (engine, sender) = (self.engine.clone(), sender.clone());
                relm4::spawn(async move {
                    let card = engine.read(move |wallet, clock| async move { wallet.contact_card(id, clock.now, clock.utc_offset, clock.hours_24).await }).await.flatten();
                    if let Some(card) = card {
                        sender.input(Input::Card(id, View(card.into())));
                    }
                });
            }
            Input::Card(id, View(card)) => self.show_card(id, &card, &sender),
            Input::Add => self.show_editor(None, std::sync::Arc::new(ContactDraft { name: String::new(), note: None, photo: None, send_address: None, also_paid_at: vec![] }), &sender),
            Input::Unsaved(address) => {
                let (engine, sender) = (self.engine.clone(), sender.clone());
                relm4::spawn(async move {
                    if let Some(draft) = engine.read(move |wallet, _| async move { wallet.new_contact_for(address).await }).await {
                        sender.input(Input::Draft(View(draft.into())));
                    }
                });
            }
            Input::Draft(View(draft)) => self.show_editor(None, draft, &sender),
            Input::Edit(id) => {
                let (engine, sender) = (self.engine.clone(), sender.clone());
                relm4::spawn(async move {
                    let contact = engine.read(move |wallet, _| async move { wallet.contact(id).await }).await.flatten();
                    if let Some(contact) = contact {
                        sender.input(Input::Editor(View(contact.into())));
                    }
                });
            }
            Input::Editor(View(contact)) => {
                let draft = ContactDraft { name: contact.name.clone(), note: contact.note.clone(), photo: contact.photo.clone(), send_address: contact.send_address.clone(), also_paid_at: contact.also_paid_at.clone() };
                self.show_editor(Some(contact.id), std::sync::Arc::new(draft), &sender);
            }
            Input::Save(id, View(draft)) => self.save(id, copy(&draft), &sender),
            Input::ChoosePhoto(id, View(draft)) => {
                // What's typed so far comes back with the photo.
                pick_photo(self.list.upcast_ref(), id, draft, sender.clone());
            }
            Input::Delete(id) => {
                let (engine, sender) = (self.engine.clone(), sender.clone());
                relm4::spawn(async move {
                    if let Err(e) = engine.change(move |wallet| async move { wallet.delete_contact(id).await }).await {
                        sender.input(Input::Failed(e.to_string()));
                    }
                });
            }
            Input::Pay(address) => {
                let _ = sender.output(RootMsg::Go(Go::SendFrom(address)));
            }
            Input::Request(id) => {
                let _ = sender.output(RootMsg::Go(Go::Request(id)));
            }
            Input::Failed(why) => {
                let _ = sender.output(RootMsg::Say(why));
            }
        }
    }
}

fn copy(draft: &ContactDraft) -> ContactDraft {
    ContactDraft { name: draft.name.clone(), note: draft.note.clone(), photo: draft.photo.clone(), send_address: draft.send_address.clone(), also_paid_at: draft.also_paid_at.clone() }
}

fn pick_photo(parent: &gtk::Widget, id: Option<u64>, draft: std::sync::Arc<ContactDraft>, sender: ComponentSender<Contacts>) {
    widgets::choose_photo_then(parent, move |picked| match picked {
        Ok(photo) => {
            let mut draft = copy(&draft);
            draft.photo = Some(photo);
            match id {
                Some(id) => sender.input(Input::Save(Some(id), View(draft.into()))),
                None => sender.input(Input::Draft(View(draft.into()))),
            }
        }
        Err(why) => sender.input(Input::Failed(why)),
    });
}

impl Contacts {
    fn ask(&self, sender: &ComponentSender<Self>) {
        let (engine, search, sender) = (self.engine.clone(), self.search.text().to_string(), sender.clone());
        relm4::spawn(async move {
            let rows = engine.read(move |wallet, clock| async move { wallet.contacts(search, clock.now, clock.utc_offset, clock.hours_24).await }).await;
            if let Some(rows) = rows {
                sender.input(Input::Found(View(rows.into())));
            }
        });
    }

    fn draw(&self, rows: &[ContactRow], sender: &ComponentSender<Self>) {
        widgets::clear(&self.list);
        if rows.is_empty() {
            self.list.append(&widgets::row("No contacts", "People you pay show here once you add them."));
        }
        for contact in rows {
            let row = widgets::row(&contact.name, &contact.line);
            row.add_prefix(&widgets::contact_avatar(&self.engine, 32, &contact.name, contact.id, contact.photo_key.as_deref()));
            row.set_activatable(true);
            let (id, sender) = (contact.id, sender.clone());
            row.connect_activated(move |_| sender.input(Input::Open(id)));
            self.list.append(&row);
        }
    }

    fn draw_unsaved(&self, unsaved: &[PaidBefore], sender: &ComponentSender<Self>) {
        widgets::clear(&self.unsaved);
        for paid in unsaved {
            let row = widgets::row(&paid.name, &paid.line);
            row.set_activatable(true);
            let (address, sender) = (paid.address.clone(), sender.clone());
            row.connect_activated(move |_| sender.input(Input::Unsaved(address.clone())));
            self.unsaved.append(&row);
        }
    }

    fn show_card(&self, id: u64, card: &ContactCard, sender: &ComponentSender<Self>) {
        let column = gtk::Box::new(gtk::Orientation::Vertical, 12);
        let face = widgets::avatar(64, &card.contact.name, card.contact.photo.as_deref());
        face.set_halign(gtk::Align::Center);
        column.append(&face);
        if let Some(short) = &card.send_short {
            column.append(&widgets::row(&card.send_label, short));
        }
        if let (Some(title), Some(line)) = (&card.half_title, &card.half_line) {
            column.append(&widgets::row(title, line));
        }
        for address in &card.also_paid_at {
            column.append(&widgets::row(&address.short, &address.line));
        }
        if !card.requests.is_empty() {
            column.append(&widgets::heading(&card.requests_title));
            column.append(&widgets::centred(&card.requests_note));
        }
        for request in &card.requests {
            let row = widgets::row(&request.note, &request.line);
            let state = gtk::Label::new(Some(request.asked.as_deref().unwrap_or(&request.state)));
            state.add_css_class("dim-label");
            row.add_suffix(&state);
            row.set_activatable(true);
            let (then, request) = (sender.clone(), request.id);
            row.connect_activated(move |_| then.input(Input::Request(request)));
            column.append(&row);
        }
        if !card.paid.is_empty() || card.paid_empty.is_some() {
            column.append(&widgets::heading(&card.paid_title));
        }
        for paid in &card.paid {
            let row = widgets::row(&paid.title, &paid.when);
            row.add_suffix(&gtk::Label::new(Some(&paid.amount_text)));
            column.append(&row);
        }
        if let Some(empty) = &card.paid_empty {
            column.append(&widgets::dim(empty));
        }
        let dialog = adw::AlertDialog::builder().heading(&card.contact.name).extra_child(&column).build();
        if let Some(note) = &card.contact.note {
            dialog.set_body(note);
        }
        dialog.add_responses(&[("close", "_Close"), ("delete", "_Delete"), ("edit", "_Edit")]);
        dialog.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
        if card.contact.send_address.is_some() {
            dialog.add_response("pay", &card.send_button);
            dialog.set_response_appearance("pay", adw::ResponseAppearance::Suggested);
        }
        dialog.set_close_response("close");
        let (sender, address) = (sender.clone(), card.contact.send_address.clone());
        dialog.connect_response(None, move |_, response| match response {
            "edit" => sender.input(Input::Edit(id)),
            "delete" => sender.input(Input::Delete(id)),
            "pay" => {
                if let Some(address) = &address {
                    sender.input(Input::Pay(address.clone()));
                }
            }
            _ => {}
        });
        dialog.present(Some(&self.list));
    }

    fn show_editor(&self, id: Option<u64>, draft: std::sync::Arc<ContactDraft>, sender: &ComponentSender<Self>) {
        let (name_row, note_row, address_row) = (
            adw::EntryRow::builder().title("Name").text(&draft.name).build(),
            adw::EntryRow::builder().title("Note (optional)").text(draft.note.as_deref().unwrap_or("")).build(),
            adw::EntryRow::builder().title("Send address (optional)").text(draft.send_address.as_deref().unwrap_or("")).build(),
        );
        let form = widgets::list();
        form.append(&name_row);
        form.append(&note_row);
        form.append(&address_row);
        let column = gtk::Box::new(gtk::Orientation::Vertical, 12);
        let face = widgets::avatar(64, &draft.name, draft.photo.as_deref());
        face.set_halign(gtk::Align::Center);
        column.append(&face);
        column.append(&form);
        let dialog = adw::AlertDialog::builder().heading(if id.is_some() { "Edit contact" } else { "Add a contact" }).extra_child(&column).build();
        dialog.add_responses(&[("cancel", "_Cancel"), ("photo", "_Photo…"), ("save", "_Save")]);
        dialog.set_response_appearance("save", adw::ResponseAppearance::Suggested);
        dialog.set_close_response("cancel");
        let sender = sender.clone();
        dialog.connect_response(None, move |_, response| {
            if response == "cancel" {
                return;
            }
            let some = |text: String| Some(text).filter(|t| !t.trim().is_empty());
            // Everything not on this form stays as it was.
            let filled = std::sync::Arc::new(ContactDraft { name: name_row.text().to_string(), note: some(note_row.text().to_string()), photo: draft.photo.clone(), send_address: some(address_row.text().to_string()), also_paid_at: draft.also_paid_at.clone() });
            if response == "photo" {
                sender.input(Input::ChoosePhoto(id, View(filled)));
            } else {
                sender.input(Input::Save(id, View(filled)));
            }
        });
        dialog.present(Some(&self.list));
    }

    fn save(&self, id: Option<u64>, draft: ContactDraft, sender: &ComponentSender<Self>) {
        let (engine, sender) = (self.engine.clone(), sender.clone());
        relm4::spawn(async move {
            let saved = engine
                .change(move |wallet| async move {
                    match id {
                        Some(id) => wallet.edit_contact(id, draft).await.map(|_| ()),
                        None => wallet.add_contact(draft).await.map(|_| ()),
                    }
                })
                .await;
            match saved {
                Ok(()) => sender.input(Input::Search),
                Err(e) => sender.input(Input::Failed(e.to_string())),
            }
        });
    }
}
