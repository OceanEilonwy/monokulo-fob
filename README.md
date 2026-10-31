# Fob

A small Monero wallet: fast to catch up, simple to pay with, and it keeps
the [Open Receipt Format](https://github.com/openreceiptformat/orf-spec)
receipts that payment codes can carry (as Monokular's tills add).

- `crates/fob-core`: the whole wallet, in Rust, on
  [monero-oxide](https://github.com/monero-oxide/monero-oxide). Seeds and
  keys, payment and wallet codes, receipts, the encrypted wallet file,
  syncing and sending.
- `crates/fob-ffi`: fob-core for Kotlin (later Swift) through UniFFI. Types
  only.
- `android/`: the Android app, a thin Compose layer over fob-ffi.
- `third_party/`: two vendored seed crates (see its README).
- `docs/decisions.md`: why things are the way they are.

Designs: https://claude.ai/artifact/MK3rLvr8rcGa6WA5CZrXat

## Trying it

`dist/fob-debug.apk` is a debug build. It starts on **stagenet** (test coins,
nothing to lose); Settings › Use mainnet switches to a separate mainnet
wallet. Stagenet coins come from a faucet, for example
https://stagenet-faucet.xmr-tw.org.

```sh
adb install -r dist/fob-debug.apk
```

## Building

You need Rust, `cargo-ndk`, the Android SDK (platform 37.2, NDK r27) and a
JDK 21 (Android Studio's: `/opt/android-studio/jbr`).

```sh
cargo nextest run --workspace       # or cargo test --workspace
cd android && echo "sdk.dir=$HOME/Android/Sdk" > local.properties
JAVA_HOME=/opt/android-studio/jbr ./gradlew :app:assembleDebug
```

The Gradle build compiles fob-ffi for each ABI with cargo-ndk and generates
the Kotlin bindings from it, so the app and the library can't drift.

Tests against real stagenet nodes are ignored by default:

```sh
cargo test --release -p fob-core --test stagenet_over_http -- --ignored
cargo run --release -p fob-core --example fetch_speed [node] [blocks]
```
