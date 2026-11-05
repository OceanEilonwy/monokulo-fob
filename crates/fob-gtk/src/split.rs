//! Split up: what's ready paid back to this wallet as pieces, so there's still
//! money ready after a payment. Rust picks the notes, what fits and every word.

use fob_app::{
    Engine,
    views::{Prepared, SplitBlock, SplitView},
};
use relm4::{
    adw::{self, prelude::*},
    gtk::{self, cairo},
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
    /// A balance or a price changed.
    Refresh,
    Show(View<SplitView>),
    Add(u32),
    Remove(u32),
    Counts(Vec<u32>),
    Clear,
    Refused(String),
    Prepare,
    Prepared(View<Prepared>),
}

pub struct Split {
    engine: Engine,
    counts: Vec<u32>,
    view: Option<View<SplitView>>,
    busy: bool,
    error: Option<String>,
    body: gtk::Box,
    title: adw::WindowTitle,
    auto: gtk::ToggleButton,
}

#[relm4::component(pub)]
impl SimpleComponent for Split {
    type Init = Engine;
    type Input = Input;
    type Output = RootMsg;

    view! {
        adw::ToolbarView {
            add_top_bar = &adw::HeaderBar {
                #[local_ref]
                #[wrap(Some)]
                set_title_widget = title -> adw::WindowTitle {},
                #[local_ref]
                pack_end = auto -> gtk::ToggleButton {
                    #[watch]
                    set_visible: model.view.as_ref().is_some_and(|View(v)| v.auto.is_some()),
                    connect_clicked[sender] => move |_| sender.input(Input::Refresh),
                },
                pack_end = &gtk::Button {
                    set_label: "Clear",
                    #[watch]
                    set_visible: !model.counts.is_empty(),
                    connect_clicked => Input::Clear,
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
                        body -> gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 18,
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
                            add_css_class: "pill",
                            add_css_class: "suggested-action",
                            set_halign: gtk::Align::Center,
                            #[watch]
                            set_label: &if model.busy { "Getting the fee…".to_owned() } else { model.view.as_ref().map_or(String::new(), |View(v)| v.button.clone()) },
                            #[watch]
                            set_sensitive: !model.busy && model.view.as_ref().is_some_and(|View(v)| v.ready),
                            connect_clicked => Input::Prepare,
                        },
                    },
                },
            },
        }
    }

    fn init(engine: Engine, root: Self::Root, sender: ComponentSender<Self>) -> ComponentParts<Self> {
        let body = gtk::Box::new(gtk::Orientation::Vertical, 18);
        let (title, auto) = (adw::WindowTitle::new("", ""), gtk::ToggleButton::new());
        let model = Split { engine, counts: vec![], view: None, busy: false, error: None, body: body.clone(), title: title.clone(), auto: auto.clone() };
        let (body, title, auto) = (&body, &title, &auto);
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, input: Input, sender: ComponentSender<Self>) {
        match input {
            Input::Opened => {
                self.counts.clear();
                self.error = None;
                self.busy = false;
                let (engine, then) = (self.engine.clone(), sender.clone());
                relm4::spawn(async move {
                    // The estimate is asked again once the node's fee rate is in.
                    engine.read(|wallet, _| async move { wallet.refresh_fee_rate().await }).await;
                    then.input(Input::Refresh);
                });
                self.ask(&sender);
            }
            Input::Refresh => {
                // The Auto button: lit means the mix; pressing it unlit brings it back.
                if self.auto.is_active()
                    && let Some(View(v)) = &self.view
                    && !v.is_auto
                    && let Some(auto) = &v.auto
                {
                    sender.input(Input::Counts(auto.counts.clone()));
                    return;
                }
                self.ask(&sender);
            }
            Input::Show(view) => {
                self.draw(&view.0, &sender);
                self.view = Some(view);
            }
            Input::Add(index) => {
                let (engine, counts, sender) = (self.engine.clone(), self.counts.clone(), sender.clone());
                relm4::spawn(async move {
                    match engine.read(move |wallet, _| async move { wallet.split_add(counts, index).await }).await {
                        Some(Ok(counts)) => sender.input(Input::Counts(counts)),
                        Some(Err(e)) => sender.input(Input::Refused(e.to_string())),
                        None => {}
                    }
                });
            }
            Input::Remove(index) => {
                let (engine, counts, sender) = (self.engine.clone(), self.counts.clone(), sender.clone());
                relm4::spawn(async move {
                    if let Some(counts) = engine.read(move |wallet, _| async move { wallet.split_remove(counts, index).await }).await {
                        sender.input(Input::Counts(counts));
                    }
                });
            }
            Input::Counts(counts) => {
                self.counts = counts;
                self.error = None;
                self.ask(&sender);
            }
            Input::Clear => sender.input(Input::Counts(vec![])),
            Input::Refused(why) => {
                self.busy = false;
                self.error = Some(why);
            }
            Input::Prepare => {
                self.busy = true;
                self.error = None;
                let (engine, counts, sender) = (self.engine.clone(), self.counts.clone(), sender.clone());
                relm4::spawn(async move {
                    match engine.read(move |wallet, _| async move { wallet.prepare_split(counts).await }).await {
                        Some(Ok(prepared)) => sender.input(Input::Prepared(View(prepared.into()))),
                        Some(Err(e)) => sender.input(Input::Refused(e.to_string())),
                        None => {}
                    }
                });
            }
            Input::Prepared(prepared) => {
                self.busy = false;
                let _ = sender.output(RootMsg::Go(Go::ConfirmReady(prepared)));
            }
        }
    }
}

impl Split {
    fn ask(&self, sender: &ComponentSender<Self>) {
        let (engine, counts, sender) = (self.engine.clone(), self.counts.clone(), sender.clone());
        relm4::spawn(async move {
            let view = engine.read(move |wallet, clock| async move { wallet.split_view(counts, clock.now, clock.utc_offset, clock.hours_24).await }).await;
            if let Some(view) = view {
                sender.input(Input::Show(View(view.into())));
            }
        });
    }

    fn draw(&mut self, view: &SplitView, sender: &ComponentSender<Self>) {
        self.title.set_title(&view.title);
        if let Some(auto) = &view.auto {
            self.auto.set_label(&auto.label);
            self.auto.set_tooltip_text(Some(&auto.spoken));
        }
        // `clicked` is someone pressing it; this doesn't fire that.
        self.auto.set_active(view.is_auto);

        widgets::empty(&self.body);
        self.body.append(&widgets::dim(&view.intro));
        let tiles = gtk::FlowBox::builder().selection_mode(gtk::SelectionMode::None).min_children_per_line(2).max_children_per_line(3).homogeneous(true).column_spacing(12).row_spacing(12).build();
        for (index, tile) in view.tiles.iter().enumerate() {
            tiles.insert(&Self::tile(index as u32, &tile.label, &tile.count_text, tile.count, &tile.add_label, &tile.remove_label, sender), -1);
        }
        self.body.append(&tiles);
        if view.pieces > 0 {
            let pieces = gtk::Box::new(gtk::Orientation::Vertical, 8);
            pieces.add_css_class("card");
            let inner = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(8).margin_top(12).margin_bottom(12).margin_start(12).margin_end(12).build();
            inner.append(&Self::bar(&view.bar));
            if let Some(summary) = &view.summary {
                inner.append(&widgets::words(summary));
            }
            if let Some(left) = &view.left_over {
                inner.append(&widgets::dim(left));
            }
            pieces.append(&inner);
            self.body.append(&pieces);
        }
        self.body.append(&widgets::centred(&view.footer));
    }

    fn tile(index: u32, label: &str, count_text: &str, count: u32, add: &str, remove: &str, sender: &ComponentSender<Self>) -> gtk::Box {
        let tile = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        tile.add_css_class("linked");
        let pick = gtk::Button::builder().hexpand(true).tooltip_text(add).build();
        let column = gtk::Box::new(gtk::Orientation::Vertical, 2);
        let big = gtk::Label::new(Some(label));
        big.add_css_class("title-3");
        column.append(&big);
        let under = gtk::Label::new(Some(count_text));
        under.add_css_class("dim-label");
        under.add_css_class("caption");
        column.append(&under);
        pick.set_child(Some(&column));
        let then = sender.clone();
        pick.connect_clicked(move |_| then.input(Input::Add(index)));
        tile.append(&pick);
        if count > 0 {
            let less = gtk::Button::builder().icon_name("list-remove-symbolic").tooltip_text(remove).build();
            let then = sender.clone();
            less.connect_clicked(move |_| then.input(Input::Remove(index)));
            tile.append(&less);
        }
        tile
    }

    /// The pieces side by side, each as wide as its value and as tall as its square root.
    fn bar(blocks: &[SplitBlock]) -> gtk::DrawingArea {
        let shapes: Vec<(f64, f64, bool)> = blocks.iter().map(|b| (f64::from(b.width.max(0.001)), f64::from(b.height.clamp(0.12, 1.0)), b.left_over)).collect();
        let area = gtk::DrawingArea::builder().content_height(44).hexpand(true).build();
        area.set_draw_func(move |area, cr: &cairo::Context, width, height| {
            let total: f64 = shapes.iter().map(|s| s.0).sum();
            let (width, height) = (f64::from(width), f64::from(height));
            let colour = area.color();
            let mut x = 0.0;
            for (share, tall, left_over) in &shapes {
                let wide = (share / total * width - 2.0).max(1.0);
                let alpha = if *left_over { 0.25 } else { 1.0 };
                cr.set_source_rgba(f64::from(colour.red()), f64::from(colour.green()), f64::from(colour.blue()), alpha);
                cr.rectangle(x, height * (1.0 - tall), wide, height * tall);
                let _ = cr.fill();
                x += wide + 2.0;
            }
        });
        area
    }
}
