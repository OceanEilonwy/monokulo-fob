//! Errors, in words an app can show.

use fob_chain::error::EndpointProblem;
use fob_codes::{
    error::{CodeError, ProfileCodeProblem},
    pay, swap,
};
use fob_monero::{amount, error::MoneroError};
use fob_price::error::PriceError;
use fob_screens::error::ScreenError;
use fob_vault::error::VaultError;

use crate::{Device, device};

pub fn monero(e: MoneroError) -> String {
    match e {
        MoneroError::BadAmount => "that amount isn't valid".into(),
        MoneroError::BadAddress(network) => format!("that isn't a Monero address for {}", network.name()),
        MoneroError::WrongNetwork { expected, found } => format!("the wallet code is for {}, but this wallet is on {}", found.name(), expected.name()),
    }
}

pub fn price(e: &PriceError) -> String {
    match e {
        PriceError::NoPrices { code } => format!("Fob has no prices in {code}"),
        PriceError::NotPriced { provider, currency } => format!("{provider} doesn't price {currency}"),
        PriceError::TorNotYet => "prices over Tor aren't available yet".into(),
        PriceError::NotOverTransport { provider, transport } => format!("{provider} doesn't answer {}", crate::prices::over(*transport)),
        PriceError::NoRoute { currency } => format!("Kraken has no XMR price in {currency}"),
        PriceError::BadUrl { url } => format!("{url} isn't an address"),
        PriceError::HttpsOnly => "prices are only asked over https".into(),
        PriceError::OnionSource => "Fob can't reach .onion price sources yet".into(),
        PriceError::Client(why) => why.clone(),
        PriceError::Failed(failure) => crate::prices::failure(*failure).into(),
    }
}

pub fn vault(e: &VaultError) -> String {
    vault_on(device(), e)
}

fn vault_on(device: Device, e: &VaultError) -> String {
    let storage = |why: &str| format!("couldn't read or save the wallet: {why}");
    match e {
        VaultError::BadSeed => "that isn't a valid seed phrase".into(),
        VaultError::BadKey => "that key isn't valid".into(),
        VaultError::ViewOnly => "this wallet can only watch: it has no spend key".into(),
        VaultError::Storage(why) => storage(why),
        VaultError::Sealing => storage("sealing"),
        VaultError::NotAWalletFile => storage("not a Fob wallet file"),
        VaultError::WrongKey => "the wallet file can't be opened with this key".into(),
        VaultError::WrongPassphrase => "That isn't the passphrase".into(),
        VaultError::PassphraseTooShort { min } => format!("A passphrase needs at least {min} characters"),
        VaultError::PassphraseExists => "This computer already has a passphrase".into(),
        VaultError::NameEmpty => "A wallet needs a name".into(),
        VaultError::NameTooLong { max } => format!("Keep the name to {max} characters"),
        VaultError::NameTaken { name } => format!("You already have a wallet called {name}"),
        VaultError::NotOnThisDevice => format!("That wallet isn't on this {}", device.noun()),
        VaultError::NotAWalletFileName => "Not a wallet file".into(),
        VaultError::Monero(e) => monero(*e),
    }
}

pub fn code(e: &CodeError) -> String {
    match e {
        CodeError::BadCode => "that code isn't a payment or wallet code Fob can read".into(),
        CodeError::TagHasNoCode => "That tag has no payment code.".into(),
        CodeError::ProfileCode(ProfileCodeProblem::IsPaymentCode) => "That's a payment code, not a profile.".into(),
        CodeError::ProfileCode(ProfileCodeProblem::NoName) => "That code has no name, so it isn't a profile.".into(),
        CodeError::ProfileCode(ProfileCodeProblem::PhotoTooBig) => "The photo in that profile is too big for Fob.".into(),
        CodeError::ProfileCode(ProfileCodeProblem::PhotoDamaged) => "The photo in that profile is damaged.".into(),
        CodeError::ProfileCode(ProfileCodeProblem::PhotoNotWebp) => "The photo in that profile isn't a WebP image.".into(),
        CodeError::PaymentTooBigForNfc => "That payment code is too big to send by touching phones.".into(),
        CodeError::ProfileTooBigForNfc => "Your profile is too big to swap by touching phones.".into(),
        CodeError::NameEmpty => "A contact needs a name.".into(),
        CodeError::NameTooLong { max } => format!("A name can be up to {max} characters."),
        CodeError::NoteTooLong { max } => format!("A note can be up to {max} characters."),
        CodeError::PhotoTooBig { bytes, max } => format!("That photo is {bytes} bytes; a contact's photo can be at most {max}."),
        CodeError::PhotoNotWebp => "That photo isn't a WebP image Fob can show.".into(),
        CodeError::PhotoTooWide { width, height, max } => format!("That photo is {width} × {height}; a contact's photo can be at most {max} × {max}."),
        CodeError::Monero(e) => monero(*e),
    }
}

pub fn screen(e: &ScreenError) -> String {
    match e {
        ScreenError::ContactDeleted => "That contact was deleted.".into(),
        ScreenError::AddressIsContacts { name } => format!("That address is {name}'s already: update {name} instead."),
        ScreenError::NameIsContacts { name } => format!("You have a contact named {name}: choose Update or Add as new contact."),
        ScreenError::RequestAddressUsed => "That address was already given to a request.".into(),
        ScreenError::ProfileNameEmpty => "Your profile needs a name.".into(),
        ScreenError::ProfileNameTooLong { max } => format!("A name can be up to {max} characters."),
        ScreenError::ProfilePhotoBad { max } => format!("Your photo must be a WebP of at most {max} bytes."),
        ScreenError::ProfileNotSetUp => "Set up your profile first.".into(),
        ScreenError::ShareAddressUsed => "That address was already given to a share.".into(),
        ScreenError::ShareDeleted => "That share was deleted.".into(),
        ScreenError::ShareNotFresh => "NFC works on a new Share my profile only.".into(),
        ScreenError::TooManyCoins { max } => format!("{max} coins is the most one split can make"),
        ScreenError::OverBudget => "That's more than is ready to spend, with the fee".into(),
        ScreenError::NoSuchCoin => "There's no such coin".into(),
        ScreenError::NotEnoughForCoin { coin } => format!("Not enough ready to spend for another {coin} coin"),
        ScreenError::NoCoinToSplit => "Add a coin to split".into(),
        ScreenError::NothingToSwap => "With XMR as the default currency there's nothing to swap".into(),
        ScreenError::NoPriceToConvert => "no price to convert with".into(),
        ScreenError::NotEnoughFunds { available, needed } => {
            format!("not enough unlocked money: {} available, {} needed", amount::format(*available), amount::format(*needed))
        }
        ScreenError::NoSuchPayment => "that payment was already sent or has expired".into(),
        ScreenError::Code(e) => code(e),
        ScreenError::Monero(e) => monero(*e),
    }
}

pub fn endpoint(e: &EndpointProblem) -> String {
    match e {
        EndpointProblem::Unreadable(text) => format!("{text} isn't an address Fob can read"),
        EndpointProblem::Onion => crate::node::NO_ONION.into(),
        EndpointProblem::ZmqEmpty => "enter the node's ZMQ address, like tcp://host:18083".into(),
        EndpointProblem::ZmqNotTcp => "Fob listens over tcp:// only".into(),
        EndpointProblem::ZmqNotHostAndPort => "only a host and port: tcp://host:18083".into(),
        EndpointProblem::ZmqNoHost => "add the host: tcp://host:18083".into(),
        EndpointProblem::ZmqNoPort => "add the port: tcp://host:18083".into(),
        EndpointProblem::ZmqOtherHost(host) => format!("instant updates come from your node's own host, {host}"),
        EndpointProblem::NoNodeSet => "set a node first".into(),
    }
}

/// A failure the node gave, in its library's words.
pub fn node(why: &str) -> String {
    // An onion node's refusal travels in the interface error's text.
    format!("the node didn't answer: {}", why.replace(fob_chain::node::ONION_REFUSED, crate::node::NO_ONION))
}

/// A failure building or publishing a transaction, in its library's words.
pub fn reorg_too_deep() -> String {
    "the chain changed further back than Fob can check".into()
}

/// With Manual Sync on, the node refused a payment. `said` is what follows
/// the words: what the node said, or nothing.
pub fn spend_rejected(said: &str) -> String {
    format!("your node refused this payment{said}")
}

pub fn send(why: &str) -> String {
    format!("couldn't build the transaction: {why}")
}

/// A Trezor couldn't be reached, refused, or answered wrongly: `why`, in
/// words already.
pub fn trezor(why: &str) -> String {
    why.to_owned()
}

pub fn trezor_cancelled() -> String {
    "cancelled on the Trezor".into()
}

pub fn trezor_needed() -> String {
    "connect the Trezor this wallet belongs to".into()
}

/// `n` coins were kept before Fob recorded transaction keys: the device
/// can't be asked about them until a rescan finds them again.
pub fn trezor_frozen(n: u32) -> String {
    let coins = if n == 1 { "one coin arrived".to_owned() } else { format!("{n} coins arrived") };
    format!("{coins} before Fob kept transaction keys, so the Trezor can't be asked about them: scan again from before they arrived")
}

/// Why a code can't be paid, when it's fine as a code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotTaken {
    WalletCode,
    NotPayment,
}

pub fn code_not_taken(why: NotTaken) -> String {
    match why {
        NotTaken::WalletCode => "That's a wallet code, not a payment code.".into(),
        NotTaken::NotPayment => "That isn't a payment code.".into(),
    }
}

/// The two passphrases typed aren't the same.
pub fn passphrases_differ() -> String {
    "Those passphrases aren't the same".into()
}

pub fn passphrase_changed() -> String {
    "Passphrase changed".into()
}

pub fn nothing_to_paste() -> String {
    "Nothing to paste.".into()
}

pub fn no_code_in_picture() -> String {
    "Fob couldn't find a code in that picture.".into()
}

pub fn photo_too_big() -> String {
    "Fob couldn't make that picture small enough to keep.".into()
}

pub fn profile_not_payment() -> String {
    "That's a profile, not a payment code. Add them from Contacts.".into()
}

/// Above a seed phrase on a computer, where nothing stops a screenshot.
pub fn seed_warning(words: usize) -> String {
    format!("These {words} words are your wallet. Write them down in order and keep them somewhere safe: anyone with them can spend your money, and without them it can't be restored. Fob can't keep them out of a screenshot on a computer.")
}

pub fn wallet_code_warning() -> String {
    "Anyone who scans this can use this wallet. Fob can't keep it out of a screenshot on a computer.".into()
}

/// Why two Fobs couldn't swap profiles by touching.
pub fn swap_failed(problem: &swap::Problem) -> String {
    match problem {
        swap::Problem::Away => "The other phone isn't on Share my profile.".into(),
        swap::Problem::Version => "The other phone has another version of Fob. Update both, or scan each other's code.".into(),
        swap::Problem::Garbled => "That didn't come through whole. Hold the phones together again.".into(),
        swap::Problem::Refused => "The other phone couldn't take your profile. Hold the phones together again.".into(),
        swap::Problem::NotProfile(e) => code(e),
    }
}

/// Why touching another Fob to pay it didn't work.
pub fn pay_failed(problem: &pay::Problem) -> String {
    match problem {
        pay::Problem::Away => "The other phone isn't on Receive, and isn't showing a Monero tag.".into(),
        pay::Problem::Version => "The other phone has another version of Fob. Update both, or scan its code.".into(),
        pay::Problem::Garbled => "That didn't come through whole. Hold the phones together again.".into(),
        pay::Problem::NotPayment(e) => code(e),
    }
}

#[cfg(test)]
mod tests {
    use fob_monero::network::Network;
    use fob_price::{Failure, Transport};

    use super::*;

    /// The words apps showed before these became enums, unchanged.
    #[test]
    fn errors_say_what_they_always_said() {
        let (main, stage) = (Network::Mainnet, Network::Stagenet);
        assert_eq!(monero(MoneroError::BadAmount), "that amount isn't valid");
        assert_eq!(monero(MoneroError::BadAddress(stage)), "that isn't a Monero address for stagenet");
        assert_eq!(monero(MoneroError::WrongNetwork { expected: stage, found: main }), "the wallet code is for mainnet, but this wallet is on stagenet");
        assert_eq!(price(&PriceError::NotOverTransport { provider: "Kraken", transport: Transport::Tor }), "Kraken doesn't answer over Tor");
        assert_eq!(price(&PriceError::Failed(Failure::Busy)), "Busy, try again soon");
        assert_eq!(vault(&VaultError::Storage("disk full".into())), "couldn't read or save the wallet: disk full");
        assert_eq!(vault(&VaultError::NotAWalletFile), "couldn't read or save the wallet: not a Fob wallet file");
        assert_eq!(vault(&VaultError::NameTooLong { max: 30 }), "Keep the name to 30 characters");
        assert_eq!(code(&CodeError::PhotoTooWide { width: 200, height: 100, max: 128 }), "That photo is 200 × 100; a contact's photo can be at most 128 × 128.");
        assert_eq!(code(&CodeError::ProfileCode(ProfileCodeProblem::NoName)), "That code has no name, so it isn't a profile.");
        assert_eq!(screen(&ScreenError::AddressIsContacts { name: "Sam".into() }), "That address is Sam's already: update Sam instead.");
        assert_eq!(screen(&ScreenError::NotEnoughFunds { available: 1_000_000_000_000, needed: 2_500_000_000_000 }), "not enough unlocked money: 1 available, 2.5 needed");
        assert_eq!(screen(&ScreenError::Code(CodeError::BadCode)), "that code isn't a payment or wallet code Fob can read");
        assert_eq!(endpoint(&EndpointProblem::ZmqOtherHost("node.example".into())), "instant updates come from your node's own host, node.example");
        assert_eq!(node("timed out"), "the node didn't answer: timed out");
        assert_eq!(send("no inputs"), "couldn't build the transaction: no inputs");
    }

    #[test]
    fn a_sentence_names_the_device_it_is_read_on() {
        assert_eq!(vault_on(Device::Phone, &VaultError::NotOnThisDevice), "That wallet isn't on this phone");
        assert_eq!(vault_on(Device::Computer, &VaultError::NotOnThisDevice), "That wallet isn't on this computer");
    }
}
