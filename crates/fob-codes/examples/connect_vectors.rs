//! Writes Connect to Fob's version 1 vectors (`vectors/connect-v1.json`)
//! from fixed keys and grants. The file is the reference: change it only
//! with a new protocol version.
//!
//! cargo run -p fob-codes --example connect_vectors > crates/fob-codes/vectors/connect-v1.json

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD as B64};
use fob_codes::connect::{self, Asker, Grant, SharedNode, SharedPrices, Want};
use fob_monero::network::Network;
use serde_json::{Value, json};

struct Input {
    name: &'static str,
    app_secret: [u8; 32],
    state: [u8; 16],
    app: &'static str,
    wallet_name: &'static str,
    device: &'static str,
    network: Network,
    want: Want,
    callback: Option<&'static str>,
    fob_secret: [u8; 32],
    grant: Grant,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn counting(from: u8) -> [u8; 32] {
    std::array::from_fn(|i| from.wrapping_add(i as u8))
}

fn main() {
    let inputs = [
        Input {
            name: "same device, node and prices shared",
            app_secret: counting(0x01),
            state: std::array::from_fn(|i| 0xa0 + i as u8),
            app: "Till",
            wallet_name: "Corner Café",
            device: "Corner Café iPad",
            network: Network::Mainnet,
            want: Want { node: true, prices: true },
            callback: Some("https://till.monokulo.com/fob/connected"),
            fob_secret: counting(0x41),
            grant: Grant {
                address: "4AdUndXHHZ6cfufTMvppY6JwXNouMBzSkbLYfpAV5Usx3skxNgYeYTRj5UzqtReoS44qo9mtmXCqY45DJ852K5Jv2684Rge".into(),
                view_key: "f359631075708155cc3d92a32b75a7d02a5dcf27756707b47a2b31b21c389501".into(),
                network: "mainnet".into(),
                account: 13,
                label: "Till (Corner Café)".into(),
                restore_height: 3_512_345,
                wallet_id: "6f1b0c2e9d4a47b8a3e5c7d9f0b1a2c3".into(),
                wallet_name: "Corner Café".into(),
                node: Some(SharedNode { url: "https://node.cornercafe.example:18089".into(), self_signed: true, pin: Some("9f".repeat(32)) }),
                light: None,
                prices: Some(SharedPrices { provider: "Kraken".into(), currency: "AUD".into() }),
            },
        },
        Input {
            name: "two devices by QR code, view only",
            app_secret: counting(0x81),
            state: [0x5a; 16],
            app: "Till",
            wallet_name: "Kiosk",
            device: "iPad",
            network: Network::Stagenet,
            want: Want::default(),
            callback: None,
            fob_secret: counting(0xc1),
            grant: Grant {
                address: "55LTR8KniP4LQGJSPtbYDacR7dz8RBFnsfAKMaMuwUNYX6aQbBcovzDPyrQF9KXF9tVU6Xk3K8no1BywnJX6GvZX8yJsXvt".into(),
                view_key: "0a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e800".into(),
                network: "stagenet".into(),
                account: 1,
                label: "Till (Kiosk)".into(),
                restore_height: 2_045_000,
                wallet_id: "00112233445566778899aabbccddeeff".into(),
                wallet_name: "Kiosk".into(),
                node: None,
                light: None,
                prices: None,
            },
        },
        Input {
            name: "a light wallet server shared, another app",
            app_secret: counting(0x11),
            state: std::array::from_fn(|i| (i as u8).wrapping_mul(17)),
            app: "Shop & Go",
            wallet_name: "",
            device: "Till #2 / back bar",
            network: Network::Mainnet,
            want: Want { node: true, prices: false },
            callback: Some("shopandgo://fob?x=1"),
            fob_secret: counting(0x21),
            grant: Grant {
                address: "4AdUndXHHZ6cfufTMvppY6JwXNouMBzSkbLYfpAV5Usx3skxNgYeYTRj5UzqtReoS44qo9mtmXCqY45DJ852K5Jv2684Rge".into(),
                view_key: "f359631075708155cc3d92a32b75a7d02a5dcf27756707b47a2b31b21c389501".into(),
                network: "mainnet".into(),
                account: 2,
                label: "Shop & Go".into(),
                restore_height: 3_600_000,
                wallet_id: "ffeeddccbbaa99887766554433221100".into(),
                wallet_name: "My wallet".into(),
                node: None,
                light: Some(SharedNode { url: "https://lws.example".into(), self_signed: false, pin: None }),
                prices: None,
            },
        },
    ];
    let cases: Vec<Value> = inputs
        .iter()
        .map(|input| {
            let asker = Asker::with(&input.app_secret, input.state, input.app, input.wallet_name, input.device, input.network, input.want, input.callback).expect("a request");
            let request = asker.request();
            let answer = connect::seal_with(request, &input.grant, &input.fob_secret).expect("sealed");
            let icon = request.identicon();
            let mut want = vec!["view"];
            if input.want.node {
                want.push("node");
            }
            if input.want.prices {
                want.push("prices");
            }
            json!({
                "name": input.name,
                "app_secret": hex(&input.app_secret),
                "state": B64.encode(input.state),
                "app": input.app,
                "wallet_name": input.wallet_name,
                "device": input.device,
                "network": input.network.name(),
                "want": want,
                "callback": input.callback,
                "fob_secret": hex(&input.fob_secret),
                "grant": serde_json::to_value(&input.grant).expect("JSON"),
                "pk": B64.encode(request.pk),
                "request": request.to_uri(),
                "answer": answer,
                "declined": connect::decline(request),
                "identicon": icon.rows(),
                "palette": icon.palette,
            })
        })
        .collect();
    let doc = json!({
        "about": "Connect to Fob, version 1 (fob-codes src/connect.rs). Secrets are hex; state, pk, epk and box are base64url without padding. identicon rows: '#' ink, '.' paper; palette indexes fob_codes::connect::PALETTE.",
        "version": 1,
        "cases": cases,
    });
    println!("{}", serde_json::to_string_pretty(&doc).expect("JSON"));
}
