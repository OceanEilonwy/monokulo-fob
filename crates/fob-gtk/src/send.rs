//! Send's form. What a code does to it and how money survives a price going
//! are `fob_app::form`'s rules; every word is Rust's.

use fob_app::{
    Engine,
    form::{self, Amount, Taken},
    views::{AmountEntry, Network, Screens},
};
use relm4::{
    adw::{self, prelude::*},
    gtk,
    prelude::*,
};

use crate::{
    bridge::View,
    confirm::Filled,
    root::{Go, Msg as RootMsg},
    widgets,
};

#[derive(Debug)]
pub enum Input {
    Screens(View<Screens>),
    Opened(Option<String>),
    To,
    Typed,
    Switch,
    Entry(View<AmountEntry>),
    Paste,
    Picture,
    Take(String),
    /// Replace what's filled with a code, once asked.
    Replace(String),
    Refused(String),
    Pick(String),
    /// A value for the amount field, converted by Rust.
    Converted(String),
    Review,
    Reviewed(View<fob_app::views::PaymentPreview>, Filled),
    Failed(String),
}

pub struct Send {
    engine: Engine,
    amount: Amount,
    entry: Option<View<AmountEntry>>,
    code: Option<String>,
    busy: bool,
    error: Option<String>,
    to: adw::EntryRow,
    /// Before `to`: the address's pattern, once it is one.
    to_mark: gtk::Box,
    amount_row: adw::EntryRow,
    note: adw::EntryRow,
    people: adw::PreferencesGroup,
    people_rows: Vec<adw::ActionRow>,
    /// Set while code writes to a field, so its own change isn't taken for typing.
    filling: bool,
}

fn network(engine: &Engine) -> Network {
    engine.network()
}

#[relm4::component(pub)]
impl SimpleComponent for Send {
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

                        gtk::Box {
                            set_halign: gtk::Align::Center,
                            set_spacing: 12,
                            gtk::Button { set_label: "Paste a code", add_css_class: "pill", connect_clicked => Input::Paste },
                            gtk::Button { set_label: "Open a picture", add_css_class: "pill", connect_clicked => Input::Picture },
                        },
                        adw::PreferencesGroup {
                            #[local_ref]
                            add = to -> adw::EntryRow {
                                set_title: "To",
                                connect_changed => Input::To,
                            },
                            #[local_ref]
                            add = amount_row -> adw::EntryRow {
                                set_input_purpose: gtk::InputPurpose::Number,
                                connect_changed => Input::Typed,
                                #[watch]
                                set_title: model.entry.as_ref().map_or("Amount", |View(e)| e.label.as_str()),
                                // The unit with the swap mark where it can swap: a flat button, as GTK has them.
                                add_suffix = &gtk::Button {
                                    add_css_class: "flat",
                                    set_valign: gtk::Align::Center,
                                    #[watch]
                                    set_sensitive: model.entry.as_ref().is_some_and(|View(e)| e.can_switch),
                                    #[watch]
                                    set_tooltip_text: model.entry.as_ref().and_then(|View(e)| e.switch_label.as_deref()),
                                    connect_clicked => Input::Switch,
                                    #[wrap(Some)]
                                    set_child = &gtk::Box {
                                        set_spacing: 8,
                                        gtk::Label {
                                            add_css_class: "heading",
                                            #[watch]
                                            set_label: model.entry.as_ref().map_or("XMR", |View(e)| e.unit.as_str()),
                                        },
                                        #[local_ref]
                                        unit_swap -> gtk::DrawingArea {
                                            #[watch]
                                            set_visible: model.entry.as_ref().is_some_and(|View(e)| e.can_switch),
                                        },
                                    },
                                },
                            },
                            #[local_ref]
                            add = note -> adw::EntryRow { set_title: "Note (optional)" },
                        },
                        gtk::Label {
                            add_css_class: "dim-label",
                            set_wrap: true,
                            #[watch]
                            set_label: model.entry.as_ref().and_then(|View(e)| e.line.as_deref()).unwrap_or(""),
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
                            set_label: "Review",
                            #[watch]
                            set_sensitive: model.can_review(),
                            connect_clicked => Input::Review,
                        },
                        #[local_ref]
                        people -> adw::PreferencesGroup { set_title: "Contacts" },
                    },
                },
            },
        }
    }

    fn init(engine: Engine, root: Self::Root, sender: ComponentSender<Self>) -> ComponentParts<Self> {
        let (to, amount_row, note) = (adw::EntryRow::new(), adw::EntryRow::new(), adw::EntryRow::new());
        let people = adw::PreferencesGroup::new();
        let to_mark = gtk::Box::builder().visible(false).valign(gtk::Align::Center).build();
        to.add_prefix(&to_mark);
        let model = Send {
            engine,
            amount: Amount::default(),
            entry: None,
            code: None,
            busy: false,
            error: None,
            to: to.clone(),
            to_mark,
            amount_row: amount_row.clone(),
            note: note.clone(),
            people: people.clone(),
            people_rows: vec![],
            filling: false,
        };
        let unit_swap = widgets::swap_icon(16, None);
        let (to, amount_row, note, people, unit_swap) = (&to, &amount_row, &note, &people, &unit_swap);
        let widgets = view_output!();
        widgets::hide_pencil(amount_row);
        ComponentParts { model, widgets }
    }

    fn update(&mut self, input: Input, sender: ComponentSender<Self>) {
        match input {
            Input::Screens(View(screens)) => {
                self.draw_people(&screens, &sender);
                self.ask_entry(&sender);
            }
            Input::Opened(code) => {
                self.error = None;
                self.busy = false;
                self.clear();
                self.ask_entry(&sender);
                // So Confirm's first fee doesn't wait on this.
                let engine = self.engine.clone();
                relm4::spawn(async move {
                    engine.read(|wallet, _| async move { wallet.warm_up_send().await }).await;
                });
                if let Some(code) = code {
                    sender.input(Input::Take(code));
                }
            }
            // An address typed over a code's isn't that code's any more.
            Input::To => {
                if !self.filling {
                    self.code = None;
                }
                self.error = None;
                // Its pattern shows once it's an address of this network, as on Android.
                let text = self.to.text().trim().to_owned();
                widgets::empty(&self.to_mark);
                let valid = form::to_is_valid(network(&self.engine), text.clone());
                if valid {
                    self.to_mark.append(&widgets::pattern_sized(&text, 32));
                }
                self.to_mark.set_visible(valid);
            }
            Input::Typed => {
                if !self.filling {
                    self.amount.text = self.amount_row.text().to_string();
                    self.error = None;
                    self.ask_entry(&sender);
                }
            }
            Input::Switch => {
                self.amount.in_fiat = !self.amount.in_fiat;
                // Rust gives the same amount in the other unit; the field shows it.
                let (engine, text, to_fiat, sender) = (self.engine.clone(), self.amount.text.clone(), self.amount.in_fiat, sender.clone());
                relm4::spawn(async move {
                    let converted = engine.read(move |wallet, _| async move { wallet.convert_amount(text, to_fiat).await }).await;
                    if let Some(converted) = converted {
                        sender.input(Input::Converted(converted));
                    }
                });
            }
            Input::Entry(view) => {
                // Behind what's typed by now: a newer answer is on its way.
                if view.0.for_text != self.amount.text || view.0.for_in_fiat != self.amount.in_fiat {
                    return;
                }
                let can_switch = view.0.can_switch;
                self.entry = Some(view);
                if self.amount.price_is(can_switch) {
                    self.set_amount_field();
                    self.ask_entry(&sender);
                }
            }
            Input::Paste => {
                let sender = sender.clone();
                widgets::paste(&self.to, move |text| match text {
                    Some(text) if !text.trim().is_empty() => sender.input(Input::Take(text.trim().to_owned())),
                    _ => sender.input(Input::Refused(fob_app::words::errors::nothing_to_paste())),
                });
            }
            Input::Picture => {
                let sender = sender.clone();
                widgets::read_code_from_picture(self.to.upcast_ref(), move |text| match text {
                    Some(text) => sender.input(Input::Take(text)),
                    None => sender.input(Input::Refused(fob_app::words::errors::no_code_in_picture())),
                });
            }
            Input::Take(text) => self.take(text, &sender),
            Input::Replace(text) => self.fill(text, &sender),
            Input::Refused(why) => self.error = Some(why),
            Input::Converted(text) => {
                self.amount.text = text;
                self.set_amount_field();
                self.ask_entry(&sender);
            }
            Input::Pick(address) => {
                self.filling = true;
                self.to.set_text(&address);
                self.filling = false;
                self.code = None;
            }
            Input::Review => self.review(&sender),
            Input::Reviewed(preview, filled) => {
                self.busy = false;
                let _ = preview;
                let _ = sender.output(RootMsg::Go(Go::Confirm(filled)));
            }
            Input::Failed(why) => {
                self.busy = false;
                self.error = Some(why);
            }
        }
    }
}

impl Send {
    /// A new payment starts empty. Back from Confirm doesn't come through here.
    fn clear(&mut self) {
        self.filling = true;
        self.to.set_text("");
        self.note.set_text("");
        self.amount_row.set_text("");
        self.filling = false;
        self.code = None;
        self.amount = Amount::new(self.entry.as_ref().is_some_and(|View(e)| e.in_fiat));
    }

    fn can_review(&self) -> bool {
        !self.busy && form::address_is_valid(network(&self.engine), &self.to.text()) && self.entry.as_ref().is_some_and(|View(e)| e.sendable && e.for_text == self.amount.text && e.for_in_fiat == self.amount.in_fiat)
    }

    fn set_amount_field(&mut self) {
        self.filling = true;
        self.amount_row.set_text(&self.amount.text);
        self.filling = false;
    }

    fn ask_entry(&self, sender: &ComponentSender<Self>) {
        let (engine, text, in_fiat, sender) = (self.engine.clone(), self.amount.text.clone(), self.amount.in_fiat, sender.clone());
        relm4::spawn(async move {
            let entry = engine.read(move |wallet, clock| async move { wallet.amount_entry(text, in_fiat, clock.now, clock.utc_offset, clock.hours_24).await }).await;
            if let Some(entry) = entry {
                sender.input(Input::Entry(View(entry.into())));
            }
        });
    }

    /// A code from outside. With To filled it's never taken unasked.
    fn take(&mut self, text: String, sender: &ComponentSender<Self>) {
        if let Some(other) = form::asks_first(network(&self.engine), &self.to.text(), &text) {
            let dialog = adw::AlertDialog::builder().heading("Replace who this pays?").body(&other).build();
            dialog.add_responses(&[("keep", "_Keep"), ("replace", "_Replace")]);
            dialog.set_close_response("keep");
            let sender = sender.clone();
            dialog.connect_response(None, move |_, response| {
                if response == "replace" {
                    sender.input(Input::Replace(text.clone()));
                }
            });
            dialog.present(Some(&self.to));
            return;
        }
        self.fill(text, sender);
    }

    fn fill(&mut self, text: String, sender: &ComponentSender<Self>) {
        self.error = None;
        match form::take(network(&self.engine), &text) {
            Taken::Profile => self.error = Some(fob_app::words::errors::profile_not_payment()),
            Taken::Refused(why) => self.error = Some(why),
            Taken::Fill { to, note, code } => self.put(&to, &note, code),
            Taken::Review { to, amount, note, code } => {
                self.put(&to, &note, code);
                self.amount.text = amount;
                self.amount.in_fiat = false;
                self.set_amount_field();
                self.ask_entry(sender);
                self.review(sender);
            }
        }
    }

    fn put(&mut self, to: &str, note: &str, code: String) {
        self.filling = true;
        self.to.set_text(to);
        self.note.set_text(note);
        self.filling = false;
        self.code = Some(code);
    }

    /// Checked without asking the node, so Confirm opens at once and the fee comes in on it.
    fn review(&mut self, sender: &ComponentSender<Self>) {
        self.busy = true;
        self.error = None;
        let note = Some(self.note.text().to_string()).filter(|n| !n.trim().is_empty());
        let filled = Filled { to: self.to.text().trim().to_owned(), amount: self.amount.text.clone(), in_fiat: self.amount.in_fiat, note, code: self.code.clone() };
        let (engine, sender) = (self.engine.clone(), sender.clone());
        relm4::spawn(async move {
            let asked = filled.clone();
            let preview = engine.read(move |wallet, _| async move { wallet.preview(asked.to, asked.amount, asked.in_fiat, asked.code).await }).await;
            match preview {
                Some(Ok(preview)) => sender.input(Input::Reviewed(View(preview.into()), filled)),
                Some(Err(e)) => sender.input(Input::Failed(e.to_string())),
                None => {}
            }
        });
    }

    fn draw_people(&mut self, screens: &Screens, sender: &ComponentSender<Self>) {
        for row in self.people_rows.drain(..) {
            self.people.remove(&row);
        }
        for contact in screens.send_contacts.iter() {
            let Some(address) = contact.send_address.clone() else { continue };
            let row = widgets::row(&contact.name, &contact.line);
            row.set_activatable(true);
            let sender = sender.clone();
            row.connect_activated(move |_| sender.input(Input::Pick(address.clone())));
            self.people.add(&row);
            self.people_rows.push(row);
        }
        for paid in &screens.paid_before {
            let row = widgets::row(&paid.name, &paid.line);
            row.set_activatable(true);
            let (address, sender) = (paid.address.clone(), sender.clone());
            row.connect_activated(move |_| sender.input(Input::Pick(address.clone())));
            self.people.add(&row);
            self.people_rows.push(row);
        }
        self.people.set_visible(!self.people_rows.is_empty());
    }
}
