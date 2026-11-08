//! A Trezor's exchange on screen: one card that mirrors the device and says
//! what to do there. The device drives; Fob draws a Safe 3 with what its
//! screen shows, counts the steps of connecting a wallet off at the corner,
//! and asks the one question of its own (passphrase or not) before the
//! device would. Add a wallet shows the card in its form; Home and Confirm
//! show it in a dialog. The card's words are Rust's ([`words`]), shared
//! with Android.

use std::{
    cell::RefCell,
    rc::{Rc, Weak},
    sync::{Arc, mpsc},
};

use fob_app::{Prompter, TrezorAnswer, TrezorAsk, TrezorRequest, Zeroizing, views::TrezorPassphrase, words::trezor as words};
use relm4::{
    Component, ComponentSender,
    adw::{self, prelude::*},
    gtk,
};

use crate::root::Msg as RootMsg;

/// A prompter whose questions go to the root as [`RootMsg::Trezor`], from a
/// screen whose output is the root's messages.
pub fn prompter<C: Component<Output = RootMsg>>(sender: &ComponentSender<C>) -> Arc<Prompter> {
    let out = sender.output_sender().clone();
    Prompter::new(move |request| {
        let _ = out.send(RootMsg::Trezor(request));
    })
}

/// What the person did on the card.
#[derive(Clone, Debug)]
pub enum Action {
    /// The X: the exchange is given up, whatever it was doing.
    Cancel,
    /// Try again, after a problem.
    Retry,
    NoPassphrase,
    OnDevice,
    TypeHere,
    BackToDevice,
    Passphrase(Zeroizing<String>),
    /// The keypad fallback's answer; `None` cancels the PIN.
    Pin(Option<String>),
}

/// What came of an action, for the screen that owns the card.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Nothing,
    /// Start the exchange over.
    Retry,
    /// The card is done with: hide it.
    Cancelled,
}

/// What the device's screen shows in the drawing.
#[derive(Clone, Debug, PartialEq)]
pub enum Screen {
    Dark,
    Pin,
    Passphrase,
    Confirm(&'static str),
    Address,
    Syncing(f64),
}

/// The drawing beside the words.
#[derive(Clone, Debug, PartialEq)]
pub enum Art {
    Device(Screen),
    Problem { stop: bool },
}

/// Under the words.
#[derive(Clone, Debug)]
pub enum Extra {
    None,
    Spinner(String),
    Progress(f64, String),
    PassphraseQuestion { on_device: bool },
    PassphraseField,
    PinPad,
    Actions(Vec<(String, Action, bool)>),
}

/// Everything the card draws.
#[derive(Clone, Debug)]
pub struct View {
    pub card: words::Card,
    pub step: Option<String>,
    pub art: Art,
    pub extra: Extra,
    pub tone: Tone,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Look,
    Problem,
    Stop,
}

/// The exchange as the card sees it: the last question, where to answer
/// it, and what the hello said the device will ask.
#[derive(Default)]
pub struct Flow {
    device: words::Device,
    name: String,
    ask: Option<TrezorAsk>,
    reply: Option<mpsc::Sender<TrezorAnswer>>,
    cancel: Option<Arc<dyn Fn() + Send + Sync>>,
    /// Yes, type it here was chosen.
    typed: bool,
    problem: Option<words::Problem>,
    /// The person pressed the X: what the engine says after isn't shown.
    dismissed: bool,
    /// There's something to show.
    pub live: bool,
}

impl Flow {
    /// A question from the device, or the end of the exchange.
    pub fn take(&mut self, request: TrezorRequest) {
        let TrezorRequest { ask, reply, cancel } = request;
        match ask {
            TrezorAsk::Over => {
                self.ask = None;
                self.reply = None;
                self.cancel = None;
                self.live = self.problem.is_some();
            }
            TrezorAsk::Hello { name, pin, passphrase } => {
                self.name = name;
                self.device = words::Device { pin, passphrase };
                self.cancel = Some(cancel);
            }
            TrezorAsk::Looking => {
                // A fresh exchange: what went before is forgotten.
                self.problem = None;
                self.dismissed = false;
                self.device = words::Device::default();
                self.typed = false;
                self.ask = Some(TrezorAsk::Looking);
                self.reply = None;
                self.cancel = Some(cancel);
                self.live = true;
            }
            other => {
                self.typed = false;
                self.ask = Some(other);
                self.reply = reply;
                self.cancel = Some(cancel);
                self.live = true;
            }
        }
    }

    /// The exchange failed: `reason` in Fob's words, `cancelled` if the
    /// person said no. Whether the card has something to show for it (not
    /// after the X: that's an end the person chose).
    pub fn fail(&mut self, reason: &str, cancelled: bool) -> bool {
        self.ask = None;
        self.reply = None;
        if self.dismissed {
            self.dismissed = false;
            self.live = false;
            return false;
        }
        self.problem = Some(words::problem(reason, cancelled));
        self.live = true;
        true
    }

    /// The exchange is over and the wallet is here: nothing left to show.
    pub fn done(&mut self) {
        *self = Flow::default();
    }

    pub fn act(&mut self, action: Action) -> Outcome {
        match action {
            Action::Cancel => {
                self.dismissed = self.ask.is_some();
                if let Some(reply) = self.reply.take() {
                    let answer = match self.ask {
                        Some(TrezorAsk::Pin) => TrezorAnswer::Pin(None),
                        _ => TrezorAnswer::Passphrase(TrezorPassphrase::Cancelled),
                    };
                    let _ = reply.send(answer);
                }
                if let Some(cancel) = self.cancel.take() {
                    cancel();
                }
                self.ask = None;
                self.problem = None;
                self.live = false;
                Outcome::Cancelled
            }
            Action::Retry => {
                self.problem = None;
                self.live = false;
                Outcome::Retry
            }
            Action::NoPassphrase => self.answer(TrezorAnswer::Passphrase(TrezorPassphrase::Typed { text: String::new() })),
            Action::OnDevice => self.answer(TrezorAnswer::Passphrase(TrezorPassphrase::OnDevice)),
            Action::TypeHere => {
                self.typed = true;
                Outcome::Nothing
            }
            Action::BackToDevice => {
                self.typed = false;
                Outcome::Nothing
            }
            Action::Passphrase(text) => self.answer(TrezorAnswer::Passphrase(TrezorPassphrase::Typed { text: String::clone(&text) })),
            Action::Pin(pin) => self.answer(TrezorAnswer::Pin(pin.map(Zeroizing::new))),
        }
    }

    fn answer(&mut self, answer: TrezorAnswer) -> Outcome {
        if let Some(reply) = self.reply.take() {
            let _ = reply.send(answer);
        }
        // The device takes it from here: the drawing shows it busy.
        self.ask = Some(TrezorAsk::Button(String::new()));
        self.typed = false;
        Outcome::Nothing
    }

    pub fn view(&self) -> View {
        if let Some(problem) = &self.problem {
            let actions = if problem.retry { vec![(words::TRY_AGAIN.into(), Action::Retry, true)] } else { vec![] };
            return View {
                card: problem.card.clone(),
                step: None,
                art: Art::Problem { stop: problem.stop },
                extra: Extra::Actions(actions),
                tone: if problem.stop { Tone::Stop } else { Tone::Problem },
            };
        }
        let place = |step| self.device.place_text(step);
        match &self.ask {
            None | Some(TrezorAsk::Looking) | Some(TrezorAsk::Over) | Some(TrezorAsk::Hello { .. }) => {
                View { card: words::looking(), step: None, art: Art::Device(Screen::Dark), extra: Extra::Spinner(words::LOOKING_HINT.into()), tone: Tone::Look }
            }
            Some(TrezorAsk::Button(what)) => {
                let w = what.to_ascii_lowercase();
                let screen = if w.contains("pin") {
                    Screen::Pin
                } else if w.contains("address") {
                    Screen::Address
                } else if w.contains("watch") || w.contains("export") {
                    Screen::Confirm("Export watch-only\ncredentials?")
                } else if w.contains("key_image") || w.contains("sync") {
                    Screen::Confirm("Sync key images?")
                } else if w.is_empty() {
                    Screen::Dark
                } else {
                    Screen::Confirm("Confirm")
                };
                let card = if w.is_empty() { words::Card { eyebrow: words::LOOK.into(), title: "The Trezor is working.".into(), body: "Leave it plugged in.".into() } } else { words::button(what) };
                View { card, step: words::step_for(what).and_then(place), art: Art::Device(screen), extra: Extra::None, tone: Tone::Look }
            }
            Some(TrezorAsk::Pin) => View { card: words::pin(), step: place(words::Step::Unlock), art: Art::Device(Screen::Pin), extra: Extra::PinPad, tone: Tone::Look },
            Some(TrezorAsk::Passphrase { on_device }) => {
                let (card, extra) = if self.typed { (words::passphrase_typed(), Extra::PassphraseField) } else { (words::passphrase(), Extra::PassphraseQuestion { on_device: *on_device }) };
                View { card, step: place(words::Step::Wallet), art: Art::Device(Screen::Passphrase), extra, tone: Tone::Look }
            }
            Some(TrezorAsk::Progress { done, total }) => {
                let fraction = if *total == 0 { 0.0 } else { f64::from(*done) / f64::from(*total) };
                View { card: words::progress(*done, *total), step: None, art: Art::Device(Screen::Syncing(fraction)), extra: Extra::Progress(fraction, words::KEPT_IF_CANCELLED.into()), tone: Tone::Look }
            }
        }
    }
}

/// The card on screen.
pub struct Card {
    root: gtk::Box,
    eyebrow: gtk::Label,
    step: gtk::Label,
    art: gtk::DrawingArea,
    art_state: Rc<RefCell<Art>>,
    title: gtk::Label,
    body: gtk::Label,
    extra: gtk::Box,
    on: Rc<dyn Fn(Action)>,
}

impl Card {
    pub fn new(on: impl Fn(Action) + 'static) -> Card {
        let root = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(12).css_classes(["card", "trezor-card"]).build();
        let header = gtk::Box::builder().orientation(gtk::Orientation::Horizontal).spacing(10).build();
        let eyebrow = gtk::Label::builder().css_classes(["trezor-eyebrow"]).halign(gtk::Align::Start).hexpand(true).xalign(0.0).build();
        let step = gtk::Label::builder().css_classes(["dim-label", "caption"]).halign(gtk::Align::End).build();
        header.append(&eyebrow);
        header.append(&step);
        root.append(&header);

        let row = gtk::Box::builder().orientation(gtk::Orientation::Horizontal).spacing(18).build();
        let art_state = Rc::new(RefCell::new(Art::Device(Screen::Dark)));
        let art = gtk::DrawingArea::builder().content_width(118).content_height(184).valign(gtk::Align::Center).build();
        art.update_property(&[gtk::accessible::Property::Label("The Trezor")]);
        {
            let state = art_state.clone();
            art.set_draw_func(move |_, cr, width, height| draw(cr, width, height, &state.borrow()));
        }
        row.append(&art);
        let column = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(8).hexpand(true).valign(gtk::Align::Center).build();
        let title = gtk::Label::builder().css_classes(["title-4"]).wrap(true).xalign(0.0).halign(gtk::Align::Start).build();
        let body = gtk::Label::builder().css_classes(["dim-label"]).wrap(true).xalign(0.0).halign(gtk::Align::Start).selectable(false).build();
        let extra = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(8).build();
        column.append(&title);
        column.append(&body);
        column.append(&extra);
        row.append(&column);
        root.append(&row);
        Card { root, eyebrow, step, art, art_state, title, body, extra, on: Rc::new(on) }
    }

    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }

    pub fn show(&self, view: &View) {
        self.eyebrow.set_label(&view.card.eyebrow);
        for class in ["problem", "stop"] {
            self.eyebrow.remove_css_class(class);
        }
        match view.tone {
            Tone::Look => {}
            Tone::Problem => self.eyebrow.add_css_class("problem"),
            Tone::Stop => self.eyebrow.add_css_class("stop"),
        }
        self.step.set_label(view.step.as_deref().unwrap_or(""));
        self.step.set_visible(view.step.is_some());
        self.title.set_label(&view.card.title);
        self.body.set_label(&view.card.body);
        *self.art_state.borrow_mut() = view.art.clone();
        self.art.queue_draw();
        while let Some(child) = self.extra.first_child() {
            self.extra.remove(&child);
        }
        let on = self.on.clone();
        match &view.extra {
            Extra::None => {}
            Extra::Spinner(text) => {
                let line = gtk::Box::builder().orientation(gtk::Orientation::Horizontal).spacing(10).build();
                line.append(&adw::Spinner::builder().width_request(16).height_request(16).build());
                line.append(&gtk::Label::builder().label(text).css_classes(["dim-label", "caption"]).build());
                self.extra.append(&line);
            }
            Extra::Progress(fraction, hint) => {
                self.extra.append(&gtk::ProgressBar::builder().fraction(*fraction).build());
                self.extra.append(&gtk::Label::builder().label(hint).css_classes(["dim-label", "caption"]).xalign(0.0).build());
            }
            Extra::PassphraseQuestion { on_device } => {
                // Three answers: they wrap onto a second line when the card is narrow.
                let buttons = gtk::FlowBox::builder().selection_mode(gtk::SelectionMode::None).column_spacing(8).row_spacing(8).min_children_per_line(1).max_children_per_line(3).halign(gtk::Align::End).margin_top(4).build();
                buttons.append(&button(words::TYPE_HERE, false, on.clone(), Action::TypeHere));
                if *on_device {
                    buttons.append(&button(words::ON_THE_TREZOR, false, on.clone(), Action::OnDevice));
                }
                buttons.append(&button(words::NO_PASSPHRASE, true, on.clone(), Action::NoPassphrase));
                self.extra.append(&buttons);
            }
            Extra::PassphraseField => {
                let entry = gtk::PasswordEntry::builder().show_peek_icon(true).placeholder_text(words::PASSPHRASE_FIELD).activates_default(false).build();
                entry.update_property(&[gtk::accessible::Property::Label(words::PASSPHRASE_FIELD)]);
                self.extra.append(&entry);
                let buttons = gtk::Box::builder().orientation(gtk::Orientation::Horizontal).spacing(8).margin_top(4).build();
                let back = gtk::Button::builder().label(words::ENTER_ON_TREZOR_INSTEAD).css_classes(["flat"]).halign(gtk::Align::Start).hexpand(true).build();
                {
                    let on = on.clone();
                    back.connect_clicked(move |_| on(Action::BackToDevice));
                }
                buttons.append(&back);
                let open = gtk::Button::builder().label(words::OPEN_WALLET).css_classes(["suggested-action"]).build();
                {
                    let on = on.clone();
                    let field = entry.clone();
                    let send = Rc::new(move || {
                        let typed = crate::widgets::secret_text(field.text());
                        field.set_text("");
                        on(Action::Passphrase(typed));
                    });
                    let s = send.clone();
                    open.connect_clicked(move |_| s());
                    entry.connect_activate(move |_| send());
                }
                buttons.append(&open);
                self.extra.append(&buttons);
            }
            Extra::PinPad => self.extra.append(&pin_pad(on)),
            Extra::Actions(actions) => {
                if actions.is_empty() {
                    return;
                }
                let buttons = gtk::Box::builder().orientation(gtk::Orientation::Horizontal).spacing(8).halign(gtk::Align::End).margin_top(4).build();
                for (label, action, suggested) in actions {
                    buttons.append(&button(label, *suggested, on.clone(), action.clone()));
                }
                self.extra.append(&buttons);
            }
        }
    }
}

fn button(label: &str, suggested: bool, on: Rc<dyn Fn(Action)>, action: Action) -> gtk::Button {
    let b = gtk::Button::builder().label(label).build();
    if suggested {
        b.add_css_class("suggested-action");
    }
    b.connect_clicked(move |_| on(action.clone()));
    b
}

/// The keypad, for a device that wants the PIN typed here: the device's
/// 3 × 3 with the digits hidden, answered by position.
fn pin_pad(on: Rc<dyn Fn(Action)>) -> gtk::Box {
    let typed: Rc<RefCell<String>> = Rc::new(RefCell::new(String::new()));
    let dots = gtk::Label::builder().label(" ").css_classes(["title-2"]).build();
    dots.update_property(&[gtk::accessible::Property::Label("Digits typed")]);
    let grid = gtk::Grid::builder().row_spacing(6).column_spacing(6).halign(gtk::Align::Start).build();
    for (row, positions) in [[7, 8, 9], [4, 5, 6], [1, 2, 3]].iter().enumerate() {
        for (column, position) in positions.iter().enumerate() {
            let key = gtk::Button::builder().label("•").width_request(56).height_request(40).build();
            key.update_property(&[gtk::accessible::Property::Label(&format!("Keypad position {position}"))]);
            let (typed, dots, position) = (typed.clone(), dots.clone(), *position);
            key.connect_clicked(move |_| {
                let mut t = typed.borrow_mut();
                if t.len() < 50 {
                    t.push(char::from(b'0' + position));
                }
                dots.set_label(&"•".repeat(t.len()));
            });
            grid.attach(&key, column as i32, row as i32, 1, 1);
        }
    }
    let column = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(8).build();
    column.append(&dots);
    column.append(&grid);
    let buttons = gtk::Box::builder().orientation(gtk::Orientation::Horizontal).spacing(8).build();
    let delete = gtk::Button::builder().label(words::PIN_DELETE).build();
    {
        let (typed, dots) = (typed.clone(), dots.clone());
        delete.connect_clicked(move |_| {
            let mut t = typed.borrow_mut();
            t.pop();
            let shown = "•".repeat(t.len());
            dots.set_label(if t.is_empty() { " " } else { &shown });
        });
    }
    buttons.append(&delete);
    let enter = gtk::Button::builder().label(words::PIN_ENTER).css_classes(["suggested-action"]).build();
    {
        let typed = typed.clone();
        enter.connect_clicked(move |_| {
            let pin = std::mem::take(&mut *typed.borrow_mut());
            on(Action::Pin((!pin.is_empty()).then_some(pin)));
        });
    }
    buttons.append(&enter);
    column.append(&buttons);
    column
}

/// The Safe 3, as the device is drawn everywhere: a dark body, its screen,
/// two flat buttons under it, the wordmark at the foot. What the screen
/// shows follows the exchange. A problem or the end replaces the screen
/// with a mark.
fn draw(cr: &gtk::cairo::Context, width: i32, height: i32, art: &Art) {
    let (w, h) = (f64::from(width), f64::from(height));
    rounded(cr, 1.0, 1.0, w - 2.0, h - 2.0, 18.0);
    cr.set_source_rgb(0.067, 0.067, 0.067);
    let _ = cr.fill_preserve();
    cr.set_source_rgb(0.29, 0.29, 0.29);
    cr.set_line_width(2.0);
    let _ = cr.stroke();
    // The screen.
    let (sx, sy, sw, sh) = (12.0, 18.0, w - 24.0, 62.0);
    rounded(cr, sx, sy, sw, sh, 4.0);
    cr.set_source_rgb(0.0, 0.0, 0.0);
    let _ = cr.fill_preserve();
    cr.set_source_rgb(0.2, 0.2, 0.2);
    cr.set_line_width(1.0);
    let _ = cr.stroke();
    // Two buttons, and the wordmark.
    for x in [sx, sx + sw - 40.0] {
        rounded(cr, x, sy + sh + 10.0, 40.0, 16.0, 4.0);
        cr.set_source_rgb(0.2, 0.2, 0.2);
        let _ = cr.fill_preserve();
        cr.set_source_rgb(0.33, 0.33, 0.33);
        let _ = cr.stroke();
    }
    cr.select_font_face("Sans", gtk::cairo::FontSlant::Normal, gtk::cairo::FontWeight::Bold);
    cr.set_font_size(8.0);
    cr.set_source_rgb(0.54, 0.54, 0.54);
    let mark = "T R E Z O R";
    if let Ok(extents) = cr.text_extents(mark) {
        cr.move_to((w - extents.width()) / 2.0, h - 14.0);
        let _ = cr.show_text(mark);
    }
    // What the screen shows.
    let centred = |text: &str, y: f64, size: f64, (r, g, b): (f64, f64, f64)| {
        cr.set_font_size(size);
        cr.set_source_rgb(r, g, b);
        if let Ok(extents) = cr.text_extents(text) {
            cr.move_to(sx + (sw - extents.width()) / 2.0, y);
            let _ = cr.show_text(text);
        }
    };
    let white = (1.0, 1.0, 1.0);
    let grey = (0.6, 0.6, 0.6);
    cr.select_font_face("Sans", gtk::cairo::FontSlant::Normal, gtk::cairo::FontWeight::Bold);
    match art {
        Art::Device(Screen::Dark) => {}
        Art::Device(Screen::Pin) => {
            centred("Enter PIN", sy + 18.0, 9.0, white);
            for row in 0..2 {
                for column in 0..3 {
                    cr.rectangle(sx + 28.0 + f64::from(column) * 14.0, sy + 26.0 + f64::from(row) * 11.0, 10.0, 8.0);
                }
            }
            cr.set_source_rgb(0.45, 0.45, 0.45);
            cr.set_line_width(1.0);
            let _ = cr.stroke();
        }
        Art::Device(Screen::Passphrase) => {
            centred("Enter passphrase", sy + 18.0, 8.5, white);
            cr.rectangle(sx + 12.0, sy + 26.0, sw - 24.0, 10.0);
            cr.set_source_rgb(0.45, 0.45, 0.45);
            let _ = cr.stroke();
            centred("empty = standard", sy + 50.0, 7.0, grey);
        }
        Art::Device(Screen::Confirm(text)) => {
            let lines: Vec<&str> = text.lines().collect();
            let mut y = sy + 16.0;
            for line in &lines {
                centred(line, y, 8.0, white);
                y += 11.0;
            }
            centred("Confirm", sy + sh - 8.0, 8.0, (0.5, 0.83, 0.6));
        }
        Art::Device(Screen::Address) => {
            centred("Address 1/3", sy + 14.0, 7.0, grey);
            cr.select_font_face("Monospace", gtk::cairo::FontSlant::Normal, gtk::cairo::FontWeight::Bold);
            centred("5A3kR 9wqM2", sy + 30.0, 8.0, white);
            centred("xT7Pb Vn4Lc", sy + 42.0, 8.0, white);
            centred("8sYe2 hQjDm", sy + 54.0, 8.0, white);
        }
        Art::Device(Screen::Syncing(fraction)) => {
            centred("Syncing…", sy + 22.0, 9.0, white);
            rounded(cr, sx + 12.0, sy + 34.0, sw - 24.0, 6.0, 3.0);
            cr.set_source_rgb(0.2, 0.2, 0.2);
            let _ = cr.fill();
            rounded(cr, sx + 12.0, sy + 34.0, (sw - 24.0) * fraction.clamp(0.0, 1.0), 6.0, 3.0);
            cr.set_source_rgb(0.5, 0.83, 0.6);
            let _ = cr.fill();
        }
        Art::Problem { stop } => {
            let (r, g, b) = if *stop { (0.88, 0.27, 0.24) } else { (1.0, 0.54, 0.5) };
            cr.arc(w / 2.0, h / 2.0, 26.0, 0.0, std::f64::consts::TAU);
            cr.set_source_rgb(r * 0.35, g * 0.35, b * 0.35);
            let _ = cr.fill();
            cr.set_source_rgb(r, g, b);
            cr.set_line_width(3.0);
            if *stop {
                cr.move_to(w / 2.0 - 9.0, h / 2.0 - 9.0);
                cr.line_to(w / 2.0 + 9.0, h / 2.0 + 9.0);
                cr.move_to(w / 2.0 + 9.0, h / 2.0 - 9.0);
                cr.line_to(w / 2.0 - 9.0, h / 2.0 + 9.0);
            } else {
                cr.move_to(w / 2.0, h / 2.0 - 10.0);
                cr.line_to(w / 2.0, h / 2.0 + 3.0);
                cr.move_to(w / 2.0, h / 2.0 + 9.0);
                cr.line_to(w / 2.0, h / 2.0 + 9.5);
            }
            let _ = cr.stroke();
        }
    }
}

fn rounded(cr: &gtk::cairo::Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    use std::f64::consts::PI;
    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -PI / 2.0, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, PI / 2.0);
    cr.arc(x + r, y + h - r, r, PI / 2.0, PI);
    cr.arc(x + r, y + r, r, PI, 3.0 * PI / 2.0);
    cr.close_path();
}

/// The card in a dialog, for screens with no room of their own for it:
/// Home's key image sync and Confirm's signing. Closes when the exchange
/// is over; its X cancels.
#[derive(Default)]
pub struct Host {
    inner: Option<Rc<RefCell<Inner>>>,
}

struct Inner {
    flow: Flow,
    card: Card,
    dialog: adw::Dialog,
}

impl Host {
    pub fn show(&mut self, parent: Option<&gtk::Widget>, request: TrezorRequest) {
        if matches!(request.ask, TrezorAsk::Over) {
            if let Some(inner) = self.inner.take() {
                let mut inner = inner.borrow_mut();
                inner.flow.take(request);
                inner.dialog.force_close();
            }
            return;
        }
        if self.inner.as_ref().is_some_and(|inner| !inner.borrow().dialog.is_mapped()) {
            // Closed by its X: the next exchange gets a dialog of its own.
            self.inner = None;
        }
        let inner = match &self.inner {
            Some(inner) => inner.clone(),
            None => {
                let inner = Rc::new_cyclic(|weak: &Weak<RefCell<Inner>>| {
                    let weak = weak.clone();
                    let card = Card::new(move |action| {
                        if let Some(inner) = weak.upgrade() {
                            let outcome = inner.borrow_mut().flow.act(action);
                            let inner = inner.borrow();
                            if outcome == Outcome::Cancelled {
                                inner.dialog.force_close();
                            } else {
                                inner.card.show(&inner.flow.view());
                            }
                        }
                    });
                    let bar = adw::HeaderBar::builder().title_widget(&adw::WindowTitle::new(words::YOUR_TREZOR, "")).build();
                    let content = adw::ToolbarView::new();
                    content.add_top_bar(&bar);
                    let clamp = adw::Clamp::builder().maximum_size(560).margin_start(16).margin_end(16).margin_bottom(16).build();
                    clamp.set_child(Some(card.widget()));
                    content.set_content(Some(&clamp));
                    let dialog = adw::Dialog::builder().title(words::YOUR_TREZOR).content_width(600).child(&content).build();
                    RefCell::new(Inner { flow: Flow::default(), card, dialog })
                });
                // Closing the dialog by its X, or Escape, cancels the exchange.
                {
                    let weak = Rc::downgrade(&inner);
                    inner.borrow().dialog.connect_closed(move |_| {
                        if let Some(inner) = weak.upgrade() {
                            let mut inner = inner.borrow_mut();
                            if inner.flow.live {
                                inner.flow.act(Action::Cancel);
                            }
                        }
                    });
                }
                inner.borrow().dialog.present(parent);
                self.inner = Some(inner.clone());
                inner
            }
        };
        let mut guard = inner.borrow_mut();
        guard.flow.take(request);
        let view = guard.flow.view();
        guard.card.show(&view);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(ask: TrezorAsk) -> (TrezorRequest, mpsc::Receiver<TrezorAnswer>) {
        let (reply, answers) = mpsc::channel();
        (TrezorRequest { ask, reply: Some(reply), cancel: Arc::new(|| {}) }, answers)
    }

    #[test]
    fn the_card_follows_the_device_and_counts_its_steps() {
        let mut flow = Flow::default();
        assert!(!flow.live);
        flow.take(request(TrezorAsk::Looking).0);
        assert!(flow.live);
        assert_eq!(flow.view().card.eyebrow, "LOOKING FOR YOUR TREZOR");
        flow.take(request(TrezorAsk::Hello { name: "Safe 3".into(), pin: true, passphrase: false }).0);
        flow.take(request(TrezorAsk::Button("pin_device".into())).0);
        let view = flow.view();
        assert_eq!(view.step.as_deref(), Some("1 of 3"), "no passphrase protection: three steps");
        assert_eq!(view.art, Art::Device(Screen::Pin));
        flow.take(request(TrezorAsk::Button("show_address".into())).0);
        assert_eq!(flow.view().step.as_deref(), Some("3 of 3"));
        flow.take(request(TrezorAsk::Button("key_image_sync".into())).0);
        assert_eq!(flow.view().step, None, "a sync isn't a step of connecting");
        flow.take(request(TrezorAsk::Over).0);
        assert!(!flow.live);
    }

    #[test]
    fn the_passphrase_question_answers_the_device() {
        let mut flow = Flow::default();
        flow.take(request(TrezorAsk::Looking).0);
        let (req, answers) = request(TrezorAsk::Passphrase { on_device: true });
        flow.take(req);
        assert!(matches!(flow.view().extra, Extra::PassphraseQuestion { on_device: true }));
        assert_eq!(flow.act(Action::TypeHere), Outcome::Nothing);
        assert!(matches!(flow.view().extra, Extra::PassphraseField));
        assert_eq!(flow.act(Action::BackToDevice), Outcome::Nothing);
        assert_eq!(flow.act(Action::NoPassphrase), Outcome::Nothing);
        assert!(matches!(answers.recv().unwrap(), TrezorAnswer::Passphrase(TrezorPassphrase::Typed { text }) if text.is_empty()), "No is the empty passphrase: the standard wallet");
        // The device is busy with it: nothing more to answer.
        assert!(matches!(flow.view().extra, Extra::None));
    }

    #[test]
    fn a_problem_offers_try_again_and_the_x_says_nothing_more() {
        let mut flow = Flow::default();
        flow.take(request(TrezorAsk::Looking).0);
        assert!(flow.fail("the Trezor stopped answering: cancelled", false));
        let view = flow.view();
        assert_eq!(view.tone, Tone::Problem);
        assert!(matches!(&view.extra, Extra::Actions(a) if a.len() == 1));
        assert_eq!(flow.act(Action::Retry), Outcome::Retry);
        assert!(!flow.live);

        flow.take(request(TrezorAsk::Looking).0);
        assert!(!flow.fail("the address the Trezor shows isn't the one it sent", false) || true);
        assert_eq!(flow.view().tone, Tone::Stop);
        assert!(matches!(&flow.view().extra, Extra::Actions(a) if a.is_empty()), "no Try again after a differing address");

        // The X during a question: the failure that follows isn't shown.
        let mut flow = Flow::default();
        flow.take(request(TrezorAsk::Looking).0);
        let (req, answers) = request(TrezorAsk::Pin);
        flow.take(req);
        assert_eq!(flow.act(Action::Cancel), Outcome::Cancelled);
        assert!(matches!(answers.recv().unwrap(), TrezorAnswer::Pin(None)));
        assert!(!flow.fail("cancelled on the Trezor", true));
        assert!(!flow.live);
    }
}
