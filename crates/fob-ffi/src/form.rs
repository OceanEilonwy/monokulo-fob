//! The rules of Send's form, apart from how it's drawn.

use std::sync::Arc;

use crate::{AmountUnit, Network, PaymentRequest, Result, TypedAmount};
use fob_wallet::{uri, words::errors::{NotTaken, code_not_taken}};

/// A payment code as read, once: its address, amount, name and receipt
/// parsed and kept in Rust, so the form hands back this rather than the
/// text, and preparing a payment doesn't read the code (or decode its
/// receipt) again.
#[derive(Debug, PartialEq, uniffi::Object)]
pub struct ScannedCode(uri::PaymentRequest);

#[uniffi::export]
impl ScannedCode {
    /// Reads `text` as a payment code; throws when it isn't one.
    #[uniffi::constructor]
    pub fn new(network: Network, text: String) -> Result<Arc<ScannedCode>> {
        Ok(Arc::new(ScannedCode(uri::parse_payment(network.into(), &text)?)))
    }

    /// What the code asks, as [`parse_payment`](crate::parse_payment) says it.
    pub fn payment(&self) -> PaymentRequest {
        self.0.clone().into()
    }
}

impl ScannedCode {
    pub(crate) fn request(&self) -> &uri::PaymentRequest {
        &self.0
    }
}

/// What a pasted, opened or linked code does to the form.
#[derive(Clone, Debug, PartialEq, uniffi::Enum)]
pub enum Taken {
    /// A profile: show who it is first, to pay them or add them.
    Profile,
    /// An address, or a code with no amount: fills the form. The note follows
    /// the code: its description, or none.
    Fill { to: String, note: String, code: Arc<ScannedCode> },
    /// A code with its amount: filled too, and straight to Confirm.
    Review { to: String, amount: String, note: String, code: Arc<ScannedCode> },
    /// Not something to pay, and why.
    Refused(String),
}

/// What `text` does to the form.
pub fn take(network: Network, text: &str) -> Taken {
    read_code(network, text, true)
}

/// What `text` does to the form. `as_profile` is off when someone chose to pay
/// the profile they were shown, so it's taken as the payment code it also is.
/// The code is read as a payment once, and what else it might be is told
/// from that.
fn read_code(network: Network, text: &str, as_profile: bool) -> Taken {
    let payment = uri::parse_payment(network.into(), text);
    if as_profile && payment.as_ref().is_ok_and(|p| uri::profile_from(p.clone(), text).is_ok()) {
        return Taken::Profile;
    }
    if uri::parse_wallet_code(network.into(), text).is_ok() {
        return Taken::Refused(code_not_taken(NotTaken::WalletCode));
    }
    let payment = match payment {
        Ok(payment) => payment,
        Err(e) => return Taken::Refused(crate::FobError::from(e).to_string()),
    };
    let (to, note) = (payment.address.clone(), payment.description.clone().unwrap_or_default());
    match payment.amount {
        Some(amount) => Taken::Review { to, amount: crate::exact(amount), note, code: Arc::new(ScannedCode(payment)) },
        None => Taken::Fill { to, note, code: Arc::new(ScannedCode(payment)) },
    }
}

/// Whether a code taken while `to` is filled should ask first: a code is never
/// taken unasked once there's an address, in case it was one nearby meant for
/// someone else. `Some` with the other address when it's a different one.
pub fn asks_first(network: Network, to: &str, text: &str) -> Option<String> {
    let found = uri::parse_payment(network.into(), text).ok()?;
    (!to.trim().is_empty() && found.address != to.trim()).then_some(found.address)
}

/// Whether `to` is an address the form can pay as it is.
pub fn address_is_valid(network: Network, to: &str) -> bool {
    !to.trim().is_empty() && uri::parse_payment(network.into(), to).is_ok_and(|p| p.address == to.trim())
}

/// [`take`], for the other languages Fob's apps are written in.
#[uniffi::export]
pub fn take_code(network: Network, text: String, as_profile: bool) -> Taken {
    read_code(network, &text, as_profile)
}

/// [`asks_first`], for the other languages Fob's apps are written in.
#[uniffi::export]
pub fn code_asks_first(network: Network, to: String, text: String) -> Option<String> {
    asks_first(network, &to, &text)
}

/// [`address_is_valid`], for the other languages Fob's apps are written in.
#[uniffi::export]
pub fn to_is_valid(network: Network, to: String) -> bool {
    address_is_valid(network, &to)
}

/// The amount field while a price comes and goes. Money typed is never read as
/// XMR: when the price goes it's kept aside, and put back when one comes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Amount {
    pub text: String,
    pub unit: AmountUnit,
    held_money: Option<String>,
}

impl Amount {
    /// Starts in the unit the blank field opens in.
    pub fn new(unit: AmountUnit) -> Amount {
        Amount { unit, ..Amount::default() }
    }

    /// What's typed, in its unit, as Rust takes it.
    pub fn typed(&self) -> TypedAmount {
        TypedAmount::new(self.text.clone(), self.unit)
    }

    /// What a price being available now does to the field. Returns whether it
    /// changed, so the field's words want reading again.
    pub fn price_is(&mut self, can_switch: bool) -> bool {
        if can_switch
            && self.text.trim().is_empty()
            && self.unit == AmountUnit::Xmr
            && let Some(money) = self.held_money.take()
        {
            self.text = money;
            self.unit = AmountUnit::Fiat;
            return true;
        }
        if self.unit == AmountUnit::Xmr && !self.text.trim().is_empty() {
            self.held_money = None;
        }
        if !can_switch && self.unit == AmountUnit::Fiat {
            if !self.text.trim().is_empty() {
                self.held_money = Some(std::mem::take(&mut self.text));
            }
            self.unit = AmountUnit::Xmr;
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
        assert_eq!(take(Network::Stagenet, &to), Taken::Fill { to: to.clone(), note: String::new(), code: ScannedCode::new(Network::Stagenet, to.clone()).unwrap() });
        let code = format!("monero:{to}?tx_amount=0.5&tx_description=Pizza");
        assert_eq!(take(Network::Stagenet, &code), Taken::Review { to: to.clone(), amount: "0.5".into(), note: "Pizza".into(), code: ScannedCode::new(Network::Stagenet, code.clone()).unwrap() });
        let Taken::Review { code: scanned, .. } = take(Network::Stagenet, &code) else { panic!() };
        assert_eq!((scanned.payment().amount, scanned.payment().description.as_deref()), (Some(500_000_000_000), Some("Pizza")), "kept as read");
    }

    #[test]
    fn a_profile_is_shown_first_and_other_codes_say_why_not() {
        let to = address();
        assert_eq!(take(Network::Stagenet, &format!("monero:{to}?recipient_name=Sam")), Taken::Profile);
        assert_eq!(take(Network::Stagenet, "not a code"), Taken::Refused("that isn't a Monero address for stagenet".into()));
        let wallet = crate::classify_code(Network::Stagenet, format!("monero_wallet:{to}?view_key=00"));
        assert!(matches!(wallet, crate::CodeKind::Wallet));
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
        let mut a = Amount::new(AmountUnit::Fiat);
        a.text = "20".into();
        assert!(!a.price_is(true), "a price, nothing to do");
        assert!(a.price_is(false));
        assert_eq!(a.typed(), TypedAmount::Xmr { text: "".into() }, "not 20 XMR");
        assert!(a.is_filled());
        assert!(a.price_is(true));
        assert_eq!(a.typed(), TypedAmount::Fiat { text: "20".into() }, "back when a price comes");
    }

    #[test]
    fn typing_xmr_meanwhile_lets_the_money_go() {
        let mut a = Amount::new(AmountUnit::Fiat);
        a.text = "20".into();
        a.price_is(false);
        a.text = "0.5".into();
        assert!(!a.price_is(false));
        a.text.clear();
        assert!(!a.price_is(true), "what was typed since replaced it");
        assert_eq!((a.text.as_str(), a.unit), ("", AmountUnit::Xmr));
    }
}
