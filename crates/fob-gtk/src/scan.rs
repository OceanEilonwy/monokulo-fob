//! The Scan page: what to look through and what that costs, or how far a
//! running scan has got. With Manual Sync on it's how a wallet catches up; it's
//! also how any wallet looks through past blocks again.

use fob_app::{
    Engine,
    views::{BlockInfo, ScanDrawer, ScanRequest, ScanView, SyncStatus},
};
use relm4::{
    adw::{self, prelude::*},
    gtk::{self, glib},
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
    /// The drawer's words; `true` once the node has said where the chain is.
    Drawer(View<ScanDrawer>, bool),
    Status(View<SyncStatus>),
    Quick(usize),
    Start,
    /// It finished at once, with this to say.
    Checked(String),
    /// It's under way; `Status` shows how it goes.
    Started,
    Failed(String),
    Stop,
    HeightTyped,
    Info(View<BlockInfo>),
    ToDate,
}

pub struct Scan {
    engine: Engine,
    drawer: Option<View<ScanDrawer>>,
    /// The node hasn't said where the chain is since opening: the options show
    /// a spinner for their counts.
    loading: bool,
    running: Option<View<ScanView>>,
    /// A scan was seen running since opening: its end closes the page.
    seen_running: bool,
    picked: usize,
    busy: bool,
    error: Option<String>,
    choosing: gtk::Box,
    tabs: adw::ViewStack,
    options: gtk::ListBox,
    from: gtk::Calendar,
    until: gtk::Calendar,
    use_until: adw::SwitchRow,
    height: adw::EntryRow,
    facts: gtk::ListBox,
    bar: gtk::DrawingArea,
    tip: u64,
}

/// A day's first second here, in seconds since the epoch.
fn midnight(calendar: &gtk::Calendar, days_on: i32) -> Option<u64> {
    let date = calendar.date();
    let day = glib::DateTime::from_local(date.year(), date.month(), date.day_of_month(), 0, 0, 0.0).ok()?;
    u64::try_from(day.add_days(days_on).ok()?.to_unix()).ok()
}

fn draw_bar(area: &gtk::DrawingArea, view: &ScanView) {
    let (progress, marks): (f64, Vec<(f64, bool)>) = (f64::from(view.progress.clamp(0.0, 1.0)), view.marks.iter().map(|m| (f64::from(m.position), m.incoming)).collect());
    area.set_draw_func(move |area, cr, width, height| {
        let (width, height) = (f64::from(width), f64::from(height));
        let ink = area.color();
        cr.set_source_rgba(f64::from(ink.red()), f64::from(ink.green()), f64::from(ink.blue()), 0.15);
        cr.rectangle(0.0, height / 2.0 - 4.0, width, 8.0);
        let _ = cr.fill();
        cr.set_source_rgb(0.702, 0.278, 0.0);
        cr.rectangle(0.0, height / 2.0 - 4.0, width * progress, 8.0);
        let _ = cr.fill();
        // Money in, or money out, in the colours of Activity.
        for (position, incoming) in &marks {
            if *incoming {
                cr.set_source_rgb(0.15, 0.64, 0.41);
            } else {
                cr.set_source_rgba(f64::from(ink.red()), f64::from(ink.green()), f64::from(ink.blue()), 1.0);
            }
            cr.rectangle(width * position - 1.5, 0.0, 3.0, height);
            let _ = cr.fill();
        }
    });
    area.queue_draw();
}

fn copy_request(request: &ScanRequest) -> ScanRequest {
    match request {
        ScanRequest::Unsettled => ScanRequest::Unsettled,
        ScanRequest::FromLast => ScanRequest::FromLast,
        ScanRequest::Back { blocks } => ScanRequest::Back { blocks: *blocks },
        ScanRequest::Whole => ScanRequest::Whole,
        ScanRequest::Height { height } => ScanRequest::Height { height: *height },
        ScanRequest::Dates { from, to } => ScanRequest::Dates { from: *from, to: *to },
    }
}

#[relm4::component(pub)]
impl SimpleComponent for Scan {
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

                        // A scan running.
                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 12,
                            #[watch]
                            set_visible: model.running.is_some(),
                            gtk::Label {
                                add_css_class: "title-3",
                                set_wrap: true,
                                #[watch]
                                set_label: model.running.as_ref().map_or("", |View(s)| s.title.as_str()),
                            },
                            gtk::Label {
                                add_css_class: "dim-label",
                                set_wrap: true,
                                set_label: "You can leave this page. Fob keeps going, and you can still send.",
                            },
                            #[local_ref]
                            bar -> gtk::DrawingArea {
                                set_content_height: 22,
                                set_hexpand: true,
                            },
                            gtk::Label {
                                #[watch]
                                set_label: model.running.as_ref().map_or("", |View(s)| s.blocks.as_str()),
                            },
                            gtk::Button {
                                set_label: "Stop",
                                add_css_class: "pill",
                                set_halign: gtk::Align::Center,
                                connect_clicked => Input::Stop,
                            },
                        },

                        // What to look through.
                        #[local_ref]
                        choosing -> gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 14,
                            #[watch]
                            set_visible: model.running.is_none(),
                            gtk::Label {
                                add_css_class: "dim-label",
                                set_wrap: true,
                                set_xalign: 0.0,
                                #[watch]
                                set_label: model.drawer.as_ref().map_or("", |View(d)| d.last_scan.as_str()),
                            },
                            adw::InlineViewSwitcher {
                                set_display_mode: adw::InlineViewSwitcherDisplayMode::Labels,
                                set_homogeneous: true,
                                set_stack: Some(tabs),
                            },
                            #[local_ref]
                            tabs -> adw::ViewStack {
                                set_vhomogeneous: false,
                                connect_visible_child_name_notify[sender] => move |_| sender.input(Input::HeightTyped),
                            },
                            gtk::Label {
                                add_css_class: "error",
                                set_wrap: true,
                                #[watch]
                                set_visible: model.error.is_some(),
                                #[watch]
                                set_label: model.error.as_deref().unwrap_or(""),
                            },
                            gtk::Label {
                                add_css_class: "dim-label",
                                set_wrap: true,
                                set_justify: gtk::Justification::Center,
                                #[watch]
                                set_label: &model.cost(),
                                #[watch]
                                set_visible: !model.cost().is_empty(),
                            },
                            gtk::Button {
                                add_css_class: "pill",
                                add_css_class: "suggested-action",
                                set_halign: gtk::Align::Center,
                                #[watch]
                                set_label: &model.action(),
                                #[watch]
                                set_sensitive: !model.busy && model.can_start(),
                                connect_clicked => Input::Start,
                            },
                        },
                    },
                },
            },
        }
    }

    fn init(engine: Engine, root: Self::Root, sender: ComponentSender<Self>) -> ComponentParts<Self> {
        let (choosing, tabs, bar) = (gtk::Box::new(gtk::Orientation::Vertical, 14), adw::ViewStack::new(), gtk::DrawingArea::new());
        let options = widgets::list();
        tabs.add_titled(&options, Some("quick"), "Quick");

        let (from, until) = (gtk::Calendar::new(), gtk::Calendar::new());
        let use_until = adw::SwitchRow::builder().title("Stop at a date").subtitle("Without one, the scan runs up to now").build();
        let dates = gtk::Box::new(gtk::Orientation::Vertical, 12);
        dates.append(&widgets::dim("From"));
        dates.append(&from);
        let stop = widgets::list();
        stop.append(&use_until);
        dates.append(&stop);
        dates.append(&until);
        until.set_visible(false);
        let then = sender.clone();
        use_until.connect_active_notify(move |_| then.input(Input::ToDate));
        tabs.add_titled(&dates, Some("dates"), "Dates");

        let height = adw::EntryRow::builder().title("From block").input_purpose(gtk::InputPurpose::Digits).build();
        let then = sender.clone();
        height.connect_changed(move |_| then.input(Input::HeightTyped));
        let facts = widgets::list();
        let block = gtk::Box::new(gtk::Orientation::Vertical, 12);
        let field = widgets::list();
        field.append(&height);
        block.append(&field);
        block.append(&facts);
        tabs.add_titled(&block, Some("block"), "Block");

        let model = Scan {
            engine,
            drawer: None,
            loading: false,
            running: None,
            seen_running: false,
            picked: 1,
            busy: false,
            error: None,
            choosing: choosing.clone(),
            tabs: tabs.clone(),
            options,
            from,
            until,
            use_until,
            height,
            facts,
            bar: bar.clone(),
            tip: 0,
        };
        let (choosing, tabs, bar) = (&choosing, &tabs, &bar);
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, input: Input, sender: ComponentSender<Self>) {
        match input {
            Input::Opened => {
                self.error = None;
                self.busy = false;
                self.seen_running = self.running.is_some();
                self.picked = 1;
                self.loading = true;
                // The choices show at once, a spinner under each, while the node
                // is asked where the chain is.
                if let Some(View(drawer)) = &self.drawer {
                    self.draw_options(drawer, &sender);
                } else {
                    let (engine, sender) = (self.engine.clone(), sender.clone());
                    relm4::spawn(async move {
                        if let Some(drawer) = engine.read(|wallet, clock| async move { wallet.scan_drawer_known(clock.now, clock.utc_offset, clock.hours_24).await }).await {
                            sender.input(Input::Drawer(View(drawer.into()), false));
                        }
                    });
                }
                let (engine, sender) = (self.engine.clone(), sender.clone());
                relm4::spawn(async move {
                    if let Some(drawer) = engine.read(|wallet, clock| async move { wallet.scan_drawer(clock.now, clock.utc_offset, clock.hours_24).await }).await {
                        sender.input(Input::Drawer(View(drawer.into()), true));
                    }
                });
            }
            Input::Drawer(drawer, fresh) => {
                // What was known before asking, arriving after the answer, is stale.
                if !fresh && !self.loading {
                    return;
                }
                self.loading = !fresh;
                self.draw_options(&drawer.0, &sender);
                self.drawer = Some(drawer);
            }
            Input::Status(View(status)) => {
                self.tip = status.tip;
                match &status.scan {
                    Some(scan) => {
                        self.seen_running = true;
                        draw_bar(&self.bar, scan);
                        self.running = Some(View(scan_copy(scan).into()));
                    }
                    None => {
                        let was_running = self.running.take().is_some();
                        if was_running && self.seen_running && self.choosing.is_mapped_or_parent() {
                            self.seen_running = false;
                            let _ = sender.output(RootMsg::Go(Go::ScanOver));
                        }
                    }
                }
            }
            Input::Quick(index) => self.picked = index,
            Input::Start => self.start(&sender),
            Input::Checked(said) => {
                self.busy = false;
                let _ = sender.output(RootMsg::Say(said));
                let _ = sender.output(RootMsg::Go(Go::ScanOver));
            }
            Input::Started => self.busy = false,
            Input::Failed(why) => {
                self.busy = false;
                self.error = Some(why);
            }
            Input::Stop => {
                let engine = self.engine.clone();
                relm4::spawn(async move {
                    engine.read(|wallet, _| async move { wallet.stop_scan().await }).await;
                    engine.refresh().await;
                });
            }
            Input::HeightTyped => self.ask_block(&sender),
            Input::Info(View(info)) => {
                widgets::clear(&self.facts);
                let fact = |title: &str, value: &str| {
                    let row = widgets::row(title, value);
                    row.add_css_class("property");
                    self.facts.append(&row);
                };
                if let Some(mined) = &info.mined {
                    fact("Mined", mined);
                }
                fact("Newest block", &info.newest);
                fact("Wallet's first block", &info.first);
                if self.typed_height().is_some_and(|block| self.tip > 0 && block >= self.tip) {
                    self.error = Some("That's past the newest block.".into());
                } else {
                    self.error = None;
                }
            }
            Input::ToDate => self.until.set_visible(self.use_until.is_active()),
        }
    }
}

/// An option's row while its count is asked for: a small spinner where the line
/// under the title goes, so the row keeps its height when the words come.
fn counting_row(title: &str) -> adw::ActionRow {
    let row = widgets::row(title, "");
    let spinner = gtk::Spinner::builder().spinning(true).halign(gtk::Align::Start).width_request(16).height_request(16).build();
    spinner.add_css_class("dim-label");
    spinner.update_property(&[gtk::accessible::Property::Label("Counting")]);
    match subtitle_of(row.upcast_ref()) {
        Some(subtitle) => {
            if let Some(parent) = subtitle.parent().and_downcast::<gtk::Box>() {
                parent.insert_child_after(&spinner, Some(&subtitle));
            }
        }
        // Where the line goes is libadwaita's to arrange; without it, at the end.
        None => row.add_suffix(&spinner),
    }
    row
}

/// The label an `ActionRow` writes its subtitle in.
fn subtitle_of(widget: &gtk::Widget) -> Option<gtk::Label> {
    let mut child = widget.first_child();
    while let Some(widget) = child {
        if let Some(label) = widget.downcast_ref::<gtk::Label>().filter(|label| label.has_css_class("subtitle")) {
            return Some(label.clone());
        }
        if let Some(found) = subtitle_of(&widget) {
            return Some(found);
        }
        child = widget.next_sibling();
    }
    None
}

/// `ScanView` isn't `Clone`, as it crosses to other languages.
fn scan_copy(scan: &ScanView) -> ScanView {
    ScanView {
        title: scan.title.clone(),
        blocks: scan.blocks.clone(),
        progress: scan.progress,
        marks: scan.marks.iter().map(|m| fob_app::views::ScanMark { position: m.position, incoming: m.incoming }).collect(),
    }
}

trait OnScreen {
    fn is_mapped_or_parent(&self) -> bool;
}

impl OnScreen for gtk::Box {
    /// The box is hidden while a scan runs, so its page showing is what counts.
    fn is_mapped_or_parent(&self) -> bool {
        self.parent().is_some_and(|parent| parent.is_mapped())
    }
}

impl Scan {
    fn tab(&self) -> String {
        self.tabs.visible_child_name().map_or("quick".into(), |name| name.to_string())
    }

    fn typed_height(&self) -> Option<u64> {
        self.height.text().trim().parse().ok()
    }

    fn cost(&self) -> String {
        match (self.tab().as_str(), &self.drawer) {
            ("quick", Some(View(drawer))) if !self.loading => drawer.options.get(self.picked).map_or(String::new(), |option| option.cost.clone()),
            _ => String::new(),
        }
    }

    fn action(&self) -> String {
        match (self.tab().as_str(), &self.drawer) {
            ("dates", _) => "Scan these dates".into(),
            ("block", _) => "Scan from this block".into(),
            (_, Some(View(drawer))) if !self.loading => drawer.options.get(self.picked).map_or("Scan".into(), |option| option.action.clone()),
            _ => "Scan".into(),
        }
    }

    fn can_start(&self) -> bool {
        match self.tab().as_str() {
            "dates" => true,
            "block" => self.typed_height().is_some_and(|block| self.tip == 0 || block < self.tip),
            _ => self.drawer.is_some() && !self.loading,
        }
    }

    fn request(&self) -> Option<ScanRequest> {
        match self.tab().as_str() {
            "dates" => Some(ScanRequest::Dates { from: midnight(&self.from, 0)?, to: if self.use_until.is_active() { Some(midnight(&self.until, 1)?) } else { None } }),
            "block" => Some(ScanRequest::Height { height: self.typed_height()? }),
            _ => self.drawer.as_ref().and_then(|View(drawer)| drawer.options.get(self.picked)).map(|option| copy_request(&option.request)),
        }
    }

    fn start(&mut self, sender: &ComponentSender<Self>) {
        let Some(request) = self.request() else { return };
        self.busy = true;
        self.error = None;
        let (engine, sender) = (self.engine.clone(), sender.clone());
        relm4::spawn(async move {
            match engine.change(move |wallet| async move { wallet.start_scan(request).await }).await {
                Ok(Some(said)) => sender.input(Input::Checked(format!("{} · {}", said.title, said.line))),
                Ok(None) => sender.input(Input::Started),
                Err(e) => sender.input(Input::Failed(e.to_string())),
            }
        });
    }

    fn ask_block(&self, sender: &ComponentSender<Self>) {
        let (engine, block, sender) = (self.engine.clone(), self.typed_height().unwrap_or(0), sender.clone());
        relm4::spawn(async move {
            if let Some(info) = engine.read(move |wallet, clock| async move { wallet.block_info(block, clock.now, clock.utc_offset, clock.hours_24).await }).await {
                sender.input(Input::Info(View(info.into())));
            }
        });
    }

    fn draw_options(&self, drawer: &ScanDrawer, sender: &ComponentSender<Self>) {
        widgets::clear(&self.options);
        // One chosen, marked with a tick at the row's end as the wallets are: a radio
        // button's round border drew stray marks at some window sizes.
        let ticks: std::rc::Rc<Vec<gtk::Image>> = std::rc::Rc::new(
            (0..drawer.options.len()).map(|index| gtk::Image::builder().icon_name("object-select-symbolic").visible(index == self.picked).build()).collect(),
        );
        for (index, option) in drawer.options.iter().enumerate() {
            let row = if self.loading { counting_row(&option.label) } else { widgets::row(&option.label, &option.note) };
            row.add_suffix(&ticks[index]);
            row.set_activatable(true);
            let (then, ticks) = (sender.clone(), ticks.clone());
            row.connect_activated(move |_| {
                for (i, tick) in ticks.iter().enumerate() {
                    tick.set_visible(i == index);
                }
                then.input(Input::Quick(index));
            });
            self.options.append(&row);
        }
    }
}
