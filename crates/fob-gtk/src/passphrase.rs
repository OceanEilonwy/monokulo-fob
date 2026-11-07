//! Choosing the passphrase on first run, and typing it after.

use fob_app::{Engine, Phase};
use relm4::{
    adw::{self, prelude::*},
    gtk,
    prelude::*,
};

use crate::{root::Msg as RootMsg, widgets};

#[derive(Debug)]
pub enum Input {
    Phase(Phase),
    Go,
    Failed(String),
    Done,
}

pub struct Passphrase {
    engine: Engine,
    choosing: bool,
    busy: bool,
    error: Option<String>,
    first: gtk::PasswordEntry,
    second: gtk::PasswordEntry,
}

#[relm4::component(pub)]
impl SimpleComponent for Passphrase {
    type Init = Engine;
    type Input = Input;
    type Output = RootMsg;

    view! {
        adw::ToolbarView {
            add_top_bar = &adw::HeaderBar { set_show_title: false },
            #[wrap(Some)]
            set_content = &adw::Clamp {
                set_maximum_size: 420,
                set_valign: gtk::Align::Center,
                #[wrap(Some)]
                set_child = &gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 16,
                    set_margin_all: 24,

                    gtk::Label {
                        add_css_class: "title-1",
                        #[watch]
                        set_label: if model.choosing { "Choose a passphrase" } else { "Fob is locked" },
                    },
                    gtk::Label {
                        set_wrap: true,
                        set_justify: gtk::Justification::Center,
                        add_css_class: "dim-label",
                        #[watch]
                        set_label: if model.choosing {
                            "It opens your wallets on this computer, and Fob asks for it again before spending or showing keys. If you lose it, only your seed phrases bring your wallets back."
                        } else {
                            "Type your passphrase to open your wallets."
                        },
                    },
                    #[local_ref]
                    first -> gtk::PasswordEntry {
                        set_show_peek_icon: true,
                        set_placeholder_text: Some("Passphrase"),
                        update_property: &[gtk::accessible::Property::Label("Passphrase")],
                        #[watch]
                        set_sensitive: !model.busy,
                        connect_activate => Input::Go,
                    },
                    #[local_ref]
                    second -> gtk::PasswordEntry {
                        set_show_peek_icon: true,
                        set_placeholder_text: Some("The passphrase again"),
                        update_property: &[gtk::accessible::Property::Label("The passphrase again")],
                        #[watch]
                        set_visible: model.choosing,
                        #[watch]
                        set_sensitive: !model.busy,
                        connect_activate => Input::Go,
                    },
                    gtk::Label {
                        set_wrap: true,
                        add_css_class: "error",
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
                        set_label: if model.busy { "Opening…" } else if model.choosing { "Continue" } else { "Unlock" },
                        #[watch]
                        set_sensitive: !model.busy,
                        connect_clicked => Input::Go,
                    },
                },
            },
        }
    }

    fn init(engine: Engine, root: Self::Root, sender: ComponentSender<Self>) -> ComponentParts<Self> {
        let (first, second) = (gtk::PasswordEntry::new(), gtk::PasswordEntry::new());
        let model = Passphrase { engine, choosing: true, busy: false, error: None, first: first.clone(), second: second.clone() };
        let (first, second) = (&first, &second);
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, input: Input, sender: ComponentSender<Self>) {
        match input {
            Input::Phase(phase) => {
                self.choosing = phase == Phase::NoPassphrase;
                self.busy = false;
                self.error = None;
                self.first.set_text("");
                self.second.set_text("");
                if matches!(phase, Phase::NoPassphrase | Phase::Locked) {
                    self.first.grab_focus();
                }
            }
            Input::Go => self.go(&sender),
            Input::Failed(why) => {
                self.busy = false;
                self.error = Some(why);
                self.first.grab_focus();
            }
            Input::Done => self.busy = false,
        }
    }
}

impl Passphrase {
    fn go(&mut self, sender: &ComponentSender<Self>) {
        if self.busy {
            return;
        }
        let typed = widgets::secret_text(self.first.text());
        if self.choosing && typed.as_str() != self.second.text().as_str() {
            self.error = Some(fob_app::words::errors::passphrases_differ());
            return;
        }
        self.busy = true;
        self.error = None;
        let (engine, choosing, sender) = (self.engine.clone(), self.choosing, sender.clone());
        relm4::spawn(async move {
            let result = if choosing { engine.set_up(String::clone(&typed)).await } else { engine.unlock(String::clone(&typed)).await };
            sender.input(match result {
                Ok(()) => Input::Done,
                Err(e) => Input::Failed(e.to_string()),
            });
        });
    }
}
