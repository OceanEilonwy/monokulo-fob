//! Prices: the default currency, who's asked and how, each checked as this
//! page opens. What each costs in privacy is in Rust's words.

use fob_app::{
    Engine,
    views::{PriceChecks, PriceProvider, PriceTransport, Screens},
};
use relm4::{
    adw::{self, prelude::*},
    gtk,
    prelude::*,
};

use crate::{bridge::View, root::Msg as RootMsg, widgets};

#[derive(Debug)]
pub enum Input {
    Screens(View<Screens>),
    Opened,
    Checks(Option<View<PriceChecks>>),
    Currency,
    Provider(PriceProvider),
    Transport(PriceTransport),
    Say(String),
}

pub struct Prices {
    engine: Engine,
    intro: String,
    currency: String,
    sources: gtk::ListBox,
    checks: gtk::ListBox,
    summary: String,
    checking: bool,
}

#[relm4::component(pub)]
impl SimpleComponent for Prices {
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
                    set_maximum_size: 600,
                    #[wrap(Some)]
                    set_child = &gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 18,
                        set_margin_all: 16,
                        gtk::Label {
                            add_css_class: "dim-label",
                            set_wrap: true,
                            set_xalign: 0.0,
                            #[watch]
                            set_label: &model.intro,
                        },
                        adw::PreferencesGroup {
                            add = &adw::ActionRow {
                                set_use_markup: false,
                                set_title: "Default currency",
                                set_activatable: true,
                                #[watch]
                                set_subtitle: &gtk::glib::markup_escape_text(&model.currency),
                                connect_activated => Input::Currency,
                            },
                        },
                        adw::PreferencesGroup {
                            set_title: "How prices are asked for",
                            #[local_ref]
                            add = sources -> gtk::ListBox {
                                add_css_class: "boxed-list",
                                set_selection_mode: gtk::SelectionMode::None,
                            },
                        },
                        adw::PreferencesGroup {
                            set_title: "Who's asked",
                            #[watch]
                            set_description: Some(gtk::glib::markup_escape_text(&model.summary).as_str()),
                            #[local_ref]
                            add = checks -> gtk::ListBox {
                                add_css_class: "boxed-list",
                                set_selection_mode: gtk::SelectionMode::None,
                            },
                        },
                        gtk::Spinner {
                            #[watch]
                            set_spinning: model.checking,
                            #[watch]
                            set_visible: model.checking,
                        },
                    },
                },
            },
        }
    }

    fn init(engine: Engine, root: Self::Root, sender: ComponentSender<Self>) -> ComponentParts<Self> {
        let (sources, checks) = (gtk::ListBox::new(), gtk::ListBox::new());
        let model = Prices { engine, intro: String::new(), currency: String::new(), sources: sources.clone(), checks: checks.clone(), summary: String::new(), checking: false };
        let (sources, checks) = (&sources, &checks);
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, input: Input, sender: ComponentSender<Self>) {
        match input {
            Input::Screens(View(screens)) => {
                let prices = &screens.prices;
                self.intro = format!("{}{}{}", prices.intro_before, prices.intro_bold.as_deref().unwrap_or(""), prices.intro_after);
                self.currency = prices.currency_text.clone();
                self.draw_sources(&screens, &sender);
            }
            Input::Opened => self.check(&sender),
            Input::Checks(checks) => {
                self.checking = false;
                widgets::clear(&self.checks);
                self.summary.clear();
                if let Some(View(checks)) = checks {
                    self.summary = checks.summary.clone();
                    for check in &checks.lines {
                        let line = check.failure.as_deref().or(check.rate.as_deref()).unwrap_or("");
                        let row = widgets::row(&check.name, line);
                        row.upcast_ref::<gtk::Widget>().update_property(&[gtk::accessible::Property::Label(&check.label)]);
                        if check.chosen {
                            row.add_suffix(&gtk::Image::from_icon_name("object-select-symbolic"));
                        } else if check.can_pick {
                            row.set_activatable(true);
                            let (provider, then) = (check.provider, sender.clone());
                            row.connect_activated(move |_| then.input(Input::Provider(provider)));
                        }
                        self.checks.append(&row);
                    }
                }
            }
            Input::Currency => self.ask_currency(&sender),
            Input::Provider(provider) => {
                let (engine, then) = (self.engine.clone(), sender.clone());
                relm4::spawn(async move {
                    match engine.change(move |wallet| async move { wallet.set_price_provider(provider).await }).await {
                        Ok(said) => {
                            if let Some(said) = said {
                                then.input(Input::Say(said));
                            }
                            then.input(Input::Opened);
                        }
                        Err(e) => then.input(Input::Say(e.to_string())),
                    }
                });
            }
            Input::Transport(transport) => {
                let (engine, then) = (self.engine.clone(), sender.clone());
                relm4::spawn(async move {
                    match engine.change(move |wallet| async move { wallet.set_price_transport(transport).await }).await {
                        Ok(()) => then.input(Input::Opened),
                        Err(e) => then.input(Input::Say(e.to_string())),
                    }
                });
            }
            Input::Say(text) => {
                let _ = sender.output(RootMsg::Say(text));
            }
        }
    }
}

impl Prices {
    fn check(&mut self, sender: &ComponentSender<Self>) {
        self.checking = true;
        let (engine, sender) = (self.engine.clone(), sender.clone());
        relm4::spawn(async move {
            let checks = engine.read(|wallet, _| async move { wallet.check_prices().await }).await.flatten();
            sender.input(Input::Checks(checks.map(|c| View(c.into()))));
        });
    }

    fn draw_sources(&self, screens: &Screens, sender: &ComponentSender<Self>) {
        widgets::clear(&self.sources);
        for source in &screens.prices.sources {
            let row = widgets::row(&source.title, &source.line);
            row.set_sensitive(source.available);
            if source.chosen {
                row.add_suffix(&gtk::Image::from_icon_name("object-select-symbolic"));
            } else if source.available {
                row.set_activatable(true);
                let (transport, then) = (source.transport, sender.clone());
                row.connect_activated(move |_| then.input(Input::Transport(transport)));
            }
            self.sources.append(&row);
        }
    }

    fn ask_currency(&self, sender: &ComponentSender<Self>) {
        let holder = gtk::ListBox::new();
        holder.add_css_class("boxed-list");
        holder.set_selection_mode(gtk::SelectionMode::None);
        let scroll = gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).min_content_height(380).child(&holder).build();
        let dialog = adw::AlertDialog::builder().heading("Default currency").extra_child(&scroll).build();
        dialog.add_response("close", "_Close");
        dialog.present(Some(&self.sources));
        let (engine, sender, dialog_ref) = (self.engine.clone(), sender.clone(), dialog.clone());
        gtk::glib::spawn_future_local(async move {
            let lookup = engine.clone();
            let list = relm4::spawn(async move { lookup.read(|wallet, _| async move { wallet.currency_list(None, String::new()).await }).await }).await.ok().flatten();
            let Some(list) = list else { return };
            for group in &list.groups {
                for currency in &group.rows {
                    let row = widgets::row(&currency.name, &currency.line);
                    row.set_activatable(true);
                    if currency.chosen {
                        row.add_suffix(&gtk::Image::from_icon_name("object-select-symbolic"));
                    }
                    let (code, engine, sender, dialog) = (currency.code.clone(), engine.clone(), sender.clone(), dialog_ref.clone());
                    row.connect_activated(move |_| {
                        let (code, engine, sender) = (code.clone(), engine.clone(), sender.clone());
                        relm4::spawn(async move {
                            match engine.change(move |wallet| async move { wallet.set_currency(code).await }).await {
                                Ok(Some(said)) => sender.input(Input::Say(said)),
                                Ok(None) => sender.input(Input::Opened),
                                Err(e) => sender.input(Input::Say(e.to_string())),
                            }
                        });
                        dialog.close();
                    });
                    holder.append(&row);
                }
            }
        });
    }
}
