//! What an action on a screen can refuse. `fob-words` says it.

use fob_codes::error::CodeError;
use fob_monero::error::MoneroError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScreenError {
    ContactDeleted,
    AddressIsContacts { name: String },
    NameIsContacts { name: String },
    RequestAddressUsed,
    ProfileNameEmpty,
    ProfileNameTooLong { max: usize },
    ProfilePhotoBad { max: usize },
    ProfileNotSetUp,
    ShareAddressUsed,
    ShareDeleted,
    /// NFC only works on a share just made.
    ShareNotFresh,
    TooManyCoins { max: u32 },
    OverBudget,
    NoSuchCoin,
    NotEnoughForCoin { coin: String },
    NoCoinToSplit,
    NothingToSwap,
    NoPriceToConvert,
    NotEnoughFunds { available: u64, needed: u64 },
    NoSuchPayment,
    Code(CodeError),
    Monero(MoneroError),
}

impl From<CodeError> for ScreenError {
    fn from(e: CodeError) -> Self {
        ScreenError::Code(e)
    }
}

impl From<MoneroError> for ScreenError {
    fn from(e: MoneroError) -> Self {
        ScreenError::Monero(e)
    }
}

pub type Result<T> = std::result::Result<T, ScreenError>;
