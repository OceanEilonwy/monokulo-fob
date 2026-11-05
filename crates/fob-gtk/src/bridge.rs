//! Where the engine's event loop meets Relm4's. The engine publishes from any
//! thread; this hands each update to the root component's input, on the GTK
//! thread, and the root gives each screen the latest view it draws.
//!
//! So a screen's model is the latest view, its input is the next one, and what
//! it does goes back to the engine. Screens hold no wallet state of their own.

use std::{fmt, sync::Arc};

use fob_app::{Listener, Update};
use relm4::Sender;

/// What the engine published, as a message Relm4 can carry.
pub struct Event(pub Update);

impl fmt::Debug for Event {
    /// By kind only: a view can hold addresses and amounts, which don't belong in a log.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match &self.0 {
            Update::Phase(_) => "Phase",
            Update::Screens(_) => "Screens",
            Update::Status(_) => "Status",
            Update::Wallets { .. } => "Wallets",
            Update::Toast(_) => "Toast",
        })
    }
}

/// The latest of a view, shared, as a screen's input. Relm4's inputs must be
/// `Debug`, and a view stays out of logs for the same reason as [`Event`].
pub struct View<T>(pub Arc<T>);

impl<T> Clone for View<T> {
    fn clone(&self) -> Self {
        View(self.0.clone())
    }
}

impl<T> fmt::Debug for View<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("View")
    }
}

/// A passphrase on its way through a message: Relm4's inputs must be `Debug`,
/// and this one never says what it holds.
pub struct Secret(pub String);

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret")
    }
}

/// The engine's listener: every update goes to `to`.
pub struct Bridge<M> {
    to: Sender<M>,
    wrap: fn(Event) -> M,
}

impl<M: Send + 'static> Bridge<M> {
    pub fn new(to: Sender<M>, wrap: fn(Event) -> M) -> Arc<Self> {
        Arc::new(Bridge { to, wrap })
    }
}

impl<M: Send + 'static> Listener for Bridge<M> {
    fn on_update(&self, update: Update) {
        // Only fails with the window gone, when nobody is left to tell.
        let _ = self.to.send((self.wrap)(Event(update)));
    }
}
