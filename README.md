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

### Installing and updating with Obtainium

Every `v*` tag publishes a signed APK (`fob-<version>.apk`, with a `SHA256SUMS`
file) on the [Releases](https://github.com/OceanEilonwy/monokulo-fob/releases)
page, so [Obtainium](https://obtainium.imranr.dev/) can install Fob and keep it
up to date from there, with no store.

In Obtainium, tap **Add App**, paste
`https://github.com/OceanEilonwy/monokulo-fob` as the source, and add. Or open
this link on the phone: [obtainium://add/https://github.com/OceanEilonwy/monokulo-fob](obtainium://add/https://github.com/OceanEilonwy/monokulo-fob).
Its defaults suit this repo: the one APK on each release is picked, and the
tag (`v0.1.4`) is the version, as the app's own is. Every release is signed
with the same key, so an update installs over the last (package
`com.monokulo.fob`). The debug build above is a different package and sits
beside it.

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
