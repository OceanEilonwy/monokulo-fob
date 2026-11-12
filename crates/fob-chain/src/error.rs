//! What can be wrong with a node or ZMQ address. `fob-words` says it.

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EndpointProblem {
    /// Text that doesn't read as an address.
    Unreadable(String),
    /// On an onion service: no Tor yet, and looking it up would leak it.
    Onion,
    /// A login (`user:pass@`) over plain http to a host off this device and
    /// private networks: anyone on the way could read it.
    PlainLogin,
    ZmqEmpty,
    ZmqNotTcp,
    /// More than a host and port.
    ZmqNotHostAndPort,
    ZmqNoHost,
    ZmqNoPort,
    /// Instant updates come from the node's own host, this one.
    ZmqOtherHost(String),
    NoNodeSet,
}
