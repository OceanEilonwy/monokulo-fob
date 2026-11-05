//! Why a code can't be read or made, or a name or photo kept. `fob-words` says it.

use fob_monero::error::MoneroError;

/// Why a scanned code isn't a profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfileCodeProblem {
    IsPaymentCode,
    NoName,
    PhotoTooBig,
    PhotoDamaged,
    PhotoNotWebp,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CodeError {
    BadCode,
    /// An NFC tag with no payment code on it.
    TagHasNoCode,
    ProfileCode(ProfileCodeProblem),
    /// Too big to send by touching phones.
    PaymentTooBigForNfc,
    ProfileTooBigForNfc,
    NameEmpty,
    NameTooLong { max: usize },
    NoteTooLong { max: usize },
    PhotoTooBig { bytes: usize, max: usize },
    PhotoNotWebp,
    PhotoTooWide { width: u32, height: u32, max: u32 },
    Monero(MoneroError),
}

impl From<MoneroError> for CodeError {
    fn from(e: MoneroError) -> Self {
        CodeError::Monero(e)
    }
}

pub type Result<T> = std::result::Result<T, CodeError>;
