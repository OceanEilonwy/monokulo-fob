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
    /// A provider gave no price, and why in a word or two.
    #[error("{}", .0.text())]
    PriceFailed(crate::price::Failure),
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
    #[error("couldn't read or save the wallet: {0}")]
    Storage(String),
    #[error("the wallet file can't be opened with this key")]
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
    /// A Trezor couldn't be reached, refused, or answered wrongly, and why.
    #[error("{0}")]
    Trezor(String),
    /// The person said no on the Trezor, or to its PIN or passphrase.
    #[error("cancelled on the Trezor")]
    TrezorCancelled,
    /// This wallet's spend key is on a Trezor: the device has to be there.
    #[error("connect the Trezor this wallet belongs to")]
    TrezorNeeded,
}

pub type Result<T> = std::result::Result<T, FobError>;
