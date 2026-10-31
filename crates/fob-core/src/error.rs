/// Everything that can go wrong in Fob, in words an app can show.
#[derive(Debug, thiserror::Error)]
pub enum FobError {
    #[error("that isn't a valid seed phrase")]
    BadSeed,
    #[error("that isn't a Monero address for {0}")]
    BadAddress(&'static str),
    #[error("that key isn't valid")]
    BadKey,
    #[error("that amount isn't valid")]
    BadAmount,
    #[error("that code isn't a payment or wallet code Fob can read")]
    BadCode,
    #[error("the wallet code is for {found}, but this wallet is on {expected}")]
    WrongNetwork { expected: &'static str, found: &'static str },
    #[error("this wallet can only watch: it has no spend key")]
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
    /// Split up's pieces can't be split, and why.
    #[error("{0}")]
    Split(String),
    /// A contact can't be saved as it is, and why.
    #[error("{0}")]
    Contact(String),
    /// A request can't be made or found, and why.
    #[error("{0}")]
    Request(String),
    #[error("couldn't build the transaction: {0}")]
    Send(String),
    #[error("couldn't read or save the wallet: {0}")]
    Storage(String),
    #[error("the wallet file can't be opened with this key")]
    WrongKey,
    #[error("that payment was already sent or has expired")]
    NoSuchPayment,
}

pub type Result<T> = std::result::Result<T, FobError>;
