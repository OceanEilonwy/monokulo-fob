//! A Trezor's questions, as dialogs over whatever screen is talking to it:
//! a press to confirm on the device, its PIN (typed here by the positions
//! on the scrambled keypad the device shows), its passphrase (here, or on
//! the device), or how far a long exchange is. One dialog at a time; the
//! next question replaces it, and the end of the exchange closes it.

use std::{cell::RefCell, rc::Rc, sync::Arc};

use fob_app::{Prompter, TrezorAnswer, TrezorAsk, TrezorRequest, views::TrezorPassphrase};
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

/// What to say for the device's screens, by the names its firmware gives them.
fn button_words(what: &str) -> &'static str {
    let what = what.to_ascii_lowercase();
    if what.contains("pin") {
        "Unlock the Trezor: enter its PIN on the device."
    } else if what.contains("address") {
        "The Trezor shows this wallet's address. Fob shows the same one on Receive: if they ever differ, don't use this wallet."
    } else if what.contains("watch") || what.contains("export") {
        "On the Trezor, allow Fob to watch this wallet: it asks to export the view key."
    } else if what.contains("sync") || what.contains("key_image") {
        "On the Trezor, allow the key image sync: it shows which of this wallet's coins are spent."
    } else if what.contains("tx") || what.contains("send") || what.contains("output") || what.contains("fee") {
        "Check the payment on the Trezor and confirm it there."
    } else {
        "Confirm on the Trezor."
    }
}

/// The dialog on screen, while a device exchange is on.
#[derive(Default)]
pub struct Dialogs {
    current: Option<adw::AlertDialog>,
}

/// The response a dialog gives when Fob takes it down itself: the next
/// question replaces it, or the exchange is over. Nothing listens for it.
const TAKEN_DOWN: &str = "taken-down";

impl Dialogs {
    /// Closes the dialog on screen, if one is, without it counting as the
    /// person's answer.
    fn take_down(&mut self) {
        if let Some(old) = self.current.take() {
            // An alert dialog that closes answers with its close response,
            // "cancel" here: left so, the waiting dialog's cancel would run
            // and let go of the device between two of its questions (the
            // view key's export is confirmed, then the address shown).
            old.set_close_response(TAKEN_DOWN);
            old.force_close();
        }
    }

    fn replace(&mut self, parent: Option<&gtk::Widget>, dialog: adw::AlertDialog) {
        self.take_down();
        dialog.present(parent);
        self.current = Some(dialog);
    }

    pub fn show(&mut self, parent: Option<&gtk::Widget>, request: TrezorRequest) {
        let TrezorRequest { ask, reply, cancel } = request;
        match ask {
            TrezorAsk::Over => self.take_down(),
            TrezorAsk::Button(what) => self.replace(parent, waiting("Your Trezor", button_words(&what), cancel)),
            TrezorAsk::Progress { done, total } => {
                let body = format!("Working with the Trezor… {done} of {total}");
                match &self.current {
                    // The same dialog, told how far along.
                    Some(dialog) if dialog.heading().as_deref() == Some("Working") => dialog.set_body(&body),
                    _ => self.replace(parent, waiting("Working", &body, cancel)),
                }
            }
            TrezorAsk::Pin => {
                let typed: Rc<RefCell<String>> = Rc::new(RefCell::new(String::new()));
                let dots = gtk::Label::builder().label(" ").css_classes(["title-2"]).build();
                dots.update_property(&[gtk::accessible::Property::Label("Digits typed")]);
                let grid = gtk::Grid::builder().row_spacing(8).column_spacing(8).halign(gtk::Align::Center).build();
                // The device's keypad: 7 8 9 above, 1 2 3 below, digits hidden.
                for (row, positions) in [[7, 8, 9], [4, 5, 6], [1, 2, 3]].iter().enumerate() {
                    for (column, position) in positions.iter().enumerate() {
                        let button = gtk::Button::builder().label("•").width_request(72).height_request(56).build();
                        button.update_property(&[gtk::accessible::Property::Label(&format!("Keypad position {position}"))]);
                        let (typed, dots, position) = (typed.clone(), dots.clone(), *position);
                        button.connect_clicked(move |_| {
                            let mut t = typed.borrow_mut();
                            if t.len() < 50 {
                                t.push(char::from(b'0' + position));
                            }
                            dots.set_label(&"•".repeat(t.len()));
                        });
                        grid.attach(&button, column as i32, row as i32, 1, 1);
                    }
                }
                let delete = gtk::Button::builder().label("Delete").halign(gtk::Align::Center).build();
                {
                    let (typed, dots) = (typed.clone(), dots.clone());
                    delete.connect_clicked(move |_| {
                        let mut t = typed.borrow_mut();
                        t.pop();
                        let shown = "•".repeat(t.len());
                        dots.set_label(if t.is_empty() { " " } else { &shown });
                    });
                }
                let column = gtk::Box::new(gtk::Orientation::Vertical, 12);
                column.append(&dots);
                column.append(&grid);
                column.append(&delete);
                let dialog = adw::AlertDialog::builder()
                    .heading("Enter the PIN")
                    .body("The Trezor shows where each digit is. Press the same places here.")
                    .extra_child(&column)
                    .build();
                dialog.add_responses(&[("cancel", "_Cancel"), ("enter", "_Enter")]);
                dialog.set_default_response(Some("enter"));
                dialog.set_close_response("cancel");
                dialog.set_response_appearance("enter", adw::ResponseAppearance::Suggested);
                dialog.connect_response(None, move |_, response| {
                    let pin = std::mem::take(&mut *typed.borrow_mut());
                    let answer = (response == "enter" && !pin.is_empty()).then(|| fob_app::Zeroizing::new(pin));
                    if let Some(reply) = &reply {
                        let _ = reply.send(TrezorAnswer::Pin(answer));
                    }
                });
                self.replace(parent, dialog);
            }
            TrezorAsk::Passphrase { on_device } => {
                let entry = gtk::PasswordEntry::builder().show_peek_icon(true).activates_default(true).build();
                entry.update_property(&[gtk::accessible::Property::Label("Passphrase")]);
                let dialog = adw::AlertDialog::builder()
                    .heading("Passphrase")
                    .body("This Trezor has hidden wallets. Each passphrase is a wallet of its own; leave it empty for the standard one.")
                    .extra_child(&entry)
                    .build();
                dialog.add_responses(&[("cancel", "_Cancel"), ("use", "_Use")]);
                if on_device {
                    dialog.add_responses(&[("device", "On the _Trezor")]);
                }
                dialog.set_default_response(Some("use"));
                dialog.set_close_response("cancel");
                dialog.set_response_appearance("use", adw::ResponseAppearance::Suggested);
                dialog.connect_response(None, move |_, response| {
                    let typed = crate::widgets::secret_text(entry.text());
                    entry.set_text("");
                    let answer = match response {
                        "use" => TrezorPassphrase::Typed { text: String::clone(&typed) },
                        "device" => TrezorPassphrase::OnDevice,
                        _ => TrezorPassphrase::Cancelled,
                    };
                    if let Some(reply) = &reply {
                        let _ = reply.send(TrezorAnswer::Passphrase(answer));
                    }
                });
                self.replace(parent, dialog);
            }
        }
    }
}

/// A dialog that only waits, with a way to give up on the device.
fn waiting(heading: &str, body: &str, cancel: Arc<dyn Fn() + Send + Sync>) -> adw::AlertDialog {
    let spinner = adw::Spinner::builder().width_request(32).height_request(32).build();
    let dialog = adw::AlertDialog::builder().heading(heading).body(body).extra_child(&spinner).build();
    dialog.add_responses(&[("cancel", "_Cancel")]);
    dialog.set_close_response("cancel");
    dialog.connect_response(Some("cancel"), move |_, _| cancel());
    dialog
}

#[cfg(test)]
mod tests {
    use super::button_words;

    #[test]
    fn the_device_s_screens_are_said_in_words() {
        assert!(button_words("pin_device").starts_with("Unlock the Trezor"));
        assert!(button_words("confirm_watchkey").contains("allow Fob to watch"));
        assert!(button_words("show_address").contains("address"));
        assert!(button_words("confirm_tx").contains("payment"));
        assert_eq!(button_words("button 1"), "Confirm on the Trezor.");
    }
}
