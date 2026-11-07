//! What several screens draw alike.

use std::rc::Rc;

use fob_app::views;
use relm4::{
    adw::{self, prelude::*},
    gtk::{self, cairo, gdk, gio},
};

use crate::root::Go;

/// A boxed list, empty.
pub fn list() -> gtk::ListBox {
    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::None);
    list.add_css_class("boxed-list");
    list
}

/// Takes every row out of a list.
pub fn clear(list: &gtk::ListBox) {
    list.remove_all();
}

/// Takes every child out of a box.
pub fn empty(parent: &gtk::Box) {
    while let Some(child) = parent.first_child() {
        parent.remove(&child);
    }
}

/// A row with a title and a line under it; plain text, never markup.
pub fn row(title: &str, line: &str) -> adw::ActionRow {
    let row = adw::ActionRow::builder().use_markup(false).title(title).build();
    if !line.is_empty() {
        row.set_subtitle(line);
    }
    row
}

/// A label that wraps.
pub fn words(text: &str) -> gtk::Label {
    gtk::Label::builder().label(text).wrap(true).xalign(0.0).build()
}

/// Dim words that wrap.
pub fn dim(text: &str) -> gtk::Label {
    let label = words(text);
    label.add_css_class("dim-label");
    label
}

/// Dim words, centred.
pub fn centred(text: &str) -> gtk::Label {
    let label = dim(text);
    label.set_xalign(0.5);
    label.set_justify(gtk::Justification::Center);
    label
}

/// A heading, centred.
pub fn heading(text: &str) -> gtk::Label {
    let label = words(text);
    label.add_css_class("title-3");
    label.set_xalign(0.5);
    label.set_justify(gtk::Justification::Center);
    label
}

/// Someone's face, or their initial without a photo.
pub fn avatar(size: i32, name: &str, photo: Option<&[u8]>) -> adw::Avatar {
    let avatar = adw::Avatar::new(size, Some(name), true);
    if let Some((width, height, pixels)) = photo.and_then(fob_app::picture::photo_pixels) {
        let bytes = gtk::glib::Bytes::from_owned(pixels);
        let texture = gdk::MemoryTexture::new(width as i32, height as i32, gdk::MemoryFormat::R8g8b8a8, &bytes, width as usize * 4);
        avatar.set_custom_image(Some(&texture));
    }
    avatar
}

/// A row of Activity, opening what it's about.
pub fn activity_row(view: &views::ActivityRow, go: impl Fn(Go) + 'static) -> adw::ActionRow {
    let row = self::row(&view.title, &view.line);
    let amount = gtk::Box::builder().orientation(gtk::Orientation::Vertical).valign(gtk::Align::Center).build();
    let text = gtk::Label::builder().label(&view.amount_text).xalign(1.0).build();
    text.add_css_class("money");
    if view.activity.as_ref().is_some_and(|a| !a.outgoing) {
        text.add_css_class("incoming");
    }
    amount.append(&text);
    if let Some(fiat) = &view.fiat_text {
        let under = gtk::Label::builder().label(fiat).xalign(1.0).build();
        under.add_css_class("dim-label");
        under.add_css_class("caption");
        amount.append(&under);
    }
    row.add_suffix(&amount);
    let target = if let Some(activity) = &view.activity {
        Some(Go::Payment(activity.tx.clone()))
    } else if let Some(id) = view.request {
        Some(Go::Request(id))
    } else {
        view.share.map(Go::Share)
    };
    if let Some(target) = target {
        row.set_activatable(true);
        let target = std::cell::RefCell::new(Some(target));
        row.connect_activated(move |_| {
            let taken = target.borrow_mut().take();
            if let Some(next) = taken {
                let again = next.again();
                go(next);
                *target.borrow_mut() = Some(again);
            }
        });
    }
    row
}

/// A receipt's lines, and its total.
pub fn receipt(receipt: &views::Receipt) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::builder().title(gtk::glib::markup_escape_text(&receipt.merchant).as_str()).description(gtk::glib::markup_escape_text(&receipt.issued).as_str()).build();
    let line = |title: &str, amount: &str| {
        let row = self::row(title, "");
        let text = gtk::Label::new(Some(amount));
        text.add_css_class("money");
        row.add_suffix(&text);
        group.add(&row);
    };
    for item in &receipt.lines {
        line(&item.label, &item.amount);
    }
    if let Some(tax) = &receipt.tax {
        line("Tax", tax);
    }
    line("Total", &receipt.total);
    group
}

/// A code in a dialog, over `parent`.
pub fn show_code(parent: &gtk::Widget, heading: &str, body: &str, code: &str) {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 12);
    if let Some(qr) = qr(code, 300) {
        column.append(&qr);
    }
    let dialog = adw::AlertDialog::builder().heading(heading).body(body).extra_child(&column).build();
    dialog.add_responses(&[("close", "_Close"), ("copy", "_Copy")]);
    dialog.set_close_response("close");
    let (kept, from) = (code.to_owned(), parent.clone());
    dialog.connect_response(Some("copy"), move |_, _| copy(&from, &kept));
    dialog.present(Some(parent));
}

/// Asks for a picture and makes a photo Rust keeps from it.
pub fn choose_photo_then(parent: &gtk::Widget, then: impl Fn(Result<Vec<u8>, String>) + 'static) {
    let then = Rc::new(then);
    let dialog = gtk::FileDialog::builder().title("Choose a photo").filters(&picture_filters()).build();
    let window = parent.root().and_downcast::<gtk::Window>();
    dialog.open(window.as_ref(), gio::Cancellable::NONE, move |file| {
        let Ok(file) = file else { return };
        let then = then.clone();
        gtk::glib::spawn_future_local(async move {
            let made = file.load_bytes_future().await.ok().and_then(|(bytes, _)| texture(&bytes, fob_app::picture::OWN_PHOTO)).and_then(|texture| {
                let (width, height) = (texture.width() as u32, texture.height() as u32);
                fob_app::picture::small_photo(width, height, &texture_rgba(&texture))
            });
            then(made.ok_or_else(fob_app::words::errors::photo_too_big));
        });
    });
}

fn picture_filters() -> gio::ListStore {
    let filter = gtk::FileFilter::new();
    filter.add_pixbuf_formats();
    filter.set_name(Some("Pictures"));
    let filters = gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&filter);
    filters
}

/// A picture decoded by GDK, if its header says it's within `limits`. The
/// decoders are the system's, in C, and Fob doesn't replace them; it only keeps
/// what reaches them small and in a few formats (see `fob_app::picture`).
fn texture(bytes: &gtk::glib::Bytes, limits: fob_app::picture::Limits) -> Option<gdk::Texture> {
    if !fob_app::picture::picture_fits(bytes, limits) {
        return None;
    }
    gdk::Texture::from_bytes(bytes).ok()
}

/// A texture's pixels as RGBA. GDK downloads them in Cairo's order.
fn texture_rgba(texture: &gdk::Texture) -> Vec<u8> {
    let (width, height) = (texture.width() as usize, texture.height() as usize);
    let mut pixels = vec![0u8; width * height * 4];
    texture.download(&mut pixels, width * 4);
    for pixel in pixels.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    pixels
}

/// Puts `text` on the clipboard.
pub fn copy(widget: &impl IsA<gtk::Widget>, text: &str) {
    widget.clipboard().set_text(text);
}

/// Copies a secret, and takes it off the clipboard again after a minute if
/// it's still there. Unlike a phone, nothing can stop other programs reading it meanwhile.
///
/// It's offered with `x-kde-passwordManagerHint: secret` beside the text, which
/// clipboard managers that keep a history (Klipper, cliphist through
/// wl-clipboard, CopyQ) take as "don't keep this". A manager that ignores the
/// hint still records it, and nothing on a desktop can prevent that.
pub fn copy_secret(widget: &impl IsA<gtk::Widget>, text: &str) {
    let clipboard = widget.clipboard();
    let bytes = gtk::glib::Bytes::from(text.as_bytes());
    let provider = gdk::ContentProvider::new_union(&[
        gdk::ContentProvider::for_bytes("text/plain;charset=utf-8", &bytes),
        gdk::ContentProvider::for_bytes("text/plain", &bytes),
        gdk::ContentProvider::for_bytes("UTF8_STRING", &bytes),
        gdk::ContentProvider::for_bytes("x-kde-passwordManagerHint", &gtk::glib::Bytes::from_static(b"secret")),
    ]);
    if clipboard.set_content(Some(&provider)).is_err() {
        return;
    }
    // Still ours a minute later: it goes. Compared by provider, so the secret
    // isn't read back, or kept anywhere but on the clipboard meanwhile.
    gtk::glib::timeout_add_local_once(std::time::Duration::from_secs(60), move || {
        if clipboard.content().is_some_and(|now| now == provider) {
            let _ = clipboard.set_content(None::<&gdk::ContentProvider>);
        }
    });
}

/// An entry's text, wrapped to be wiped once Fob is done with it. GTK's own
/// buffer, and anything an input method kept, are out of Fob's reach: clearing
/// the entry right after reading it is all that helps there.
pub fn secret_text(text: gtk::glib::GString) -> fob_app::Zeroizing<String> {
    fob_app::Zeroizing::new(text.as_str().to_owned())
}

/// A code drawn dark on white in both themes, so every reader reads it.
pub fn qr(text: &str, side: i32) -> Option<gtk::DrawingArea> {
    let code = views::qr_code(text.to_owned()).ok()?;
    let area = gtk::DrawingArea::builder().content_width(side).content_height(side).halign(gtk::Align::Center).build();
    area.set_accessible_role(gtk::AccessibleRole::Img);
    area.update_property(&[gtk::accessible::Property::Label("QR code")]);
    area.set_draw_func(move |_, cr, width, height| paint_qr(cr, &code, width, height));
    Some(area)
}

/// Fills a rounded rectangle over the whole of `width` by `height` with `(r, g, b)`,
/// and leaves everything drawn after it inside it.
fn rounded_clip(cr: &cairo::Context, width: i32, height: i32, radius: f64, (red, green, blue): (f64, f64, f64)) {
    use std::f64::consts::{FRAC_PI_2, PI};
    let (w, h, r) = (f64::from(width), f64::from(height), radius);
    cr.new_sub_path();
    cr.arc(w - r, r, r, -FRAC_PI_2, 0.0);
    cr.arc(w - r, h - r, r, 0.0, FRAC_PI_2);
    cr.arc(r, h - r, r, FRAC_PI_2, PI);
    cr.arc(r, r, r, PI, 3.0 * FRAC_PI_2);
    cr.close_path();
    cr.set_source_rgb(red, green, blue);
    let _ = cr.fill_preserve();
    cr.clip();
}

fn paint_qr(cr: &cairo::Context, code: &views::QrCode, width: i32, height: i32) {
    let side = f64::from(width.min(height));
    // Still dark on white in both themes, so every reader reads it; only its corners are round.
    rounded_clip(cr, width, height, side * 0.06, (1.0, 1.0, 1.0));
    let size = code.size as usize;
    // Four modules of quiet around it, as readers want.
    let module = side / (size + 8) as f64;
    cr.set_source_rgb(0.102, 0.098, 0.09);
    for y in 0..size {
        for x in 0..size {
            if code.dark[y * size + x] {
                cr.rectangle((x + 4) as f64 * module, (y + 4) as f64 * module, module + 0.5, module + 0.5);
            }
        }
    }
    let _ = cr.fill();
}

/// An address's pattern, on its white badge, in Rust's colour.
pub fn pattern(address: &str) -> gtk::DrawingArea {
    pattern_sized(address, 64)
}

/// The same, `side` px across with rounded corners, to sit in a row.
pub fn pattern_sized(address: &str, side: i32) -> gtk::DrawingArea {
    let view = views::address_pattern(address.to_owned());
    let area = gtk::DrawingArea::builder().content_width(side).content_height(side).halign(gtk::Align::Center).valign(gtk::Align::Center).build();
    area.update_property(&[gtk::accessible::Property::Label("This address's pattern")]);
    area.set_draw_func(move |_, cr, width, height| {
        // Dark behind the dots in the dark theme, as on Android, their colour lightened to read on it.
        let dark = adw::StyleManager::default().is_dark();
        rounded_clip(cr, width, height, f64::from(width.min(height)) * 0.2, if dark { (0.165, 0.157, 0.141) } else { (1.0, 1.0, 1.0) });
        let cell = f64::from(width) / 7.0;
        let channel = |shift: u32| {
            let value = f64::from((view.rgb >> shift) & 0xFF) / 255.0;
            if dark { value + (1.0 - value) * 0.45 } else { value }
        };
        cr.set_source_rgb(channel(16), channel(8), channel(0));
        for row in 0..5 {
            for column in 0..5 {
                if view.dots[row * 5 + column] {
                    cr.arc((column + 1) as f64 * cell + cell / 2.0, (row + 1) as f64 * cell + cell / 2.0, cell * 0.38, 0.0, std::f64::consts::TAU);
                    let _ = cr.fill();
                }
            }
        }
    });
    area
}

/// An address in groups of five, those Rust marks stood out to compare by eye.
pub fn address(text: &str) -> gtk::Label {
    let marked = address_markup(text);
    let label = gtk::Label::builder().label(&marked).use_markup(true).wrap(true).selectable(true).justify(gtk::Justification::Center).build();
    label.add_css_class("address");
    label
}

/// Who a payment goes to, as a row of its own: the pattern, then the address.
pub fn recipient(text: &str) -> gtk::ListBox {
    let list = list();
    let row = adw::ActionRow::builder().title("To").use_markup(true).subtitle(address_markup(text)).subtitle_selectable(true).build();
    row.add_css_class("address-row");
    row.add_prefix(&pattern_sized(text, 44));
    list.append(&row);
    list
}

/// A button's words with Android's mark before them (`after`: behind them).
pub fn icon_label(path: &'static str, text: &str, after: bool) -> gtk::Box {
    let row = gtk::Box::builder().spacing(8).halign(gtk::Align::Center).build();
    let mark = crate::icons::icon(path, 18, 1.0);
    let label = gtk::Label::new(Some(text));
    if after {
        row.append(&label);
        row.append(&mark);
    } else {
        row.append(&mark);
        row.append(&label);
    }
    row
}

/// The words on a button: its label, or the label inside what it holds.
pub fn button_words(button: &gtk::Button) -> Option<String> {
    if let Some(label) = button.label() {
        return Some(label.to_string());
    }
    fn first_label(widget: &gtk::Widget) -> Option<String> {
        if let Some(label) = widget.downcast_ref::<gtk::Label>() {
            return Some(label.label().to_string());
        }
        let mut child = widget.first_child();
        while let Some(c) = child {
            if let Some(found) = first_label(&c) {
                return Some(found);
            }
            child = c.next_sibling();
        }
        None
    }
    button.child().and_then(|child| first_label(&child))
}

/// Takes the pencil out of an entry row. It says the row can be typed in, which a field
/// with its own button beside it doesn't need said.
pub fn hide_pencil(row: &adw::EntryRow) {
    fn walk(widget: &gtk::Widget) {
        if let Some(image) = widget.downcast_ref::<gtk::Image>() {
            if image.icon_name().as_deref() == Some("document-edit-symbolic") {
                image.set_icon_name(None);
                image.set_visible(false);
            }
        }
        let mut child = widget.first_child();
        while let Some(c) = child {
            walk(&c);
            child = c.next_sibling();
        }
    }
    walk(row.upcast_ref());
}

/// An address on one line of its own: its pattern, the address, and a button to copy.
pub fn address_row(text: &str, on_copy: impl Fn() + 'static) -> gtk::ListBox {
    let list = list();
    let row = adw::ActionRow::builder().title(address_markup(text)).use_markup(true).title_lines(0).title_selectable(true).build();
    row.add_css_class("address-line");
    row.add_prefix(&pattern_sized(text, 44));
    let copy = gtk::Button::builder().icon_name("edit-copy-symbolic").valign(gtk::Align::Center).tooltip_text("Copy").css_classes(["flat"]).build();
    copy.connect_clicked(move |_| on_copy());
    row.add_suffix(&copy);
    list.append(&row);
    list
}

/// The address's groups as Pango markup, those Rust marks stood out.
fn address_markup(text: &str) -> String {
    let mut marked = String::new();
    for (i, group) in views::address_groups(text.to_owned()).iter().enumerate() {
        if i > 0 {
            marked.push(' ');
        }
        let escaped = gtk::glib::markup_escape_text(&group.text);
        if group.highlighted {
            // A tint behind them, not a colour on them: orange text can't be read on a dark page.
            marked.push_str(&format!("<span weight=\"bold\" background=\"#ff6600\" bgalpha=\"35%\">{escaped}</span>"));
        } else {
            marked.push_str(&escaped);
        }
    }
    marked
}

/// The clipboard's text, for what `then` does with it; `None` when there's none.
pub fn paste(widget: &impl IsA<gtk::Widget>, then: impl FnOnce(Option<String>) + 'static) {
    let clipboard = widget.clipboard();
    gtk::glib::spawn_future_local(async move {
        then(clipboard.read_text_future().await.ok().flatten().map(|t| t.to_string()));
    });
}

/// Asks for a picture and reads a code from it: a desktop's way of scanning.
pub fn read_code_from_picture(parent: &gtk::Widget, then: impl Fn(Option<String>) + 'static) {
    let dialog = gtk::FileDialog::builder().title("Choose a picture of a code").filters(&picture_filters()).build();
    let window = parent.root().and_downcast::<gtk::Window>();
    let then = Rc::new(then);
    dialog.open(window.as_ref(), gio::Cancellable::NONE, move |file| {
        let Ok(file) = file else { return };
        let then = then.clone();
        gtk::glib::spawn_future_local(async move {
            // Someone else's picture, in the process holding the keys: only a
            // small one in a known format reaches GDK's decoders.
            let text = file.load_bytes_future().await.ok().and_then(|(bytes, _)| texture(&bytes, fob_app::picture::CODE_PICTURE)).and_then(|texture| read_texture(&texture));
            then(text);
        });
    });
}

fn read_texture(texture: &gdk::Texture) -> Option<String> {
    let (width, height) = (texture.width() as usize, texture.height() as usize);
    let grey: Vec<u8> = texture_rgba(texture).as_chunks::<4>().0.iter().map(|p| ((u32::from(p[0]) * 299 + u32::from(p[1]) * 587 + u32::from(p[2]) * 114) / 1000) as u8).collect();
    fob_app::picture::read_code(width, height, &grey)
}

/// Asks for the passphrase over `parent`, then answers: `Some` with what was
/// typed, or `None` if it was left.
pub fn ask_passphrase(parent: &gtk::Widget, heading: &str, body: &str, then: impl Fn(Option<fob_app::Zeroizing<String>>) + 'static) {
    let entry = gtk::PasswordEntry::builder().show_peek_icon(true).activates_default(true).build();
    entry.update_property(&[gtk::accessible::Property::Label("Passphrase")]);
    let dialog = adw::AlertDialog::builder().heading(heading).body(body).extra_child(&entry).build();
    dialog.add_responses(&[("cancel", "_Cancel"), ("go", "_Continue")]);
    dialog.set_default_response(Some("go"));
    dialog.set_close_response("cancel");
    dialog.set_response_appearance("go", adw::ResponseAppearance::Suggested);
    dialog.connect_response(None, move |_, response| {
        let typed = secret_text(entry.text());
        entry.set_text("");
        then((response == "go").then_some(typed));
    });
    dialog.present(Some(parent));
}
