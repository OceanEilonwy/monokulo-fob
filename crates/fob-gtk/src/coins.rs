//! Every coin: ready to spend, and not yet.

use fob_app::{Engine, views::Screens};
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
}

pub struct Coins {
    body: gtk::Box,
}

#[relm4::component(pub)]
impl SimpleComponent for Coins {
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
                    #[local_ref]
                    #[wrap(Some)]
                    set_child = body -> gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 18,
                        set_margin_all: 16,
                    },
                },
            },
        }
    }

    fn init(_engine: Engine, root: Self::Root, _sender: ComponentSender<Self>) -> ComponentParts<Self> {
        let body = gtk::Box::new(gtk::Orientation::Vertical, 18);
        let model = Coins { body: body.clone() };
        let body = &body;
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, input: Input, sender: ComponentSender<Self>) {
        let Input::Screens(View(screens)) = input;
        let coins = &screens.coins;
        widgets::empty(&self.body);
        self.body.append(&widgets::dim(&coins.intro));
        let bar = gtk::LevelBar::builder().value(f64::from(coins.progress)).build();
        bar.update_property(&[gtk::accessible::Property::Label(&coins.bar_description)]);
        self.body.append(&bar);
        let ready = adw::PreferencesGroup::builder().title(gtk::glib::markup_escape_text(&coins.ready_title).as_str()).description(gtk::glib::markup_escape_text(&coins.ready_sum).as_str()).build();
        for coin in &coins.ready_coins {
            let row = widgets::row(&coin.amount_text, &coin.source);
            if let Some(fiat) = &coin.fiat_text {
                let text = gtk::Label::new(Some(fiat));
                text.add_css_class("dim-label");
                row.add_suffix(&text);
            }
            ready.add(&row);
        }
        self.body.append(&ready);
        if coins.can_split {
            let split = gtk::Button::builder().child(&widgets::icon_label(crate::icons::SPLIT, "Split up", false)).halign(gtk::Align::Center).css_classes(["pill"]).build();
            let then = sender.clone();
            split.connect_clicked(move |_| {
                let _ = then.output(RootMsg::Go(Go::Split));
            });
            self.body.append(&split);
        }
        if !coins.not_ready_coins.is_empty() {
            let waiting = adw::PreferencesGroup::builder().title(gtk::glib::markup_escape_text(&coins.not_ready_title).as_str()).description(gtk::glib::markup_escape_text(&coins.not_ready_sum).as_str()).build();
            for coin in &coins.not_ready_coins {
                let row = widgets::row(&coin.amount_text, &coin.source);
                let when = gtk::Label::new(Some(&coin.ready));
                when.add_css_class("dim-label");
                row.add_suffix(&when);
                waiting.add(&row);
            }
            self.body.append(&waiting);
        }
    }
}
