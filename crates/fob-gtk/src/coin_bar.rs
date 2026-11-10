//! The coins bar and Home's card over it, as on Android: one segment per
//! coin, ready ones in the words' colour, those on their way in green,
//! each filling from faint to solid with its own confirmations. Above the
//! bar, what's ready on the left; on the right, in green, what's on its
//! way and when it'll all be ready.

use std::{cell::RefCell, rc::Rc};

use fob_app::views::{self, CoinSegment, NotReady};
use relm4::{adw::prelude::*, gtk};

use crate::icons;

/// The bar's segments as drawn: amount, and progress for one on its way.
type Segments = Rc<RefCell<Vec<(u64, Option<f32>)>>>;

/// How faint a segment on its way is before its confirmations fill it.
const FAINT: f64 = 0.28;
const HEIGHT: i32 = 6;

/// The coins bar, drawn across whatever width it's given.
#[derive(Clone)]
pub struct Bar {
    pub area: gtk::DrawingArea,
    segments: Segments,
}

impl Bar {
    pub fn new() -> Bar {
        let area = gtk::DrawingArea::builder().content_height(HEIGHT).hexpand(true).build();
        let segments: Segments = Rc::default();
        // The green comes from `.coin-incoming`; the ready segments take
        // the colour the words around them have.
        let green = gtk::Label::new(None);
        green.add_css_class("coin-incoming");
        let drawn = segments.clone();
        area.set_draw_func(move |area, cr, width, height| {
            let segments = drawn.borrow();
            let list: Vec<CoinSegment> = segments.iter().map(|&(atomic, progress)| CoinSegment { atomic, progress, coins: 1 }).collect();
            let widths = views::coin_bar_widths(list, width as f32);
            let ink = area.color();
            let green = green.color();
            let paint = |c: &gtk::gdk::RGBA, fade: f64| cr.set_source_rgba(f64::from(c.red()), f64::from(c.green()), f64::from(c.blue()), f64::from(c.alpha()) * fade);
            let (mut x, h) = (0.0_f64, f64::from(height));
            for (&(_, progress), w) in segments.iter().zip(widths) {
                let w = f64::from(w);
                match progress {
                    None => {
                        paint(&ink, 1.0);
                        cr.rectangle(x, 0.0, w, h);
                        let _ = cr.fill();
                    }
                    Some(p) => {
                        paint(&green, FAINT);
                        cr.rectangle(x, 0.0, w, h);
                        let _ = cr.fill();
                        paint(&green, 1.0);
                        cr.rectangle(x, 0.0, w * f64::from(p.clamp(0.0, 1.0)), h);
                        let _ = cr.fill();
                    }
                }
                x += w + f64::from(views::COIN_BAR_GAP);
            }
        });
        Bar { area, segments }
    }

    pub fn set(&self, segments: &[CoinSegment]) {
        *self.segments.borrow_mut() = segments.iter().map(|s| (s.atomic, s.progress)).collect();
        self.area.queue_draw();
    }
}

/// What the card's words can be, longest first.
#[derive(Default)]
struct Words {
    ready: Vec<String>,
    incoming: Vec<String>,
}

/// Home's card while some money isn't ready: pressing it opens Coins.
pub struct ReadyCard {
    pub root: gtk::Button,
    ready: gtk::Label,
    incoming: gtk::Label,
    time: gtk::Label,
    bar: Bar,
    words: Rc<RefCell<Words>>,
}

impl ReadyCard {
    pub fn new() -> ReadyCard {
        let ready = gtk::Label::builder().xalign(0.0).hexpand(true).css_classes(["heading"]).build();
        let incoming = gtk::Label::builder().css_classes(["heading"]).build();
        let time = gtk::Label::new(None);
        let down = icons::icon(icons::DOWN, 14, 1.0);
        let clock = icons::icon(icons::CLOCK, 16, 1.0);
        let coming = gtk::Box::builder().spacing(2).build();
        coming.append(&down);
        coming.append(&incoming);
        let by = gtk::Box::builder().spacing(4).build();
        by.append(&clock);
        by.append(&time);
        let fixed = gtk::Box::builder().spacing(10).css_classes(["coin-incoming"]).build();
        fixed.append(&coming);
        fixed.append(&by);
        let header = gtk::Box::builder().spacing(8).build();
        header.append(&ready);
        header.append(&fixed);
        let bar = Bar::new();
        let column = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(10).build();
        column.append(&header);
        column.append(&bar.area);
        let root = gtk::Button::builder().child(&column).css_classes(["card", "ready-card"]).tooltip_text("Coins").visible(false).build();
        let card = ReadyCard { root, ready, incoming, time, bar, words: Rc::default() };
        // The bar spans the card, so its width says how much room the words have.
        let (ready, incoming, words) = (card.ready.clone(), card.incoming.clone(), card.words.clone());
        card.bar.area.connect_resize(move |_, width, _| fit(&ready, &incoming, &words.borrow(), width));
        card
    }

    /// Shows `n`, in money while the balance shows money first; hides the
    /// card when all of it is ready.
    pub fn show(&self, n: Option<&NotReady>, money_first: bool) {
        let Some(n) = n else {
            self.root.set_visible(false);
            return;
        };
        let money = money_first && !n.ready_money.is_empty() && !n.incoming_money.is_empty();
        *self.words.borrow_mut() = if money {
            Words { ready: n.ready_money.clone(), incoming: n.incoming_money.clone() }
        } else {
            Words { ready: vec![n.ready_xmr.clone()], incoming: vec![n.incoming_xmr.clone()] }
        };
        self.ready.set_css_classes(if n.ready == 0 { &["heading", "dim-label"] } else { &["heading"] });
        self.time.set_label(&n.all_ready);
        self.bar.set(&n.segments);
        self.root.update_property(&[gtk::accessible::Property::Label(&n.description)]);
        self.root.set_visible(true);
        fit(&self.ready, &self.incoming, &self.words.borrow(), self.bar.area.width());
    }
}

/// Puts the longest words that fit `width` in the labels: all the cents,
/// then whole units, then short ("A$42.15M").
fn fit(ready: &gtk::Label, incoming: &gtk::Label, words: &Words, width: i32) {
    let (Some(first_ready), Some(first_incoming)) = (words.ready.first(), words.incoming.first()) else { return };
    let measure = |label: &gtk::Label, text: &str| label.create_pango_layout(Some(text)).pixel_size().0;
    // The arrow, clock, time and the gaps between them stay as they are.
    let fixed = 14 + 2 + 10 + 16 + 4 + measure(ready, "00:00") + 8 + 12;
    let levels = words.ready.len().max(words.incoming.len());
    let pick = |list: &[String], level: usize| list[level.min(list.len() - 1)].clone();
    let mut chosen = (first_ready.clone(), first_incoming.clone());
    for level in 0..levels {
        chosen = (pick(&words.ready, level), pick(&words.incoming, level));
        if width <= 0 || measure(ready, &chosen.0) + measure(incoming, &chosen.1) + fixed <= width {
            break;
        }
    }
    if ready.label() != chosen.0 {
        ready.set_label(&chosen.0);
    }
    if incoming.label() != chosen.1 {
        incoming.set_label(&chosen.1);
    }
}
