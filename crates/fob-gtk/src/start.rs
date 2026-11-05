//! Making a wallet or bringing one back: the first screen with no wallet, and
//! Add a wallet from Settings.

use fob_app::{Engine, Phase};
use relm4::{
    adw::{self, prelude::*},
    gtk,
    prelude::*,
};

use crate::root::{Go, Msg as RootMsg};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Way {
    New,
    Words,
    Code,
    ViewOnly,
}

#[derive(Debug)]
pub enum Input {
    Phase(Phase),
    Adding,
    Way(u32),
    Go,
    Failed(String),
    Done,
}

pub struct Start {
    engine: Engine,
    way: Way,
    busy: bool,
    adding: bool,
    error: Option<String>,
    name: adw::EntryRow,
    words: adw::EntryRow,
    view_key: adw::PasswordEntryRow,
    restore_point: adw::EntryRow,
}

#[relm4::component(pub)]
impl SimpleComponent for Start {
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
                        set_margin_all: 24,

                        gtk::Label { set_label: "A wallet for this computer", add_css_class: "title-1", set_wrap: true },
                        gtk::Label {
                            add_css_class: "dim-label",
                            set_wrap: true,
                            set_justify: gtk::Justification::Center,
                            #[watch]
                            set_label: &model.lead(),
                        },
                        adw::PreferencesGroup {
                            add = &adw::ComboRow {
                                set_title: "How",
                                set_model: Some(&gtk::StringList::new(&["Make a new wallet", "Restore from seed words", "Restore from a wallet code", "Watch only, from an address and view key"])),
                                connect_selected_notify[sender] => move |row| sender.input(Input::Way(row.selected())),
                            },
                            #[local_ref]
                            add = name -> adw::EntryRow { set_title: "Name (optional)", },
                            #[local_ref]
                            add = words -> adw::EntryRow {
                                #[watch]
                                set_visible: model.way != Way::New,
                                #[watch]
                                set_title: match model.way {
                                    Way::Code => "Wallet code",
                                    Way::ViewOnly => "Address",
                                    _ => "Seed words",
                                },
                            },
                            #[local_ref]
                            add = view_key -> adw::PasswordEntryRow {
                                set_title: "Secret view key",
                                #[watch]
                                set_visible: model.way == Way::ViewOnly,
                            },
                            #[local_ref]
                            add = restore_point -> adw::EntryRow {
                                set_title: "Restore height or date (optional)",
                                #[watch]
                                set_visible: model.way != Way::New,
                            },
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
                            set_sensitive: !model.busy,
                            #[watch]
                            set_label: if model.busy { "Working…" } else if model.way == Way::New { "Make wallet" } else { "Restore" },
                            connect_clicked => Input::Go,
                        },
                    },
                },
            },
        }
    }

    fn init(engine: Engine, root: Self::Root, sender: ComponentSender<Self>) -> ComponentParts<Self> {
        let (name, words, view_key, restore_point) = (adw::EntryRow::new(), adw::EntryRow::new(), adw::PasswordEntryRow::new(), adw::EntryRow::new());
        let model = Start { engine, way: Way::New, busy: false, adding: false, error: None, name: name.clone(), words: words.clone(), view_key: view_key.clone(), restore_point: restore_point.clone() };
        let (name, words, view_key, restore_point) = (&name, &words, &view_key, &restore_point);
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, input: Input, sender: ComponentSender<Self>) {
        match input {
            Input::Phase(Phase::Failed(why)) => self.error = Some(why),
            Input::Phase(_) => {}
            Input::Adding => self.adding = true,
            Input::Way(i) => {
                self.way = [Way::New, Way::Words, Way::Code, Way::ViewOnly][i.min(3) as usize];
                self.error = None;
            }
            Input::Go => self.go(&sender),
            Input::Failed(why) => {
                self.busy = false;
                self.error = Some(why);
            }
            Input::Done => {
                self.busy = false;
                self.error = None;
                for row in [&self.name, &self.words, &self.restore_point] {
                    row.set_text("");
                }
                self.view_key.set_text("");
                if std::mem::take(&mut self.adding) {
                    let _ = sender.output(RootMsg::Go(Go::Back));
                }
            }
        }
    }
}

impl Start {
    fn lead(&self) -> String {
        let network = match self.engine.network() {
            fob_app::views::Network::Mainnet => "real money, on mainnet",
            fob_app::views::Network::Stagenet => "test coins, on stagenet",
        };
        format!("This one holds {network}.")
    }

    fn go(&mut self, sender: &ComponentSender<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.error = None;
        let (name, text, key) = (self.name.text().to_string(), self.words.text().to_string(), self.view_key.text().to_string());
        let point = Some(self.restore_point.text().to_string()).filter(|p| !p.trim().is_empty());
        let (engine, way, sender) = (self.engine.clone(), self.way, sender.clone());
        relm4::spawn(async move {
            let result = match way {
                Way::New => engine.create_wallet(name).await,
                Way::Words => engine.restore_words(name, text, point).await,
                Way::Code => engine.restore_code(name, text, point).await,
                Way::ViewOnly => engine.restore_view_only(name, text, key, point).await,
            };
            sender.input(match result {
                Ok(()) => Input::Done,
                Err(e) => Input::Failed(e.to_string()),
            });
        });
    }
}
