//! What the wallet and the apps see, whichever part of Fob an error began in.

pub use fob_codes::error::CodeError;
pub use fob_monero::error::MoneroError;
pub use fob_price::{Failure, error::PriceError};
pub use fob_screens::error::ScreenError;
pub use fob_vault::error::VaultError;

/// Everything that can go wrong in Fob, in words an app can show.
#[derive(Debug, thiserror::Error)]
pub enum FobError {
    #[error("{}", VaultError::BadSeed)]
    BadSeed,
    #[error("{}", MoneroError::BadAddress(.0))]
    BadAddress(&'static str),
    #[error("{}", VaultError::BadKey)]
    BadKey,
    #[error("{}", MoneroError::BadAmount)]
    BadAmount,
    #[error("{}", CodeError::BadCode)]
    BadCode,
    #[error("{}", MoneroError::WrongNetwork { expected, found })]
    WrongNetwork { expected: &'static str, found: &'static str },
    #[error("{}", VaultError::ViewOnly)]
    ViewOnly,
    #[error("not enough unlocked money: {available} available, {needed} needed")]
    NotEnoughFunds { available: String, needed: String },
    /// An instant-updates (ZMQ) address that isn't right, and why.
    #[error("{0}")]
    BadEndpoint(String),
    #[error("the node didn't answer: {0}")]
    Node(String),
    /// Prices couldn't be had, or a price setting isn't one Fob has.
    #[error("{0}")]
    Price(String),
    /// A provider gave no price, and why in a word or two.
    #[error("{}", .0.text())]
    PriceFailed(Failure),
    /// Split up's pieces can't be split, and why.
    #[error("{0}")]
    Split(String),
    /// A contact can't be saved as it is, and why.
    #[error("{0}")]
    Contact(String),
    /// A request can't be made or found, and why.
    #[error("{0}")]
    Request(String),
    /// Your profile can't be saved or shared as it is, or a scanned one
    /// can't be read, and why.
    #[error("{0}")]
    Profile(String),
    /// A wallet's name can't be used, or the wallet isn't on the list, and why.
    #[error("{0}")]
    BadName(String),
    #[error("couldn't build the transaction: {0}")]
    Send(String),
    #[error("{}", VaultError::Storage(.0.clone()))]
    Storage(String),
    #[error("{}", VaultError::WrongKey)]
    WrongKey,
    #[error("that payment was already sent or has expired")]
    NoSuchPayment,
    /// A reorganisation reaches further back than the block hashes kept, so
    /// what's known from before them can't be trusted without scanning again.
    #[error("the chain changed further back than Fob can check")]
    ReorgTooDeep,
    /// With Manual Sync on, the node refused a payment. A coin in it may have
    /// been spent since the last scan.
    #[error("your node refused this payment{0}")]
    SpendRejected(String),
}

impl From<MoneroError> for FobError {
    fn from(e: MoneroError) -> Self {
        match e {
            MoneroError::BadAmount => FobError::BadAmount,
            MoneroError::BadAddress(network) => FobError::BadAddress(network),
            MoneroError::WrongNetwork { expected, found } => FobError::WrongNetwork { expected, found },
        }
    }
}

impl From<PriceError> for FobError {
    fn from(e: PriceError) -> Self {
        match e {
            PriceError::Price(why) => FobError::Price(why),
            PriceError::Failed(failure) => FobError::PriceFailed(failure),
        }
    }
}

impl From<VaultError> for FobError {
    fn from(e: VaultError) -> Self {
        match e {
            VaultError::BadSeed => FobError::BadSeed,
            VaultError::BadKey => FobError::BadKey,
            VaultError::ViewOnly => FobError::ViewOnly,
            VaultError::Storage(why) => FobError::Storage(why),
            VaultError::WrongKey => FobError::WrongKey,
            VaultError::BadName(why) => FobError::BadName(why),
            VaultError::Monero(e) => e.into(),
        }
    }
}

impl From<CodeError> for FobError {
    fn from(e: CodeError) -> Self {
        match e {
            CodeError::BadCode => FobError::BadCode,
            CodeError::Profile(why) => FobError::Profile(why),
            CodeError::Contact(why) => FobError::Contact(why),
            CodeError::Monero(e) => e.into(),
        }
    }
}

impl From<ScreenError> for FobError {
    fn from(e: ScreenError) -> Self {
        match e {
            ScreenError::Contact(why) => FobError::Contact(why),
            ScreenError::Request(why) => FobError::Request(why),
            ScreenError::Profile(why) => FobError::Profile(why),
            ScreenError::Code(e) => e.into(),
            ScreenError::Monero(e) => e.into(),
        }
    }
}

pub type Result<T> = std::result::Result<T, FobError>;

#[cfg(test)]
mod tests {
    use super::*;

    /// Whatever kind an error began as, the apps see the same words and the
    /// same variant they always have.
    #[test]
    fn each_kind_keeps_its_words_as_the_wallet_s_error() {
        let monero = || vec![MoneroError::BadAmount, MoneroError::BadAddress("stagenet"), MoneroError::WrongNetwork { expected: "stagenet", found: "mainnet" }];
        for e in monero() {
            assert_eq!(FobError::from(e.clone()).to_string(), e.to_string());
        }
        for e in [PriceError::Price("no such".into()), PriceError::Failed(Failure::TorNotYet)] {
            assert_eq!(FobError::from(e.clone()).to_string(), e.to_string());
        }
        let mut vault = vec![VaultError::BadSeed, VaultError::BadKey, VaultError::ViewOnly, VaultError::Storage("disk".into()), VaultError::WrongKey, VaultError::BadName("taken".into())];
        vault.extend(monero().into_iter().map(VaultError::Monero));
        for e in vault {
            assert_eq!(FobError::from(e.clone()).to_string(), e.to_string());
        }
        let mut code = vec![CodeError::BadCode, CodeError::Profile("big".into()), CodeError::Contact("long".into())];
        code.extend(monero().into_iter().map(CodeError::Monero));
        for e in code.clone() {
            assert_eq!(FobError::from(e.clone()).to_string(), e.to_string());
        }
        let mut screen = vec![ScreenError::Contact("a".into()), ScreenError::Request("b".into()), ScreenError::Profile("c".into())];
        screen.extend(code.into_iter().map(ScreenError::Code));
        screen.extend(monero().into_iter().map(ScreenError::Monero));
        for e in screen {
            assert_eq!(FobError::from(e.clone()).to_string(), e.to_string());
        }
    }

    #[test]
    fn nested_kinds_flatten_to_the_one_variant() {
        assert!(matches!(FobError::from(ScreenError::Code(CodeError::Monero(MoneroError::BadAmount))), FobError::BadAmount));
        assert!(matches!(FobError::from(CodeError::Contact("x".into())), FobError::Contact(_)));
        assert!(matches!(FobError::from(VaultError::Monero(MoneroError::BadAddress("x"))), FobError::BadAddress("x")));
    }
}
