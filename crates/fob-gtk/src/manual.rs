//! Manual Sync: a wallet that only scans when asked, kept per wallet.

use fob_app::{
    Engine,
    views::{ManualSettings, PullMode, manual_incoming_max, manual_settled_max, manual_settled_min},
};
use relm4::{
    adw::{self, prelude::*},
    gtk,
    prelude::*,
};

use crate::{bridge::View, root::Msg as RootMsg};

#[derive(Debug)]
pub enum Input {
    Opened,
    Settings(View<ManualSettings>),
    On(bool),
    TurnOn,
    Mode(u32),
    Incoming(u32),
    Settled(u32),
    Failed(String),
}

pub struct Manual {
    engine: Engine,
    settings: Option<View<ManualSettings>>,
    /// The rows following Rust, not someone choosing.
    filling: bool,
    on: adw::SwitchRow,
    mode: adw::ComboRow,
    incoming: adw::SpinRow,
    settled: adw::SpinRow,
}

fn mode_of(index: u32) -> PullMode {
    match index {
        0 => PullMode::Disabled,
        1 => PullMode::Send,
        _ => PullMode::Auto,
    }
}

fn says(mode: Option<&PullMode>) -> &'static str {
    match mode {
        Some(PullMode::Disabled) => "Refresh on Home does nothing. Sync still works.",
        Some(PullMode::Send) => "Updates payments you've sent, and their change.",
        _ => "Also looks for new money in the node's pool and recent blocks. Your node sees each refresh.",
    }
}

#[relm4::component(pub)]
impl SimpleComponent for Manual {
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
                        adw::PreferencesGroup {
                            #[local_ref]
                            add = on -> adw::SwitchRow {
                                set_title: "Manual Sync",
                                set_subtitle: "Only scan when you ask.",
                                connect_active_notify[sender] => move |row| sender.input(Input::On(row.is_active())),
                            },
                        },
                        adw::PreferencesGroup {
                            set_title: "Refresh on Home",
                            #[watch]
                            set_visible: model.is_on(),
                            #[watch]
                            set_description: Some(says(model.settings.as_ref().map(|View(s)| &s.pull_mode))),
                            #[local_ref]
                            add = mode -> adw::ComboRow {
                                set_title: "What it does",
                                set_model: Some(&gtk::StringList::new(&["Disabled", "Send", "Auto"])),
                                connect_selected_notify[sender] => move |row| sender.input(Input::Mode(row.selected())),
                            },
                            #[local_ref]
                            add = incoming -> adw::SpinRow {
                                set_title: "Incoming scan blocks",
                                set_subtitle: &format!("1 to {}. A block is about 2 minutes.", manual_incoming_max()),
                                #[watch]
                                set_visible: model.settings.as_ref().is_some_and(|View(s)| s.pull_mode == PullMode::Auto),
                                connect_value_notify[sender] => move |row| sender.input(Input::Incoming(row.value() as u32)),
                            },
                        },
                        adw::PreferencesGroup {
                            #[watch]
                            set_visible: model.is_on(),
                            #[local_ref]
                            add = settled -> adw::SpinRow {
                                set_title: "Settled confirmations",
                                set_subtitle: &format!("{} to {}. Coins can be spent from here, and payments stop being checked.", manual_settled_min(), manual_settled_max()),
                                connect_value_notify[sender] => move |row| sender.input(Input::Settled(row.value() as u32)),
                            },
                        },
                    },
                },
            },
        }
    }

    fn init(engine: Engine, root: Self::Root, sender: ComponentSender<Self>) -> ComponentParts<Self> {
        let on = adw::SwitchRow::new();
        let mode = adw::ComboRow::new();
        let incoming = adw::SpinRow::with_range(1.0, f64::from(manual_incoming_max()), 1.0);
        let settled = adw::SpinRow::with_range(f64::from(manual_settled_min()), f64::from(manual_settled_max()), 1.0);
        let model = Manual { engine, settings: None, filling: false, on: on.clone(), mode: mode.clone(), incoming: incoming.clone(), settled: settled.clone() };
        let (on, mode, incoming, settled) = (&on, &mode, &incoming, &settled);
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, input: Input, sender: ComponentSender<Self>) {
        match input {
            Input::Opened => self.read(&sender),
            Input::Settings(view) => {
                let s = &view.0;
                self.filling = true;
                self.on.set_active(s.on);
                self.mode.set_selected(match s.pull_mode {
                    PullMode::Disabled => 0,
                    PullMode::Send => 1,
                    PullMode::Auto => 2,
                });
                self.incoming.set_value(f64::from(s.incoming_scan_blocks));
                self.settled.set_value(f64::from(s.settled_confirmations));
                self.filling = false;
                self.settings = Some(view);
            }
            // Messages arrive after `filling` is over, so what they carry is compared instead.
            Input::On(on) if self.settings.as_ref().is_none_or(|View(s)| s.on == on) => {}
            Input::On(true) => self.ask_turn_on(&sender),
            Input::On(false) => self.apply(&sender, |wallet| Box::pin(async move { wallet.set_manual_sync(false).await })),
            Input::TurnOn => self.apply(&sender, |wallet| Box::pin(async move { wallet.set_manual_sync(true).await })),
            Input::Mode(index) => {
                let chosen = mode_of(index);
                if self.settings.as_ref().is_some_and(|View(s)| s.pull_mode != chosen) {
                    self.apply(&sender, move |wallet| Box::pin(async move { wallet.set_pull_mode(chosen).await }));
                }
            }
            Input::Incoming(blocks) => {
                if self.settings.as_ref().is_some_and(|View(s)| s.incoming_scan_blocks != blocks) {
                    self.apply(&sender, move |wallet| Box::pin(async move { wallet.set_incoming_scan_blocks(blocks).await }));
                }
            }
            Input::Settled(confirmations) => {
                if self.settings.as_ref().is_some_and(|View(s)| s.settled_confirmations != confirmations) {
                    self.apply(&sender, move |wallet| Box::pin(async move { wallet.set_settled_confirmations(confirmations).await }));
                }
            }
            Input::Failed(why) => {
                let _ = sender.output(RootMsg::Say(why));
                self.read(&sender);
            }
        }
    }
}

type Change = std::pin::Pin<Box<dyn Future<Output = Result<(), fob_app::FobError>> + Send>>;

impl Manual {
    fn is_on(&self) -> bool {
        self.settings.as_ref().is_some_and(|View(s)| s.on)
    }

    fn read(&self, sender: &ComponentSender<Self>) {
        let (engine, sender) = (self.engine.clone(), sender.clone());
        relm4::spawn(async move {
            if let Some(settings) = engine.read(|wallet, _| async move { wallet.manual_settings().await }).await {
                sender.input(Input::Settings(View(settings.into())));
            }
        });
    }

    fn apply(&self, sender: &ComponentSender<Self>, change: impl FnOnce(std::sync::Arc<fob_app::views::FobWallet>) -> Change + Send + 'static) {
        let (engine, sender) = (self.engine.clone(), sender.clone());
        relm4::spawn(async move {
            match engine.change(change).await {
                Ok(()) => {
                    if let Some(settings) = engine.read(|wallet, _| async move { wallet.manual_settings().await }).await {
                        sender.input(Input::Settings(View(settings.into())));
                    }
                }
                Err(e) => sender.input(Input::Failed(e.to_string())),
            }
        });
    }

    fn ask_turn_on(&self, sender: &ComponentSender<Self>) {
        let dialog = adw::AlertDialog::builder()
            .heading("Turn on Manual Sync?")
            .body("Fob stops looking for blocks. Instant updates turn off too. It scans when you press Sync.\n\nNew payments stay hidden until then. The money is still yours. Your balance is as of your last scan.\n\nYou can always send. Fob asks your node only for what a payment needs. If a coin was spent from another device, the payment fails and nothing is lost.")
            .build();
        dialog.add_responses(&[("not-now", "_Not now"), ("on", "_Turn on")]);
        dialog.set_close_response("not-now");
        dialog.set_response_appearance("on", adw::ResponseAppearance::Suggested);
        let sender = sender.clone();
        let row = self.on.clone();
        dialog.connect_response(None, move |_, response| {
            if response == "on" {
                sender.input(Input::TurnOn);
            } else {
                row.set_active(false);
            }
        });
        dialog.present(Some(&self.on));
    }
}
