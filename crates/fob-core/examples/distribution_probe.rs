//! Prints what a node sends for `get_output_distribution.bin` with
//! `compress` on: `cargo run -p fob-core --example distribution_probe [node]`.

use fob_core::transport::Transport;
use monero_daemon_rpc::HttpTransport;
use monero_epee::{Epee, HEADER, Type, VERSION};

fn key(r: &mut Vec<u8>, name: &str, kind: u8) {
    r.push(name.len() as u8);
    r.extend(name.as_bytes());
    r.push(kind);
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let node = args.get(1).cloned().unwrap_or_else(|| "http://stagenet.xmr-tw.org:38081".into());
    let from: u64 = args.get(2).and_then(|a| a.parse().ok()).unwrap_or(2_000_000);
    let to: u64 = args.get(3).and_then(|a| a.parse().ok()).unwrap_or(2_000_009);
    let cumulative = args.get(4).is_some_and(|a| a == "cumulative");
    let mut r = Vec::new();
    r.extend(HEADER);
    r.push(VERSION);
    r.push(6 << 2);
    key(&mut r, "amounts", Type::Uint64 as u8 | 0x80);
    r.push(1 << 2);
    r.extend(0u64.to_le_bytes());
    key(&mut r, "from_height", Type::Uint64 as u8);
    r.extend(from.to_le_bytes());
    key(&mut r, "to_height", Type::Uint64 as u8);
    r.extend(to.to_le_bytes());
    key(&mut r, "cumulative", Type::Bool as u8);
    r.push(u8::from(cumulative));
    key(&mut r, "binary", Type::Bool as u8);
    r.push(1);
    key(&mut r, "compress", Type::Bool as u8);
    r.push(1);
    let t = Transport::new(&node, false).unwrap();
    let body = t.post("get_output_distribution.bin", r, None).await.unwrap();
    println!("{} bytes", body.len());
    let mut epee = Epee::new(body.as_slice()).unwrap();
    let mut fields = epee.entry().unwrap().fields().unwrap();
    while let Some(f) = fields.next() {
        let (k, v) = f.unwrap();
        let name = String::from_utf8_lossy(k.consume()).into_owned();
        if name != "distributions" {
            if name == "status" { println!("status: {}", String::from_utf8_lossy(v.to_str().unwrap().consume())); } else { println!("{name}: {:?}", v.kind()); }
            continue;
        }
        let mut items = v.iterate().unwrap();
        while let Some(item) = items.next() {
            let mut fs = item.unwrap().fields().unwrap();
            while let Some(f) = fs.next() {
                let (k, v) = f.unwrap();
                let name = String::from_utf8_lossy(k.consume()).into_owned();
                let kind = v.kind();
                let shown = match kind {
                    Type::String => { let b = v.to_str().unwrap().consume(); format!("{} bytes {:02x?}", b.len(), &b[..b.len().min(12)]) }
                    Type::Uint64 => v.to_u64().unwrap().to_string(),
                    Type::Bool => v.to_bool().unwrap().to_string(),
                    _ => String::new(),
                };
                println!("  {name}: {kind:?} {shown}");
            }
        }
    }
}
