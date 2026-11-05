//! A hidden debug path, as tern has: builds a scene with a wallet that doesn't
//! follow the chain (only `pay-code` asks a node, for a fee), saves a picture of
//! the window and quits. It's how the windows are checked on a headless display.
//!
//! `FOB_SCENE_DIR=/tmp/x FOB_SCENE=home FOB_SCENE_PNG=/tmp/x/home.png fob`
//!
//! The `walk` scene goes through the app as someone would: it finds what's on
//! screen by its words, fills in fields and presses buttons, and fails if what
//! should come next never shows.

use std::{path::PathBuf, time::Duration};

use fob_app::Engine;
use relm4::{
    Sender,
    adw::{self, prelude::*},
    gtk::{self, gdk, glib, gsk},
};

use crate::root::{Go, Msg};

/// Everything showing, depth first, dialogs included.
fn showing() -> Vec<gtk::Widget> {
    let mut found = vec![];
    fn walk(widget: &gtk::Widget, found: &mut Vec<gtk::Widget>) {
        if !widget.is_mapped() {
            return;
        }
        found.push(widget.clone());
        let mut child = widget.first_child();
        while let Some(next) = child {
            walk(&next, found);
            child = next.next_sibling();
        }
    }
    for window in gtk::Window::list_toplevels() {
        walk(&window, &mut found);
    }
    found
}

fn plain(text: &str) -> String {
    text.replace('_', "")
}

/// Waits for what's wanted to show: a screen takes its time when the wallet has work to do.
async fn find<T>(what: &str, pick: impl Fn(&gtk::Widget) -> Option<T>) -> Result<T, String> {
    for _ in 0..200 {
        if let Some(found) = showing().iter().rev().find_map(&pick) {
            return Ok(found);
        }
        glib::timeout_future(Duration::from_millis(50)).await;
    }
    Err(format!("never showed: {what}"))
}

/// Presses a button or a row by its words.
async fn press(words: &str) -> Result<(), String> {
    eprintln!("walk: press {words}");
    let target = find(words, |widget| {
        if let Some(button) = widget.downcast_ref::<gtk::Button>()
            && button.is_sensitive()
            && (button.label().is_some_and(|l| plain(&l) == words) || button.tooltip_text().is_some_and(|t| t == words) || (words == "Back" && button.action_name().is_some_and(|a| a == "navigation.pop")))
        {
            return Some(widget.clone());
        }
        widget.downcast_ref::<adw::PreferencesRow>().filter(|row| row.is_sensitive() && plain(&row.title()) == words && !row.is::<adw::EntryRow>()).map(|_| widget.clone())
    })
    .await?;
    match target.downcast_ref::<gtk::Button>() {
        Some(button) => button.emit_clicked(),
        None => {
            target.activate();
        }
    }
    settle_short().await;
    Ok(())
}

/// Types into a field by its title, or a passphrase field by its hint.
async fn fill(title: &str, text: &str) -> Result<(), String> {
    let field = find(title, |widget| {
        if let Some(row) = widget.downcast_ref::<adw::EntryRow>()
            && row.title() == title
        {
            return widget.clone().downcast::<gtk::Editable>().ok();
        }
        widget.downcast_ref::<gtk::PasswordEntry>().filter(|entry| entry.placeholder_text().is_some_and(|hint| hint == title)).and_then(|_| widget.clone().downcast::<gtk::Editable>().ok())
    })
    .await?;
    field.set_text(text);
    settle_short().await;
    Ok(())
}

/// Waits for words to show.
async fn see(words: &str) -> Result<(), String> {
    eprintln!("walk: see {words}");
    find(words, |widget| {
        let label = widget.downcast_ref::<gtk::Label>().is_some_and(|label| label.label().contains(words));
        let row = widget.downcast_ref::<adw::PreferencesRow>().is_some_and(|row| row.title().contains(words));
        (label || row).then_some(())
    })
    .await
}

async fn settle_short() {
    glib::timeout_future(Duration::from_millis(250)).await;
}

/// First run to a wallet, through every page a person would use with no money.
async fn walk(engine: &Engine, png: &str) -> Result<(), String> {
    let shot = |name: &str| snap(&format!("{png}.{name}.png"));

    // A passphrase is chosen, with a mistake first.
    see("Choose a passphrase").await?;
    fill("Passphrase", PASS).await?;
    fill("The passphrase again", "something else").await?;
    press("Continue").await?;
    see(&fob_app::words::errors::passphrases_differ()).await?;
    fill("The passphrase again", PASS).await?;
    press("Continue").await?;

    // A wallet.
    press("Make wallet").await?;
    see("Nothing yet").await?;
    shot("home");

    // Ask for money, kept as a request, seen, then deleted.
    press("Receive").await?;
    fill("Amount in XMR (optional)", "1.5").await?;
    fill("Note (optional)", "Lunch").await?;
    press("Keep as a request").await?;
    see("Show the code again").await?;
    see("Lunch").await?;
    shot("request");
    press("Show the code again").await?;
    press("Close").await?;
    press("Delete request").await?;
    press("Delete").await?;
    see("Ask for an amount").await?;
    press("Back").await?;

    // Someone to pay.
    press("Contacts").await?;
    see("No contacts").await?;
    press("Add a contact").await?;
    fill("Name", "Sam").await?;
    press("Save").await?;
    see("Sam").await?;
    shot("contacts");
    press("Back").await?;

    // A profile, which Meet needs, and Meet.
    press("Settings").await?;
    press("Your profile").await?;
    fill("Your name", "Henry").await?;
    let name = find("Your name", |widget| widget.downcast_ref::<adw::EntryRow>().filter(|row| row.title() == "Your name").cloned()).await?;
    name.emit_by_name::<()>("apply", &[]);
    settle_short().await;
    press("Meet someone").await?;
    see("Show them your code").await?;
    shot("meet");
    press("Back").await?;
    press("Back").await?;

    // Manual Sync on, asked first; Home offers Sync; the Scan page opens; and off again.
    press("Manual Sync").await?;
    press("Manual Sync").await?;
    press("Turn on").await?;
    see("Settled confirmations").await?;
    shot("manual");
    press("Back").await?;
    press("Rescan").await?;
    see("The whole wallet").await?;
    shot("scan");
    press("Back").await?;
    // Home's Sync needs a status, so `live-scan` presses that.
    press("Manual Sync").await?;
    press("Manual Sync").await?;
    settle().await;
    press("Back").await?;
    press("Back").await?;

    // Locked, a wrong passphrase refused, and back to the same wallet.
    press("Settings").await?;
    press("Lock now").await?;
    see("Unlock").await?;
    fill("Passphrase", PASS).await?;
    press("Unlock").await?;
    see("Nothing yet").await?;

    // Left alone with a dialog open: locking takes the dialog too, as it could be a seed phrase.
    press("Settings").await?;
    press("Change passphrase").await?;
    fill("Passphrase now", "typed").await?;
    engine.lock();
    see("Unlock").await?;
    // A dialog closing animates for a moment.
    settle().await;
    if showing().iter().any(|widget| widget.is::<adw::Dialog>()) {
        return Err("a dialog stayed up over the lock".into());
    }

    fill("Passphrase", "not it").await?;
    press("Unlock").await?;
    see("That isn't the passphrase").await?;
    fill("Passphrase", PASS).await?;
    press("Unlock").await?;
    see("Nothing yet").await?;
    Ok(())
}

const PASS: &str = "correct horse";

/// Where a scene keeps its wallets; `None` outside a scene.
pub fn dir() -> Option<PathBuf> {
    std::env::var_os("FOB_SCENE_DIR").map(PathBuf::from)
}

/// Starts the scene asked for, if one was.
pub fn run(engine: &Engine, to: Sender<Msg>) {
    let Some(scene) = std::env::var("FOB_SCENE").ok() else { return };
    let png = std::env::var("FOB_SCENE_PNG").unwrap_or_else(|_| "fob-scene.png".into());
    let engine = engine.clone();
    glib::spawn_future_local(async move {
        settle().await;
        let code = if scene == "walk" { walk(&engine, &png).await } else { play(&engine, &to, &scene).await };
        settle().await;
        snap(&png);
        if let Err(why) = code {
            eprintln!("fob: scene {scene} failed: {why}");
            std::process::exit(1);
        }
        std::process::exit(0);
    });
}

async fn settle() {
    glib::timeout_future(Duration::from_millis(900)).await;
}

/// Runs on the wallet's runtime, from the GTK thread.
async fn off<T: std::marker::Send + 'static>(fut: impl Future<Output = T> + std::marker::Send + 'static) -> T {
    relm4::spawn(fut).await.expect("the scene's task")
}

async fn play(engine: &Engine, to: &Sender<Msg>, scene: &str) -> Result<(), String> {
    if scene == "first-run" {
        return Ok(());
    }
    let e = engine.clone();
    let set_up = off(async move {
        if e.phase() == fob_app::Phase::NoPassphrase { e.set_up(PASS.into()).await } else { e.unlock(PASS.into()).await }
    })
    .await;
    set_up.map_err(|e| e.to_string())?;
    if scene == "start" {
        return Ok(());
    }
    if scene == "locked" {
        let e = engine.clone();
        off(async move { e.create_wallet("Spending".into()).await }).await.map_err(|e| e.to_string())?;
        engine.lock();
        return Ok(());
    }
    let e = engine.clone();
    if e.wallet().is_none() {
        off(async move { e.create_wallet("Spending".into()).await }).await.map_err(|e| e.to_string())?;
    }
    settle().await;
    let go = match scene {
        "home" => None,
        "scan" => Some(Go::Scan),
        "manual" => Some(Go::ManualSync),
        // With FOB_SCENE_ONLINE: follows the real chain until it's caught up.
        "live" => {
            see("Synced").await?;
            None
        }
        // With FOB_SCENE_ONLINE: Manual Sync on, then a real scan, started as someone would.
        "live-scan" => {
            see("Synced").await?;
            let e = engine.clone();
            off(async move { e.change(|wallet| async move { wallet.set_manual_sync(true).await }).await }).await.map_err(|e| e.to_string())?;
            press("Sync").await?;
            see("The whole wallet").await?;
            let wallet = engine.wallet().expect("just made");
            let clock = engine.clock();
            let drawer = off(async move { wallet.scan_drawer(clock.now, clock.utc_offset, clock.hours_24).await }).await;
            eprintln!("live-scan: {}", drawer.last_scan);
            // "From last scan" has nothing to do yet: today's blocks are a real scan.
            let (label, action) = (&drawer.options[2].label, &drawer.options[2].action);
            eprintln!("live-scan: {label}: {action}");
            press(label).await?;
            press(action).await?;
            see("You can leave this page").await?;
            // Done, the page closed, and Home offers Sync again.
            let mut left = 8;
            while let Err(why) = see("Sync").await {
                left -= 1;
                if left == 0 {
                    return Err(why);
                }
            }
            None
        }
        "receive" => Some(Go::Receive),
        "send" => Some(Go::Send),
        "contacts" => Some(Go::Contacts),
        "coins" => Some(Go::Coins),
        "settings" => Some(Go::Settings),
        "add-wallet" => Some(Go::AddWallet),
        "profile" => Some(Go::Profile),
        "node" => Some(Go::Node),
        "prices" => Some(Go::Prices),
        "split" => Some(Go::Split),
        "request" => {
            let e = engine.clone();
            let made = off(async move { e.change(|wallet| async move { wallet.make_request(None, Some("1.5".into()), false, Some("Lunch".into())).await }).await }).await.map_err(|e| e.to_string())?;
            Some(Go::Request(made.id))
        }
        // Confirm opens at once, while the fee is asked for.
        "pay-code" => Some(Go::SendFrom(format!("monero:{}?tx_amount=0.5&tx_description=Pizza", engine.wallet().expect("just made").primary_address()))),
        other => return Err(format!("no scene {other}")),
    };
    if let Some(go) = go {
        let _ = to.send(Msg::Go(go));
    }
    Ok(())
}

/// A picture of the window as it is now.
fn snap(path: &str) {
    let Some(window) = relm4::main_application().active_window().or_else(|| gtk::Window::list_toplevels().into_iter().find_map(|w| w.downcast::<gtk::Window>().ok())) else {
        eprintln!("fob: no window to picture");
        return;
    };
    let (width, height) = (window.width().max(1), window.height().max(1));
    let paintable = gtk::WidgetPaintable::new(Some(&window));
    let snapshot = gtk::Snapshot::new();
    paintable.snapshot(&snapshot, f64::from(width), f64::from(height));
    let Some(node) = snapshot.to_node() else {
        eprintln!("fob: nothing drawn");
        return;
    };
    let renderer = window.native().and_then(|n| n.renderer()).expect("a renderer");
    let bounds = gtk::graphene::Rect::new(0.0, 0.0, width as f32, height as f32);
    let texture: gdk::Texture = gsk::Renderer::render_texture(&renderer, &node, Some(&bounds));
    if let Err(e) = texture.save_to_png(path) {
        eprintln!("fob: saving {path}: {e}");
    }
}
