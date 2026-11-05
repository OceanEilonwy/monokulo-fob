//! The rules of Send's form, apart from how it's drawn.

use fob_ffi::{CodeKind, Network, classify_code, is_profile_code, parse_payment};
use fob_wallet::words::errors::code_not_taken;

/// What a pasted, opened or linked code does to the form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Taken {
    /// A profile: show who it is first, to pay them or add them.
    Profile,
    /// An address, or a code with no amount: fills the form. The note follows
    /// the code: its description, or none.
    Fill { to: String, note: String, code: String },
    /// A code with its amount: filled too, and straight to Confirm.
    Review { to: String, amount: String, note: String, code: String },
    /// Not something to pay, and why.
    Refused(String),
}

/// What `text` does to the form.
pub fn take(network: Network, text: &str) -> Taken {
    if is_profile_code(network, text.to_owned()) {
        return Taken::Profile;
    }
    let refused = |kind| Taken::Refused(code_not_taken(kind));
    match classify_code(network, text.to_owned()) {
        CodeKind::Wallet => return refused(fob_wallet::words::errors::NotTaken::WalletCode),
        CodeKind::Unknown => {
            return match parse_payment(network, text.to_owned()) {
                Err(e) => Taken::Refused(e.to_string()),
                Ok(_) => refused(fob_wallet::words::errors::NotTaken::NotPayment),
            };
        }
        CodeKind::Payment => {}
    }
    let payment = match parse_payment(network, text.to_owned()) {
        Ok(payment) => payment,
        Err(e) => return Taken::Refused(e.to_string()),
    };
    let note = payment.description.unwrap_or_default();
    match payment.amount_text {
        Some(amount) => Taken::Review { to: payment.address, amount, note, code: text.to_owned() },
        None => Taken::Fill { to: payment.address, note, code: text.to_owned() },
    }
}

/// Whether a code taken while `to` is filled should ask first: a code is never
/// taken unasked once there's an address, in case it was one nearby meant for
/// someone else. `Some` with the other address when it's a different one.
pub fn asks_first(network: Network, to: &str, text: &str) -> Option<String> {
    let found = parse_payment(network, text.to_owned()).ok()?;
    (!to.trim().is_empty() && found.address != to.trim()).then_some(found.address)
}

/// Whether `to` is an address the form can pay as it is.
pub fn address_is_valid(network: Network, to: &str) -> bool {
    !to.trim().is_empty() && parse_payment(network, to.to_owned()).is_ok_and(|p| p.address == to.trim())
}

/// The amount field while a price comes and goes. Money typed is never read as
/// XMR: when the price goes it's kept aside, and put back when one comes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Amount {
    pub text: String,
    pub in_fiat: bool,
    held_money: Option<String>,
}

impl Amount {
    /// Starts in the unit the blank field opens in.
    pub fn new(in_fiat: bool) -> Amount {
        Amount { in_fiat, ..Amount::default() }
    }

    /// What a price being available now does to the field. Returns whether it
    /// changed, so the field's words want reading again.
    pub fn price_is(&mut self, can_switch: bool) -> bool {
        if can_switch
            && self.text.trim().is_empty()
            && !self.in_fiat
            && let Some(money) = self.held_money.take()
        {
            self.text = money;
            self.in_fiat = true;
            return true;
        }
        if !self.in_fiat && !self.text.trim().is_empty() {
            self.held_money = None;
        }
        if !can_switch && self.in_fiat {
            if !self.text.trim().is_empty() {
                self.held_money = Some(std::mem::take(&mut self.text));
            }
            self.in_fiat = false;
            return true;
        }
        false
    }

    /// Whether anything is typed, or kept aside.
    pub fn is_filled(&self) -> bool {
        !self.text.trim().is_empty() || self.held_money.is_some()
    }
}

#[cfg(test)]
mod tests {
    use fob_wallet::{keys::Keys, network::Network as CoreNetwork};

    use super::*;

    fn address() -> String {
        Keys::generate(CoreNetwork::Stagenet).keys.primary_address()
    }

    #[test]
    fn an_address_fills_the_form_and_a_code_with_an_amount_goes_to_confirm() {
        let to = address();
        assert_eq!(take(Network::Stagenet, &to), Taken::Fill { to: to.clone(), note: String::new(), code: to.clone() });
        let code = format!("monero:{to}?tx_amount=0.5&tx_description=Pizza");
        assert_eq!(take(Network::Stagenet, &code), Taken::Review { to: to.clone(), amount: "0.5".into(), note: "Pizza".into(), code: code.clone() });
    }

    #[test]
    fn a_profile_is_shown_first_and_other_codes_say_why_not() {
        let to = address();
        assert_eq!(take(Network::Stagenet, &format!("monero:{to}?recipient_name=Sam")), Taken::Profile);
        assert_eq!(take(Network::Stagenet, "not a code"), Taken::Refused("that isn't a Monero address for stagenet".into()));
        let wallet = fob_ffi::classify_code(Network::Stagenet, format!("monero_wallet:{to}?view_key=00"));
        assert!(matches!(wallet, CodeKind::Wallet));
        assert_eq!(take(Network::Stagenet, &format!("monero_wallet:{to}?view_key=00")), Taken::Refused("That's a wallet code, not a payment code.".into()));
    }

    #[test]
    fn a_code_for_another_address_asks_before_replacing_what_is_filled() {
        let (one, two) = (address(), address());
        assert_eq!(asks_first(Network::Stagenet, &one, &two), Some(two.clone()));
        assert_eq!(asks_first(Network::Stagenet, &one, &one), None, "the same address");
        assert_eq!(asks_first(Network::Stagenet, "", &two), None, "nothing filled");
        assert_eq!(asks_first(Network::Stagenet, &one, "not a code"), None);
    }

    #[test]
    fn only_a_whole_address_is_valid() {
        let to = address();
        assert!(address_is_valid(Network::Stagenet, &to));
        assert!(!address_is_valid(Network::Stagenet, ""));
        assert!(!address_is_valid(Network::Stagenet, &to[..20]));
    }

    #[test]
    fn money_typed_is_kept_when_the_price_goes_and_never_read_as_xmr() {
        let mut a = Amount::new(true);
        a.text = "20".into();
        assert!(!a.price_is(true), "a price, nothing to do");
        assert!(a.price_is(false));
        assert_eq!((a.text.as_str(), a.in_fiat), ("", false), "not 20 XMR");
        assert!(a.is_filled());
        assert!(a.price_is(true));
        assert_eq!((a.text.as_str(), a.in_fiat), ("20", true), "back when a price comes");
    }

    #[test]
    fn typing_xmr_meanwhile_lets_the_money_go() {
        let mut a = Amount::new(true);
        a.text = "20".into();
        a.price_is(false);
        a.text = "0.5".into();
        assert!(!a.price_is(false));
        a.text.clear();
        assert!(!a.price_is(true), "what was typed since replaced it");
        assert_eq!((a.text.as_str(), a.in_fiat), ("", false));
    }
}
