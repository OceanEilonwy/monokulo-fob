//! Contacts: everyone by name, their card, and adding or changing one.

use fob_app::{
    Engine,
    views::{ContactCard, ContactDraft, ContactRow, Screens},
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
    Save(Option<u64>, String, String, String),
    Delete(u64),
    Pay(String),
    Failed(String),
}

pub struct Contacts {
    engine: Engine,
    search: gtk::SearchEntry,
    list: gtk::ListBox,
}

#[relm4::component(pub)]
impl SimpleComponent for Contacts {
    type Init = Engine;
    type Input = Input;
    type Output = RootMsg;

    view! {
        adw::ToolbarView {
            add_top_bar = &adw::HeaderBar {
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
                    },
                },
            },
        }
    }

    fn init(engine: Engine, root: Self::Root, sender: ComponentSender<Self>) -> ComponentParts<Self> {
        let (search, list) = (gtk::SearchEntry::new(), gtk::ListBox::new());
        let model = Contacts { engine, search: search.clone(), list: list.clone() };
        let (search, list) = (&search, &list);
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, input: Input, sender: ComponentSender<Self>) {
        match input {
            Input::Screens(View(screens)) => {
                if self.search.text().is_empty() {
                    self.draw(&screens.contacts, &sender);
                }
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
            Input::Add => self.show_editor(None, "", "", "", &sender),
            Input::Edit(id) => {
                let (engine, sender) = (self.engine.clone(), sender.clone());
                relm4::spawn(async move {
                    let contact = engine.read(move |wallet, _| async move { wallet.contact(id).await }).await.flatten();
                    if let Some(contact) = contact {
                        sender.input(Input::Save(Some(id), format!("\u{0}{}", contact.name), contact.note.unwrap_or_default(), contact.send_address.unwrap_or_default()));
                    }
                });
            }
            // Internal: a NUL before the name means "ask", from the contact just read.
            Input::Save(id, name, note, address) if name.starts_with('\u{0}') => self.show_editor(id, &name[1..], &note, &address, &sender),
            Input::Save(id, name, note, address) => self.save(id, name, note, address, &sender),
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
            Input::Failed(why) => {
                let _ = sender.output(RootMsg::Say(why));
            }
        }
    }
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
            row.add_prefix(&adw::Avatar::new(32, Some(&contact.name), true));
            row.set_activatable(true);
            let (id, sender) = (contact.id, sender.clone());
            row.connect_activated(move |_| sender.input(Input::Open(id)));
            self.list.append(&row);
        }
    }

    fn show_card(&self, id: u64, card: &ContactCard, sender: &ComponentSender<Self>) {
        let column = gtk::Box::new(gtk::Orientation::Vertical, 12);
        if let Some(short) = &card.send_short {
            column.append(&widgets::row(&card.send_label, short));
        }
        if let (Some(title), Some(line)) = (&card.half_title, &card.half_line) {
            column.append(&widgets::row(title, line));
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

    fn show_editor(&self, id: Option<u64>, name: &str, note: &str, address: &str, sender: &ComponentSender<Self>) {
        let (name_row, note_row, address_row) = (adw::EntryRow::builder().title("Name").text(name).build(), adw::EntryRow::builder().title("Note (optional)").text(note).build(), adw::EntryRow::builder().title("Send address (optional)").text(address).build());
        let form = widgets::list();
        form.append(&name_row);
        form.append(&note_row);
        form.append(&address_row);
        let dialog = adw::AlertDialog::builder().heading(if id.is_some() { "Edit contact" } else { "Add a contact" }).extra_child(&form).build();
        dialog.add_responses(&[("cancel", "_Cancel"), ("save", "_Save")]);
        dialog.set_response_appearance("save", adw::ResponseAppearance::Suggested);
        dialog.set_close_response("cancel");
        let sender = sender.clone();
        dialog.connect_response(Some("save"), move |_, _| {
            sender.input(Input::Save(id, name_row.text().to_string(), note_row.text().to_string(), address_row.text().to_string()));
        });
        dialog.present(Some(&self.list));
    }

    fn save(&self, id: Option<u64>, name: String, note: String, address: String, sender: &ComponentSender<Self>) {
        let some = |text: String| Some(text).filter(|t| !t.trim().is_empty());
        let draft = ContactDraft { name, note: some(note), photo: None, send_address: some(address), also_paid_at: vec![] };
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
