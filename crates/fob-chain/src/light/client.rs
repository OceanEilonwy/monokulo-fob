//! A light wallet server's client: one `post` and a method per call.
//!
//! It rides on the node's [`Transport`]: rustls with the same certificate
//! rule (a self-signed server pinned to the key it first answers with),
//! user:pass@ in the URL sent as Basic auth, no redirects, a 15 s connect
//! and 30 s idle limit (decision 28), and an onion host refused before
//! anything is looked up (decision 73).
//!
//! Every account call carries the wallet's private view key in its body:
//! the protocol has no other credential. Nothing here logs a request or puts
//! one in an error; a failure says the status or the transport's words.

use std::time::Duration;

use serde::{Serialize, de::DeserializeOwned};

use fob_monero::network::Network;

use super::{Problem, api_network, lws::*};
use crate::transport::{CertProblem, Transport};

/// The most a server's answer may be: a wallet's whole history of
/// transactions comes in one `/get_address_txs`, a few hundred bytes each.
const LIMIT: usize = 32 << 20;

/// How long the settings screen's Test waits for `/get_version`.
pub const ANSWER: Duration = Duration::from_secs(10);

/// A light wallet server.
#[derive(Clone)]
pub struct LightClient {
    transport: Transport,
    self_signed: bool,
}

impl LightClient {
    /// A client for the server at `url` (`https://host:port`, or `http://`
    /// on the local network); `self_signed` accepts its own certificate,
    /// pinned to the first key it answers with.
    pub fn new(url: &str, self_signed: bool) -> Result<LightClient, Problem> {
        Self::pinned(url, self_signed, None)
    }

    /// [`new`](LightClient::new), held to the key `pin` fingerprints, from an
    /// earlier connection.
    pub fn pinned(url: &str, self_signed: bool, pin: Option<[u8; 32]>) -> Result<LightClient, Problem> {
        let url = crate::node::normalize(url);
        crate::node::reachable(&url).map_err(|e| match e {
            crate::error::EndpointProblem::Onion => Problem::Onion,
            _ => Problem::BadAddress,
        })?;
        let transport = Transport::pinned(&url, self_signed, pin).map_err(|_| Problem::BadAddress)?;
        Ok(LightClient { transport, self_signed })
    }

    /// The fingerprint of the key a self-signed server is held to, once known.
    pub fn pin(&self) -> Option<[u8; 32]> {
        self.transport.pin()
    }

    /// The server's certificate at the last handshake.
    pub fn certificate(&self) -> Option<Result<(), CertProblem>> {
        self.transport.certificate()
    }

    /// POSTs `body` to `/route` and reads the answer, mapping the API's
    /// statuses to [`Problem`]s.
    pub async fn post<T: Serialize, U: DeserializeOwned>(&self, route: &str, body: &T) -> Result<U, Problem> {
        let body = serde_json::to_vec(body).map_err(|_| Problem::Unreadable)?;
        let (status, answer) = match self.transport.post_json(route, body, LIMIT).await {
            Ok(answered) => answered,
            Err(text) => {
                return Err(match self.certificate() {
                    Some(Err(problem)) if !self.self_signed || !problem.acceptable() => Problem::Certificate(problem),
                    _ => Problem::Unreachable(text),
                });
            }
        };
        // The server's own words, for the statuses that carry them: a line,
        // not a page.
        let said = || {
            let text = String::from_utf8_lossy(&answer);
            let text = serde_json::from_str::<serde_json::Value>(&text)
                .ok()
                .and_then(|v| v.get("message").or_else(|| v.get("error")).and_then(|m| m.as_str()).map(str::to_owned))
                .unwrap_or_else(|| text.trim().to_owned());
            text.chars().take(200).collect::<String>()
        };
        match status {
            200..=299 => serde_json::from_slice(&answer).map_err(|_| Problem::Unreadable),
            400 => Err(Problem::BadRequest(said())),
            403 => Err(Problem::Forbidden),
            409 => Err(Problem::Conflict(said())),
            501 => Err(Problem::NotTaken),
            503 => Err(Problem::Unavailable),
            other => Err(Problem::Status(other)),
        }
    }

    /// Registers the wallet, or finds it registered.
    pub async fn login(&self, login: &Login, create_account: bool, generated_locally: bool) -> Result<LoginResponse, Problem> {
        self.post("login", &LoginRequest { login: login.clone(), create_account, generated_locally }).await
    }

    pub async fn get_address_info(&self, login: &Login) -> Result<AddressInfo, Problem> {
        self.post("get_address_info", login).await
    }

    pub async fn get_address_txs(&self, login: &Login) -> Result<AddressTxs, Problem> {
        self.post("get_address_txs", login).await
    }

    pub async fn get_unspent_outs(&self, request: &UnspentOutsRequest) -> Result<UnspentOuts, Problem> {
        self.post("get_unspent_outs", request).await
    }

    /// `count` ring members for each of `amounts` (`0` for RingCT).
    pub async fn get_random_outs(&self, count: u32, amounts: Vec<u64>) -> Result<RandomOuts, Problem> {
        self.post("get_random_outs", &RandomOutsRequest { count, amounts: amounts.into_iter().map(Amount).collect() }).await
    }

    /// Hands the server a signed transaction to broadcast. A refusal (422)
    /// comes back as the answer, with its reason, not as a [`Problem`].
    pub async fn submit_raw_tx(&self, tx: Vec<u8>) -> Result<SubmitResponse, Problem> {
        match self.post::<_, SubmitResponse>("submit_raw_tx", &SubmitRequest { tx: HexBytes(tx.clone()) }).await {
            Err(Problem::Status(422)) => {
                Ok(SubmitResponse { status: "Failed".into(), reason: None, not_relayed: Some(true) })
            }
            other => other,
        }
    }

    pub async fn import_wallet_request(&self, login: &Login) -> Result<ImportResponse, Problem> {
        self.post("import_wallet_request", login).await
    }

    pub async fn get_subaddrs(&self, login: &Login) -> Result<Subaddrs, Problem> {
        self.post("get_subaddrs", login).await
    }

    /// Asks the server to watch `ranges` too.
    pub async fn upsert_subaddrs(&self, login: &Login, ranges: Vec<SubaddrRanges>) -> Result<NewSubaddrs, Problem> {
        self.post("upsert_subaddrs", &UpsertSubaddrsRequest { login: login.clone(), subaddrs: ranges, get_all: false }).await
    }

    pub async fn provision_subaddrs(&self, request: &ProvisionSubaddrsRequest) -> Result<NewSubaddrs, Problem> {
        self.post("provision_subaddrs", request).await
    }

    /// The server's type, API level, network, subaddress limit and height.
    /// Asks nothing of the wallet: no view key goes with it.
    pub async fn get_version(&self) -> Result<Version, Problem> {
        self.post("get_version", &serde_json::json!({})).await
    }
}

/// What the settings screen's Test found of a server.
#[derive(Clone, Debug)]
pub struct Report {
    pub url: String,
    pub self_signed: bool,
    /// "lws.example:8443": no scheme, no login.
    pub address: String,
    /// The wallet's network.
    pub ours: Network,
    /// Its `/get_version`, or why there was none.
    pub outcome: Result<Version, Problem>,
}

impl Report {
    /// Why this server can't be synced through, if it can't: no answer, the
    /// wrong network, or subaddresses off.
    pub fn refusal(&self) -> Option<Problem> {
        match &self.outcome {
            Err(problem) => Some(problem.clone()),
            Ok(v) if v.network != api_network(self.ours) => Some(Problem::OtherNetwork { theirs: v.network.clone(), ours: api_network(self.ours).to_owned() }),
            Ok(v) if v.max_subaddresses == 0 => Some(Problem::NoSubaddresses),
            Ok(_) => None,
        }
    }
}

/// The settings screen's Test: `/get_version` within [`ANSWER`], which
/// sends nothing of the wallet's.
pub async fn check(network: Network, url: &str, self_signed: bool, pin: Option<[u8; 32]>) -> Report {
    let normal = crate::node::normalize(url);
    let outcome = match LightClient::pinned(&normal, self_signed, pin) {
        Err(problem) => Err(problem),
        Ok(client) => match tokio::time::timeout(ANSWER, client.get_version()).await {
            Ok(answer) => answer,
            Err(_) => Err(Problem::Unreachable(format!("no answer in {} s", ANSWER.as_secs()))),
        },
    };
    Report { address: crate::node::address(&normal), url: normal, self_signed, ours: network, outcome }
}

#[cfg(test)]
mod tests;
