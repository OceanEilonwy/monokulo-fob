//! What the wallet and the apps see, whichever part of Fob an error began in.

pub use fob_chain::error::EndpointProblem;
pub use fob_codes::error::{CodeError, ProfileCodeProblem};
pub use fob_monero::error::MoneroError;
pub use fob_price::{Failure, error::PriceError};
pub use fob_screens::error::ScreenError;
pub use fob_vault::error::VaultError;

/// Everything that can go wrong in Fob, by what it's about. `Display` gives
/// the words for it, from `fob-words`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FobError {
    Monero(MoneroError),
    Price(PriceError),
    Vault(VaultError),
    Code(CodeError),
    Screen(ScreenError),
    Endpoint(EndpointProblem),
    /// The node's or its library's own words: they name no scenario Fob has.
    Node(String),
    /// Building or publishing a transaction, in the library's own words.
    Send(String),
    /// A reorganisation reaches further back than the block hashes kept, so
    /// what's known from before them can't be trusted without scanning again.
    ReorgTooDeep,
    /// With Manual Sync on, the node refused a payment. A coin in it may have
    /// been spent since the last scan. What the node said, if anything.
    SpendRejected(String),
    /// A Trezor couldn't be reached, refused, or answered wrongly, and why.
    Trezor(String),
    /// The person said no on the Trezor, or to its PIN or passphrase.
    TrezorCancelled,
    /// This wallet's spend key is on a Trezor: the device has to be there.
    TrezorNeeded,
    /// A light wallet server refused, failed or lied.
    Light(fob_chain::light::Problem),
}

impl FobError {
    /// The file system's or database's own words for a failure.
    pub fn storage(why: impl Into<String>) -> FobError {
        FobError::Vault(VaultError::Storage(why.into()))
    }

    /// Whether this is about keeping the wallet on this device, not the node.
    pub fn is_storage(&self) -> bool {
        matches!(self, FobError::Vault(VaultError::Storage(_) | VaultError::Sealing | VaultError::NotAWalletFile))
    }
}

impl std::fmt::Display for FobError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&match self {
            FobError::Monero(e) => fob_words::errors::monero(*e),
            FobError::Price(e) => fob_words::errors::price(e),
            FobError::Vault(e) => fob_words::errors::vault(e),
            FobError::Code(e) => fob_words::errors::code(e),
            FobError::Screen(e) => fob_words::errors::screen(e),
            FobError::Endpoint(e) => fob_words::errors::endpoint(e),
            FobError::Node(why) => fob_words::errors::node(why),
            FobError::Send(why) => fob_words::errors::send(why),
            FobError::ReorgTooDeep => fob_words::errors::reorg_too_deep(),
            FobError::SpendRejected(said) => fob_words::errors::spend_rejected(said),
            FobError::Trezor(why) => fob_words::errors::trezor(why),
            FobError::TrezorCancelled => fob_words::errors::trezor_cancelled(),
            FobError::TrezorNeeded => fob_words::errors::trezor_needed(),
            FobError::Light(problem) => fob_words::light::problem(problem),
        })
    }
}

impl std::error::Error for FobError {}

impl From<MoneroError> for FobError {
    fn from(e: MoneroError) -> Self {
        FobError::Monero(e)
    }
}
impl From<PriceError> for FobError {
    fn from(e: PriceError) -> Self {
        FobError::Price(e)
    }
}
impl From<VaultError> for FobError {
    fn from(e: VaultError) -> Self {
        FobError::Vault(e)
    }
}
impl From<fob_keys::KeyError> for FobError {
    fn from(e: fob_keys::KeyError) -> Self {
        FobError::Vault(e.into())
    }
}
impl From<CodeError> for FobError {
    fn from(e: CodeError) -> Self {
        FobError::Code(e)
    }
}
impl From<ScreenError> for FobError {
    fn from(e: ScreenError) -> Self {
        FobError::Screen(e)
    }
}
impl From<EndpointProblem> for FobError {
    fn from(e: EndpointProblem) -> Self {
        FobError::Endpoint(e)
    }
}

pub type Result<T> = std::result::Result<T, FobError>;
