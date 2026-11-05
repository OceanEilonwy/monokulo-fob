//! Fob for the Linux desktop. It draws what `fob-app` publishes and does what
//! only a desktop can do; everything about the wallet is in the crates below.

mod bridge;
mod coins;
mod confirm;
mod contacts;
mod home;
mod meet;
mod node;
mod passphrase;
mod payment;
mod prices;
mod profile;
mod receive;
mod requests;
mod root;
mod scene;
mod send;
mod settings;
mod split;
mod start;
mod widgets;

use std::path::PathBuf;

use fob_app::{Clock, Device, set_device};
use relm4::{
    RelmApp, adw, gtk,
    gtk::{gio, gio::prelude::*, glib},
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

fn main() {
    set_device(Device::Computer);
    gtk::init().expect("a display to draw on");
    adw::init().expect("libadwaita");
    // A scene must not hand off to a Fob that's already open.
    let unique = if scene::dir().is_some() { gio::ApplicationFlags::NON_UNIQUE } else { gio::ApplicationFlags::empty() };
    let application = adw::Application::builder().application_id(APP_ID).flags(gio::ApplicationFlags::HANDLES_OPEN | unique).build();
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
