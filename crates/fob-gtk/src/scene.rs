//! A hidden debug path, as tern has: builds a scene with a wallet that doesn't
//! follow the chain (only `pay-code` asks a node, for a fee), saves a picture of
//! the window and quits. It's how the windows are checked on a headless display.
//!
//! `FOB_SCENE_DIR=/tmp/x FOB_SCENE=home FOB_SCENE_PNG=/tmp/x/home.png fob`

use std::{path::PathBuf, time::Duration};

use fob_app::Engine;
use relm4::{
    Sender,
    adw::prelude::*,
    gtk::{self, gdk, glib, gsk},
};

use crate::root::{Go, Msg};

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
        let code = play(&engine, &to, &scene).await;
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
        "receive" => Some(Go::Receive),
        "send" => Some(Go::Send),
        "contacts" => Some(Go::Contacts),
        "coins" => Some(Go::Coins),
        "settings" => Some(Go::Settings),
        "add-wallet" => Some(Go::AddWallet),
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
