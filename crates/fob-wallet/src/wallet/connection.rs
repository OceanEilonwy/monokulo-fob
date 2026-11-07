//! The node in use, its connection, and instant updates over ZMQ.

use super::*;

impl Wallet {
    pub async fn node(&self) -> node::Setting {
        let state = self.inner.state.lock().await;
        node::Setting { url: state.node.clone(), self_signed: state.node_self_signed }
    }

    /// The key a self-signed node in use is held to, as the hex of its pin,
    /// for the Node screen to show so it can be compared with the node's own.
    /// None until the node has first answered, or when it isn't self-signed.
    pub async fn node_pin(&self) -> Option<String> {
        let state = self.inner.state.lock().await;
        state.node_pin.clone().filter(|_| state.node_self_signed)
    }

    /// Whether the node in use last answered with a key other than its pin:
    /// someone else answering, or the node given a new certificate. Only the
    /// user can tell which, so the Node screen asks before trusting it.
    pub fn certificate_changed(&self) -> bool {
        let link = self.inner.link.lock().expect("link lock");
        link.as_ref().is_some_and(|(_, transport)| transport.certificate() == Some(Err(transport::CertProblem::Changed)))
    }

    /// Settings' Node line: the last check of it ("Working · node.example:38081")
    /// when it's newer than syncing's last failure, else "Can't reach node
    /// · …" while syncing fails (as Home's chip), else what syncing says,
    /// else just the address. A failure saving the wallet isn't the node's.
    pub async fn node_line(&self) -> String {
        let setting = self.node().await;
        let address = node::address(&setting.url);
        let checked = self.inner.node_verdict.lock().expect("node lock").clone();
        let status = self.status();
        let verdict = match checked {
            // A failed round drops the check before it: what's here is newer.
            Some((checked, verdict)) if checked == setting => Some(fob_words::node::verdict(verdict)),
            _ if status.error.is_some() && !status.error_local && !status.chain_changed => Some(fob_words::catch_up::NODE_FAILING),
            _ if status.connected => Some(fob_words::catch_up::NODE_WORKING),
            _ => None,
        };
        verdict.map_or(address.clone(), |v| format!("{v} · {address}"))
    }

    /// Checks `setting`'s node, telling `progress` as each check finishes.
    pub async fn check_node(&self, setting: &node::Setting, progress: &(dyn Fn(&node::Report) + Sync)) -> node::Report {
        let tip = self.inner.state.lock().await.tip;
        let report = node::check(self.network(), setting, tip, now(), progress).await;
        let current = self.node().await;
        if node::normalize(&setting.url) == current.url && setting.self_signed == current.self_signed {
            *self.inner.node_verdict.lock().expect("node lock") = Some((current, report.verdict()));
        }
        report
    }

    /// The nodes to offer besides the one in use: the network's own, then this
    /// wallet's custom ones, each once.
    pub async fn other_nodes(&self) -> Vec<node::Setting> {
        let (current, custom) = {
            let state = self.inner.state.lock().await;
            (state.node.clone(), state.custom_nodes.clone())
        };
        let defaults = self.network().default_nodes().iter().map(|url| node::Setting { url: (*url).to_owned(), self_signed: false });
        let mut out: Vec<node::Setting> = vec![];
        for n in defaults.chain(custom) {
            let url = node::normalize(&n.url);
            if url != current && !out.iter().any(|o| o.url == url) {
                out.push(node::Setting { url, ..n });
            }
        }
        out
    }

    /// The nodes this wallet was given, besides its network's own.
    pub async fn custom_nodes(&self) -> Vec<node::Setting> {
        self.inner.state.lock().await.custom_nodes.clone()
    }

    /// Keeps a node to change to later. One of the network's own is on the list
    /// already; one kept already takes the certificate choice given now.
    pub async fn add_node(&self, url: &str, self_signed: bool) -> Result<()> {
        let url = node::normalize(url);
        node::reachable(&url).map_err(FobError::Endpoint)?;
        let mut state = self.inner.state.lock().await;
        if self.keep_node(&mut state, node::Setting { url, self_signed }) {
            self.save_state(&state)?;
        }
        Ok(())
    }

    /// Forgets a custom node. The one in use carries on, and is kept again when
    /// the wallet changes away from it.
    pub async fn remove_node(&self, url: &str) -> Result<()> {
        let url = node::normalize(url);
        let mut state = self.inner.state.lock().await;
        let before = state.custom_nodes.len();
        state.custom_nodes.retain(|n| n.url != url);
        if state.custom_nodes.len() != before {
            self.save_state(&state)?;
        }
        Ok(())
    }

    /// Puts `setting` on the wallet's custom nodes, unless it's one of the
    /// network's own. Whether that changed anything.
    pub(super) fn keep_node(&self, state: &mut State, setting: node::Setting) -> bool {
        if self.network().default_nodes().iter().any(|url| node::normalize(url) == setting.url) {
            return false;
        }
        match state.custom_nodes.iter_mut().find(|n| n.url == setting.url) {
            Some(kept) if kept.self_signed == setting.self_signed => false,
            Some(kept) => {
                kept.self_signed = setting.self_signed;
                true
            }
            None => {
                state.custom_nodes.push(setting);
                true
            }
        }
    }

    /// Checks the other nodes at once, each with its line for the list.
    pub async fn check_nodes(&self, nodes: &[node::Setting]) -> Vec<(node::Report, node::Line)> {
        let tip = self.inner.state.lock().await.tip;
        node::check_all(self.network(), nodes, tip, now()).await
    }

    /// Talks to another node from now on; `self_signed` accepts its
    /// certificate unverified. It's kept among the wallet's custom nodes even if
    /// it's refused, and so is the one being left, as a wallet from before the
    /// list has its custom node nowhere else.
    pub async fn set_node(&self, url: &str, self_signed: bool) -> Result<()> {
        let url = crate::node::normalize(url);
        // An onion node can't be reached without Tor, and isn't looked up.
        crate::node::reachable(&url).map_err(FobError::Endpoint)?;
        // A node on the other network would never find this wallet's money.
        let setting = node::Setting { url: url.clone(), self_signed };
        {
            let mut state = self.inner.state.lock().await;
            let leaving = node::Setting { url: state.node.clone(), self_signed: state.node_self_signed };
            let left = self.keep_node(&mut state, leaving);
            if self.keep_node(&mut state, setting.clone()) | left {
                self.save_state(&state)?;
            }
        }
        crate::node::on_network(self.network(), &setting).await.map_err(|refused| FobError::Node(fob_words::node::switch_refused(&refused)))?;
        let zmq_dropped = {
            let mut state = self.inner.state.lock().await;
            // Instant updates belong to a node's host: on another host they
            // go off, and the new host's suggestion is offered instead.
            let dropped = state.zmq_endpoint.as_ref().is_some_and(|e| zmq::endpoint(&url, e).is_err());
            if dropped {
                state.zmq_endpoint = None;
                state.zmq_on = false;
            }
            state.node = url;
            state.node_self_signed = self_signed;
            state.node_pin = None;
            self.save_state(&state)?;
            dropped
        };
        if zmq_dropped {
            self.restart_zmq().await;
        }
        // The sync loop goes first: it may be waiting minutes on the old node,
        // holding what's cleared below. It starts again on the new one at once,
        // rather than at the end of a wait or a backoff.
        let listener = self.inner.listener.lock().expect("listener lock").clone();
        let looping = self.inner.syncing.lock().expect("sync lock").is_some();
        self.stop_sync();
        *self.inner.rpc.lock().await = None;
        *self.inner.link.lock().expect("link lock") = None;
        *self.inner.pipeline.lock().await = None;
        *self.inner.node_verdict.lock().expect("node lock") = None;
        // The old node's failure, and its Synced, aren't this one's: the chip
        // and Node line stop saying them now.
        let status = self.set_status(|s| {
            s.error = None;
            s.error_local = false;
            s.connected = false;
            s.waiting_seconds = 0;
        });
        if let Some(listener) = listener {
            if looping {
                self.start_sync(listener);
            } else {
                listener.on_status(status);
            }
        }
        Ok(())
    }

    /// The node's ZMQ publisher: the one set, else the usual port on the
    /// node's host, offered for Settings to show and edit.
    pub async fn zmq_endpoint(&self) -> String {
        let state = self.inner.state.lock().await;
        state.zmq_endpoint.clone().or_else(|| zmq::suggest(self.network(), &state.node)).unwrap_or_default()
    }

    /// Turns instant updates on or off. `endpoint` (checked: TCP, on the
    /// node's own host) replaces the one set; without it, the one set or
    /// the suggestion is used. Takes effect at once while syncing.
    pub async fn set_zmq(&self, on: bool, endpoint: Option<&str>) -> Result<()> {
        {
            let mut state = self.inner.state.lock().await;
            let typed = match endpoint {
                Some(typed) => Some(typed.to_owned()),
                None if on => state.zmq_endpoint.clone().or_else(|| zmq::suggest(self.network(), &state.node)),
                // Off, nothing typed: the one set stays for next time.
                None => None,
            };
            if let Some(typed) = typed {
                state.zmq_endpoint = Some(zmq::endpoint(&state.node, &typed).map_err(FobError::Endpoint)?);
            } else if on {
                return Err(FobError::Endpoint(EndpointProblem::NoNodeSet));
            }
            state.zmq_on = on;
            self.save_state(&state)?;
        }
        self.restart_zmq().await;
        Ok(())
    }

    /// Starts the ZMQ subscriber afresh from the settings, if the sync loop
    /// runs and instant updates are on; otherwise just stops it.
    pub(super) async fn restart_zmq(&self) {
        if let Some((_, task)) = self.inner.zmq_task.lock().expect("zmq lock").take() {
            task.abort();
        }
        let (on, endpoint) = {
            let state = self.inner.state.lock().await;
            (state.zmq_on && !state.manual_sync, state.zmq_endpoint.clone())
        };
        self.set_zmq_status(if on { ZmqStatus::Connecting } else { ZmqStatus::Off }, endpoint.as_deref());
        let listener = self.inner.listener.lock().expect("listener lock").clone();
        let syncing = self.inner.syncing.lock().expect("sync lock").as_ref().is_some_and(|h| !h.is_finished());
        if let (true, Some(endpoint), Some(listener), true) = (on, endpoint, listener, syncing) {
            let events = ZmqEvents { wallet: self.clone(), endpoint: endpoint.clone(), listener };
            let task = crate::runtime().spawn(zmq::listen(endpoint.clone(), events));
            if let Some((_, old)) = self.inner.zmq_task.lock().expect("zmq lock").replace((endpoint, task)) {
                old.abort();
            }
            // Stopped meanwhile: stop this one too.
            if self.inner.syncing.lock().expect("sync lock").is_none() {
                self.stop_sync();
            }
        }
    }

    pub(super) fn set_zmq_status(&self, zmq: ZmqStatus, endpoint: Option<&str>) -> SyncStatus {
        self.set_status(|s| {
            s.zmq_endpoint = endpoint.map(str::to_owned);
            s.zmq = zmq;
        })
    }

    pub(super) async fn rpc(&self) -> Result<Daemon> {
        Ok(self.connection().await?.1)
    }

    /// The node: as a raw transport (for [`fetch`](crate::fetch)), as a
    /// daemon, and as a source of decoys with the output distribution cached.
    pub(super) async fn connection(&self) -> Result<Connection> {
        let node = self.node().await;
        // Kept for the node and its mark together: a mark taken off while a
        // connection was being made never leaves one accepting any certificate.
        if let Some((setting, connection)) = self.inner.rpc.lock().await.as_ref()
            && *setting == node
        {
            return Ok(connection.clone());
        }
        // Not held while the node is asked: a slow or dead one would keep
        // anything else wanting a connection (changing node, say) waiting.
        let err = |e: InterfaceError| FobError::Node(format!("{e:?}"));
        let pin = self.inner.state.lock().await.node_pin.as_deref().and_then(|p| hex::decode(p).ok()?.try_into().ok());
        let transport = Transport::pinned(&node.url, node.self_signed, pin).map_err(err)?;
        *self.inner.link.lock().expect("link lock") = Some((node.clone(), transport.clone()));
        let daemon = monero_daemon_rpc::MoneroDaemon::new(transport.clone()).await.map_err(err)?;
        if pin.is_none() {
            self.keep_pin(&node, &transport).await?;
        }
        let decoys = Decoys::new(daemon.clone(), transport.clone(), self.decoys_path());
        let adaptive = Arc::new(Adaptive::new(Tuning::DEFAULT));
        let connection = (transport, daemon, decoys, adaptive);
        let current = self.node().await;
        let mut rpc = self.inner.rpc.lock().await;
        if let Some((setting, made)) = rpc.as_ref()
            && *setting == node
        {
            return Ok(made.clone());
        }
        // The node changed meanwhile: this one is only for whoever asked.
        if current == node {
            *rpc = Some((node, connection.clone()));
        }
        Ok(connection)
    }

    /// Keeps the key a self-signed node answered with first, to hold it to
    /// from now on, while it's still the node in use.
    pub(super) async fn keep_pin(&self, node: &node::Setting, transport: &Transport) -> Result<()> {
        let Some(pin) = transport.pin().filter(|_| node.self_signed) else { return Ok(()) };
        let mut state = self.inner.state.lock().await;
        if state.node == node.url && state.node_self_signed && state.node_pin.is_none() {
            state.node_pin = Some(hex::encode(pin));
            self.save_state(&state)?;
        }
        Ok(())
    }

    /// The output distribution's file. It's public chain data, the same for
    /// every wallet on the network, and tens of megabytes: they share it.
    pub(super) fn decoys_path(&self) -> PathBuf {
        self.inner.path.with_file_name(format!("wallet-{}.decoys", self.network().name()))
    }
}
