//! Talking to a Trezor: the one card both apps draw while a device exchange
//! is on. The device drives; Fob mirrors what it shows and says what to do
//! there. Each state is an eyebrow, a line and the words under it, and the
//! steps of connecting a wallet are counted off at the card's top right.

/// The steps of connecting a Trezor wallet, in the order the device asks.
/// A device without a PIN skips Unlock; one without passphrase protection
/// skips Wallet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Unlock,
    Wallet,
    Allow,
    Address,
}

impl Step {
    pub fn name(self) -> &'static str {
        match self {
            Step::Unlock => "Unlock",
            Step::Wallet => "Wallet",
            Step::Allow => "Allow",
            Step::Address => "Address",
        }
    }
}

/// What the device's hello said it will ask for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Device {
    pub pin: bool,
    pub passphrase: bool,
}

impl Device {
    /// The steps this device takes a person through.
    pub fn steps(&self) -> Vec<Step> {
        let mut steps = vec![];
        if self.pin {
            steps.push(Step::Unlock);
        }
        if self.passphrase {
            steps.push(Step::Wallet);
        }
        steps.push(Step::Allow);
        steps.push(Step::Address);
        steps
    }

    /// Where `step` falls for this device: `(number from 1, of how many)`;
    /// `None` for a step the device skips.
    pub fn place(&self, step: Step) -> Option<(usize, usize)> {
        let steps = self.steps();
        steps.iter().position(|s| *s == step).map(|i| (i + 1, steps.len()))
    }

    /// "2 of 4", for the card's corner.
    pub fn place_text(&self, step: Step) -> Option<String> {
        self.place(step).map(|(n, of)| format!("{n} of {of}"))
    }
}

/// The step a device screen belongs to, by the name its firmware gives the
/// screen (`ButtonRequest.name`: `pin_device`, `get_watchkey`,
/// `show_address`). `None` for a screen outside connecting: a key image
/// sync, a payment.
pub fn step_for(what: &str) -> Option<Step> {
    let what = what.to_ascii_lowercase();
    if what.contains("pin") {
        Some(Step::Unlock)
    } else if what.contains("passphrase") {
        Some(Step::Wallet)
    } else if what.contains("watch") || what.contains("export") {
        Some(Step::Allow)
    } else if what.contains("address") {
        Some(Step::Address)
    } else {
        None
    }
}

/// The card: a small eyebrow, a line, and the words under it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Card {
    pub eyebrow: String,
    pub title: String,
    pub body: String,
}

fn card(eyebrow: &str, title: &str, body: &str) -> Card {
    Card { eyebrow: eyebrow.into(), title: title.into(), body: body.into() }
}

/// The screen's title on Android, and the dialog's on the desktop.
pub const YOUR_TREZOR: &str = "Your Trezor";
/// The eyebrow over every screen the device asks something on.
pub const LOOK: &str = "LOOK AT YOUR TREZOR";
/// By the spinner while no device has answered.
pub const LOOKING_HINT: &str = "Waiting for a device on USB…";
/// Buttons.
pub const TRY_AGAIN: &str = "Try again";
pub const OPEN_WALLET: &str = "Open wallet";
pub const NO_PASSPHRASE: &str = "No passphrase";
pub const ON_THE_TREZOR: &str = "Yes, enter it on the Trezor";
pub const TYPE_HERE: &str = "Yes, type it here";
pub const ENTER_ON_TREZOR_INSTEAD: &str = "Enter it on the Trezor instead";
pub const PASSPHRASE_FIELD: &str = "Passphrase";
pub const CLOSE: &str = "Close";
/// The keypad fallback's buttons.
pub const PIN_ENTER: &str = "Enter";
pub const PIN_DELETE: &str = "Delete";
/// Under the progress bar while key images come in.
pub const KEPT_IF_CANCELLED: &str = "What's fetched is kept if you cancel.";

/// No device has answered yet.
pub fn looking() -> Card {
    card("LOOKING FOR YOUR TREZOR", "Plug it in and unlock it.", "Trezor Model T, Safe 3 and Safe 5 hold Monero. Fob goes on by itself once the device answers.")
}

/// A screen the device shows, by the name its firmware gives it.
pub fn button(what: &str) -> Card {
    let w = what.to_ascii_lowercase();
    if w.contains("pin") {
        card(LOOK, "Unlock it with your PIN.", "Enter the PIN on the device itself. Fob never sees it. A wrong PIN is asked again there.")
    } else if w.contains("address") {
        card(LOOK, "Check the address against this one.", "The Trezor shows the same address in pages. If every group matches, confirm there. If anything differs, press no: the cable or this computer changed it.")
    } else if w.contains("watch") || w.contains("export") {
        card(LOOK, "Allow Fob to watch this wallet.", "The Trezor asks to export the view key. With it Fob sees payments coming in and going out; it can't spend. The spend key stays on the device, and every payment is confirmed there.")
    } else if w.contains("key_image") || w.contains("sync") || w.contains("refresh") {
        card(LOOK, "Allow the key image sync.", "The device marks which of this wallet's coins are already spent. Confirm once; the rest runs by itself.")
    } else if w.contains("tx") || w.contains("send") || w.contains("output") || w.contains("fee") || w.contains("confirm") || w.contains("final") {
        card(LOOK, "Check the payment and confirm it there.", "The Trezor shows where the money goes and the fee. Nothing is sent until you confirm on the device.")
    } else {
        card(LOOK, "Confirm on the Trezor.", "The device is showing something to confirm.")
    }
}

/// The device wants its passphrase, and Fob asks the question first: most
/// people have none.
pub fn passphrase() -> Card {
    card("PASSPHRASE", "Do you use a passphrase?", "Most people don't: say No and the standard wallet opens. A passphrase opens a hidden wallet of its own, typed on the Trezor or here.")
}

/// The field, once Yes, type it here was chosen.
pub fn passphrase_typed() -> Card {
    card("PASSPHRASE", "Type the passphrase.", "A typo opens an empty hidden wallet, so check it twice. Open wallet sends it to the Trezor; it isn't kept here.")
}

/// The keypad fallback, for a device that wants the PIN typed here.
pub fn pin() -> Card {
    card("PIN", "Enter the PIN.", "The Trezor shows where each digit is. Press the same places here.")
}

/// A long exchange, `done` of `total`.
pub fn progress(done: u32, total: u32) -> Card {
    card("WORKING WITH YOUR TREZOR", &format!("Fetching key images, {done} of {total}."), "The device marks which of this wallet's coins are already spent. You confirmed once; the rest runs by itself. Leave the Trezor plugged in.")
}

/// Something went wrong, and whether trying again makes sense.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    pub card: Card,
    /// Try again is offered; not after an address that didn't match.
    pub retry: bool,
    /// The address differed: red, the one state that says stop.
    pub stop: bool,
}

/// What a failed exchange says. `reason` is Fob's own wording of the error
/// (never the device's), `cancelled` whether the person said no.
pub fn problem(reason: &str, cancelled: bool) -> Problem {
    if cancelled {
        return Problem { card: card("NOTHING MADE", "You said no on the Trezor.", "That's fine: nothing was made and nothing left the device. Try again starts from the beginning."), retry: true, stop: false };
    }
    let r = reason.to_ascii_lowercase();
    if r.contains("isn't the one it sent") {
        return Problem {
            card: card("STOP", "The address the Trezor shows isn't the one it sent.", "Something between the device and Fob changed it: this cable, this computer, or software on it. Nothing was made. Don't use this computer for Monero until you know which, and try another cable and computer."),
            retry: false,
            stop: true,
        };
    }
    let cant_use = ["udev", "allowed", "another app", "can't hold monero", "firmware ", "isn't set up", "no way to talk"];
    if cant_use.iter().any(|w| r.contains(w)) {
        return Problem { card: card("FOB CAN'T USE THIS TREZOR", reason, "Put that right, unplug the device, plug it in again and try again."), retry: true, stop: false };
    }
    Problem {
        card: card("LOST THE TREZOR", "The Trezor stopped answering.", &format!("It was unplugged, another app took it, or it answered out of step with Fob. Unplug it, plug it in again, unlock it, and try again. Nothing was made.\n\n{reason}")),
        retry: true,
        stop: false,
    }
}

/// The wallet is here.
pub fn connected(name: &str) -> Card {
    card("CONNECTED", name, "Fob can see payments; the Trezor signs them.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_steps_follow_what_the_device_will_ask() {
        let full = Device { pin: true, passphrase: true };
        assert_eq!(full.steps(), vec![Step::Unlock, Step::Wallet, Step::Allow, Step::Address]);
        assert_eq!(full.place_text(Step::Wallet).as_deref(), Some("2 of 4"));
        let plain = Device { pin: false, passphrase: false };
        assert_eq!(plain.steps(), vec![Step::Allow, Step::Address]);
        assert_eq!(plain.place(Step::Unlock), None);
        assert_eq!(plain.place_text(Step::Address).as_deref(), Some("2 of 2"));
    }

    #[test]
    fn the_firmware_s_screen_names_map_to_steps_and_words() {
        assert_eq!(step_for("pin_device"), Some(Step::Unlock));
        assert_eq!(step_for("get_watchkey"), Some(Step::Allow));
        assert_eq!(step_for("show_address"), Some(Step::Address));
        assert_eq!(step_for("key_image_sync"), None);
        assert_eq!(step_for("confirm_output"), None);
        assert!(button("pin_device").title.contains("PIN"));
        assert!(button("get_watchkey").title.contains("watch"));
        assert!(button("show_address").title.contains("address"));
        assert!(button("key_image_sync").title.contains("key image"));
        assert!(button("confirm_final").title.contains("payment"));
        assert_eq!(button("button 1").title, "Confirm on the Trezor.");
    }

    #[test]
    fn problems_say_what_to_do_and_whether_to_try_again() {
        let no = problem("cancelled on the Trezor", true);
        assert!(no.retry && !no.stop && no.card.title.contains("said no"));
        let differs = problem("the address the Trezor shows isn't the one it sent: don't use this cable or computer", false);
        assert!(!differs.retry && differs.stop && differs.card.eyebrow == "STOP");
        let udev = problem("Fob isn't allowed to use the Trezor. On Linux, install Trezor's udev rules (Trezor Suite does), then plug it in again.", false);
        assert!(udev.retry && udev.card.eyebrow.contains("CAN'T USE") && udev.card.title.contains("udev"));
        let lost = problem("the Trezor answered out of step: it sent Features where MoneroWatchKey was expected.", false);
        assert!(lost.retry && lost.card.eyebrow.contains("LOST") && lost.card.body.contains("out of step"));
        assert!(passphrase().title.ends_with('?'));
        assert!(progress(20, 46).title.contains("20 of 46"));
    }
}
