//! A ZMQ publisher standing in for monerod's `--zmq-pub` in the
//! subscriber's tests: ZMTP 3.0 (rfc.zeromq.org/spec/23) with the NULL
//! mechanism, written from the spec rather than with `crate::zmtp`, so a
//! misreading shared by both ends can't pass unnoticed.
//!
//! Like libzmq's PUB, it sends each message only to the subscribers with a
//! subscription that prefixes it, and drops what it sends before a
//! subscription arrives. Closing it, or dropping it, closes every
//! connection, as monerod stopping would.
//!
//! Copied from Monokulo's engine
//! (`crates/engine/tests/verification/notifications/publisher.rs`), TCP
//! only, with std's mutex.

use std::io;
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio::task::{JoinHandle, JoinSet};

pub struct Publisher {
    endpoint: String,
    peers: Arc<Mutex<Vec<Peer>>>,
    /// Owns every connection's task, so aborting it closes them all.
    accepting: JoinHandle<()>,
}

struct Peer {
    id: u64,
    subscriptions: Vec<Vec<u8>>,
    outbox: mpsc::UnboundedSender<Vec<u8>>,
}

impl Publisher {
    /// Listens at `endpoint`: `tcp://127.0.0.1:0` for a free port, or one
    /// a closed publisher had (which may take a moment to be free again).
    pub async fn bind(endpoint: &str) -> Self {
        let address = endpoint.strip_prefix("tcp://").unwrap_or_else(|| panic!("{endpoint} is not an endpoint this publisher binds"));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let listener = loop {
            match TcpListener::bind(address).await {
                Ok(bound) => break bound,
                Err(error) if error.kind() == io::ErrorKind::AddrInUse => {
                    assert!(std::time::Instant::now() < deadline, "{endpoint}: {error}");
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
                Err(error) => panic!("{endpoint}: {error}"),
            }
        };
        let endpoint = format!("tcp://{}", listener.local_addr().unwrap());
        let peers = Arc::new(Mutex::new(Vec::new()));
        let accepting = tokio::spawn(accept(listener, Arc::clone(&peers)));
        Self { endpoint, peers, accepting }
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// Sends `message` as one frame to every subscriber it matches.
    pub fn send(&self, message: &str) {
        let frame = frame(0, message.as_bytes());
        self.peers.lock().expect("peers lock").retain(|peer| {
            let wanted = peer.subscriptions.iter().any(|topic| message.as_bytes().starts_with(topic));
            !wanted || peer.outbox.send(frame.clone()).is_ok()
        });
    }

    /// Closes the listener and every connection.
    pub fn close(&self) {
        self.accepting.abort();
    }
}

impl Drop for Publisher {
    fn drop(&mut self) {
        self.close();
    }
}

async fn accept(listener: TcpListener, peers: Arc<Mutex<Vec<Peer>>>) {
    let mut connections = JoinSet::new();
    let mut next_id = 0;
    loop {
        tokio::select! {
            accepted = listener.accept() => {
                let Ok((stream, _)) = accepted else { continue };
                next_id += 1;
                let (id, peers) = (next_id, Arc::clone(&peers));
                connections.spawn(async move {
                    // A subscriber that breaks the protocol just loses its
                    // connection, as with libzmq.
                    let _ = serve(stream, id, &peers).await;
                    peers.lock().expect("peers lock").retain(|peer| peer.id != id);
                });
            }
            Some(_) = connections.join_next() => {}
        }
    }
}

async fn serve(
    stream: tokio::net::TcpStream,
    id: u64,
    peers: &Mutex<Vec<Peer>>,
) -> io::Result<()> {
    let (mut reader, mut writer) = tokio::io::split(stream);
    let mut greeting = [0u8; 64];
    greeting[0] = 0xFF;
    greeting[9] = 0x7F;
    greeting[10] = 3;
    greeting[12..16].copy_from_slice(b"NULL");
    writer.write_all(&greeting).await?;
    let mut theirs = [0u8; 64];
    reader.read_exact(&mut theirs).await?;
    if theirs[0] != 0xFF
        || theirs[9] != 0x7F
        || theirs[10] != 3
        || theirs[12..32] != greeting[12..32]
    {
        return Err(io::ErrorKind::InvalidData.into());
    }
    writer
        .write_all(&frame(0x04, b"\x05READY\x0bSocket-Type\x00\x00\x00\x03PUB"))
        .await?;
    let (flags, ready) = read_frame(&mut reader).await?;
    if flags != 0x04 || ready != b"\x05READY\x0bSocket-Type\x00\x00\x00\x03SUB" {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let (outbox, mut outgoing) = mpsc::unbounded_channel();
    peers.lock().expect("peers lock").push(Peer {
        id,
        subscriptions: Vec::new(),
        outbox,
    });
    let sending = async {
        while let Some(frame) = outgoing.recv().await {
            writer.write_all(&frame).await?;
        }
        io::Result::Ok(())
    };
    let subscribing = async {
        loop {
            let (flags, body) = read_frame(&mut reader).await?;
            let mut peers = peers.lock().expect("peers lock");
            let Some(peer) = peers.iter_mut().find(|peer| peer.id == id) else {
                return io::Result::Ok(());
            };
            match (flags, body.split_first()) {
                (0, Some((1, topic))) => peer.subscriptions.push(topic.to_vec()),
                (0, Some((0, topic))) => peer.subscriptions.retain(|t| t != topic),
                _ => {}
            }
        }
    };
    tokio::select! {
        sent = sending => sent,
        read = subscribing => read,
    }
}

async fn read_frame(reader: &mut (impl AsyncRead + Unpin)) -> io::Result<(u8, Vec<u8>)> {
    let flags = reader.read_u8().await?;
    let size = if flags & 0x02 == 0 {
        u64::from(reader.read_u8().await?)
    } else {
        reader.read_u64().await?
    };
    // Tests send small frames; anything else is a bug worth seeing.
    let mut body = vec![0; usize::try_from(size).unwrap().min(1 << 16)];
    reader.read_exact(&mut body).await?;
    Ok((flags, body))
}

fn frame(flags: u8, body: &[u8]) -> Vec<u8> {
    let mut frame = vec![flags];
    if let Ok(size) = u8::try_from(body.len()) {
        frame.push(size);
    } else {
        frame[0] |= 0x02;
        frame.extend_from_slice(&(body.len() as u64).to_be_bytes());
    }
    frame.extend_from_slice(body);
    frame
}
