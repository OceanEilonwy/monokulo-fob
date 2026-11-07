//! Fob for the Linux desktop. It draws what `fob-app` publishes and does what
//! only a desktop can do; everything about the wallet is in the crates below.

mod bridge;
mod coins;
mod confirm;
mod contacts;
mod home;
mod manual;
mod meet;
mod node;
mod passphrase;
mod payment;
mod prices;
mod profile;
mod receive;
mod requests;
mod root;
mod scan;
mod scene;
mod send;
mod settings;
mod split;
mod start;
mod trezor;
mod widgets;

use std::path::PathBuf;

use fob_app::{Clock, Device, set_device};
use relm4::{
    RelmApp, adw, gtk,
    gtk::{gdk, gdk::prelude::*, gio, glib, prelude::*},
};

pub const APP_ID: &str = "com.monokulo.Fob";

/// Where the desktop says to keep things, or where a scene says.
fn dirs() -> (PathBuf, PathBuf) {
    if let Some(dir) = scene::dir() {
        return (dir.join("data"), dir.join("cache"));
    }
    (glib::user_data_dir().join("fob"), glib::user_cache_dir().join("fob"))
}

/// The desktop's clock.
fn clock() -> impl Fn() -> Clock + Send + Sync + 'static {
    // Read once: settings are for the GTK thread, and the choice rarely changes.
    let hours_24 = gtk::gio::SettingsSchemaSource::default()
        .and_then(|source| source.lookup("org.gnome.desktop.interface", true))
        .map(|_| gtk::gio::Settings::new("org.gnome.desktop.interface"))
        .is_none_or(|settings| settings.string("clock-format") != "12h");
    move || {
        let now = glib::DateTime::now_local().expect("the clock");
        Clock { now: now.to_unix() as u64, utc_offset: now.utc_offset().as_seconds() as i32, hours_24 }
    }
}

/// Gives a window Fob's icon. The installed menu entry names it too, but a Fob
/// run without one, from a build or an AppImage, would otherwise show the
/// desktop's blank icon.
fn set_window_icon(window: &gtk::Window) {
    let set = |window: &gtk::Window| {
        let icons = [
            include_bytes!("../data/icons/fob-48.png").as_slice(),
            include_bytes!("../data/icons/fob-128.png").as_slice(),
            include_bytes!("../data/icons/fob-256.png").as_slice(),
        ]
        .map(|png| gdk::Texture::from_bytes(&glib::Bytes::from_static(png)).expect("Fob's icon is a PNG"));
        if let Some(toplevel) = window.surface().and_downcast::<gdk::Toplevel>() {
            toplevel.set_icon_list(&icons);
        }
    };
    if window.is_realized() {
        set(window);
    } else {
        window.connect_realize(set);
    }
}

/// Whether the user's settings ask for dark, read without the desktop portal.
fn wants_dark() -> bool {
    let gtk_setting = gtk::Settings::default().is_some_and(|s| s.is_gtk_application_prefer_dark_theme());
    let gnome = gio::SettingsSchemaSource::default()
        .and_then(|source| source.lookup("org.gnome.desktop.interface", true))
        .is_some_and(|schema| {
            schema.has_key("color-scheme") && gio::Settings::new("org.gnome.desktop.interface").string("color-scheme") == "prefer-dark"
        });
    gtk_setting || gnome
}

fn main() {
    // Before anything that could hold a key: a crash must not write the
    // unlocked wallet into a core file, and other programs of this user must
    // not read its memory. Then nothing Fob writes is readable by other
    // accounts, including the databases SQLCipher creates.
    fob_app::harden_process();
    fob_app::keep_files_private();
    set_device(Device::Computer);
    gtk::init().expect("a display to draw on");
    adw::init().expect("libadwaita");
    // libadwaita takes its scheme from the desktop portal. When the portal gives
    // no answer (blocked, or absent) the user's own word is all there is: the
    // GNOME color-scheme key, or the old gtk-application-prefer-dark-theme.
    if wants_dark() {
        adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceDark);
    }
    // A scene must not hand off to a Fob that's already open.
    let unique = if scene::dir().is_some() { gio::ApplicationFlags::NON_UNIQUE } else { gio::ApplicationFlags::empty() };
    let application = adw::Application::builder().application_id(APP_ID).flags(gio::ApplicationFlags::HANDLES_OPEN | unique).build();
    application.connect_window_added(|_, window| set_window_icon(window));
    application.connect_open(|app, files, _| {
        if let Some(file) = files.first() {
            root::link_opened(file.uri().to_string());
        }
        app.activate();
    });
    let links = std::env::args().skip(1).filter(|arg| arg.to_ascii_lowercase().starts_with("monero:"));
    let app = RelmApp::from_app(application).with_args(std::iter::once("fob".to_owned()).chain(links).collect());
    relm4::set_global_css(include_str!("style.css"));
    let (data, cache) = dirs();
    app.run::<root::Root>(root::Init { data, cache, online: scene::dir().is_none() || std::env::var_os("FOB_SCENE_ONLINE").is_some(), clock: Box::new(clock()) });
}
