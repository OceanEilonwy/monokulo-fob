//! What several screens draw alike.

use std::rc::Rc;

use fob_app::views;
use relm4::{
    adw::{self, prelude::*},
    gtk::{self, cairo, gdk, gio},
};

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

/// Puts `text` on the clipboard.
pub fn copy(widget: &impl IsA<gtk::Widget>, text: &str) {
    widget.clipboard().set_text(text);
}

/// Copies a secret, and takes it off the clipboard again after a minute if
/// it's still there. Unlike a phone, nothing can stop other programs reading it meanwhile.
pub fn copy_secret(widget: &impl IsA<gtk::Widget>, text: &str) {
    let clipboard = widget.clipboard();
    clipboard.set_text(text);
    let (clipboard, kept) = (clipboard.clone(), text.to_owned());
    gtk::glib::timeout_add_local_once(std::time::Duration::from_secs(60), move || {
        gtk::glib::spawn_future_local(async move {
            if clipboard.read_text_future().await.ok().flatten().is_some_and(|now| now == kept) {
                clipboard.set_text("");
            }
        });
    });
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

fn paint_qr(cr: &cairo::Context, code: &views::QrCode, width: i32, height: i32) {
    let side = f64::from(width.min(height));
    cr.set_source_rgb(1.0, 1.0, 1.0);
    let _ = cr.paint();
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
    let view = views::address_pattern(address.to_owned());
    let area = gtk::DrawingArea::builder().content_width(64).content_height(64).halign(gtk::Align::Center).build();
    area.update_property(&[gtk::accessible::Property::Label("This address's pattern")]);
    area.set_draw_func(move |_, cr, width, _| {
        cr.set_source_rgb(1.0, 1.0, 1.0);
        let _ = cr.paint();
        let cell = f64::from(width) / 7.0;
        let channel = |shift: u32| f64::from((view.rgb >> shift) & 0xFF) / 255.0;
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
    let mut marked = String::new();
    for (i, group) in views::address_groups(text.to_owned()).iter().enumerate() {
        if i > 0 {
            marked.push(' ');
        }
        let escaped = gtk::glib::markup_escape_text(&group.text);
        if group.highlighted {
            marked.push_str(&format!("<span weight=\"bold\" foreground=\"#b34700\">{escaped}</span>"));
        } else {
            marked.push_str(&escaped);
        }
    }
    let label = gtk::Label::builder().label(&marked).use_markup(true).wrap(true).selectable(true).justify(gtk::Justification::Center).build();
    label.add_css_class("address");
    label
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
    let filter = gtk::FileFilter::new();
    filter.add_pixbuf_formats();
    filter.set_name(Some("Pictures"));
    let filters = gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&filter);
    let dialog = gtk::FileDialog::builder().title("Choose a picture of a code").filters(&filters).build();
    let window = parent.root().and_downcast::<gtk::Window>();
    let then = Rc::new(then);
    dialog.open(window.as_ref(), gio::Cancellable::NONE, move |file| {
        let Ok(file) = file else { return };
        let then = then.clone();
        gtk::glib::spawn_future_local(async move {
            let text = file.load_bytes_future().await.ok().and_then(|(bytes, _)| gdk::Texture::from_bytes(&bytes).ok()).and_then(|texture| read_texture(&texture));
            then(text);
        });
    });
}

fn read_texture(texture: &gdk::Texture) -> Option<String> {
    let (width, height) = (texture.width() as usize, texture.height() as usize);
    let mut rgba = vec![0u8; width * height * 4];
    texture.download(&mut rgba, width * 4);
    // Cairo's byte order on little-endian machines: blue, green, red, alpha.
    let grey: Vec<u8> = rgba.as_chunks::<4>().0.iter().map(|p| ((u32::from(p[2]) * 299 + u32::from(p[1]) * 587 + u32::from(p[0]) * 114) / 1000) as u8).collect();
    fob_app::picture::read_code(width, height, &grey)
}

/// Asks for the passphrase over `parent`, then answers: `Some` with what was
/// typed, or `None` if it was left.
pub fn ask_passphrase(parent: &gtk::Widget, heading: &str, body: &str, then: impl Fn(Option<String>) + 'static) {
    let entry = gtk::PasswordEntry::builder().show_peek_icon(true).activates_default(true).build();
    entry.update_property(&[gtk::accessible::Property::Label("Passphrase")]);
    let dialog = adw::AlertDialog::builder().heading(heading).body(body).extra_child(&entry).build();
    dialog.add_responses(&[("cancel", "_Cancel"), ("go", "_Continue")]);
    dialog.set_default_response(Some("go"));
    dialog.set_close_response("cancel");
    dialog.set_response_appearance("go", adw::ResponseAppearance::Suggested);
    dialog.connect_response(None, move |_, response| {
        let typed = entry.text().to_string();
        entry.set_text("");
        then((response == "go").then_some(typed));
    });
    dialog.present(Some(parent));
}
