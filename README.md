# Fob

A small Monero wallet: fast to catch up, simple to pay with, and it keeps
the [Open Receipt Format](https://github.com/openreceiptformat/orf-spec)
receipts that payment codes can carry (as Monokular's tills add).

- `crates/fob-wallet`: the whole wallet, in Rust, on
  [monero-oxide](https://github.com/monero-oxide/monero-oxide). Seeds and
  keys, payment and wallet codes, receipts, the encrypted wallet file,
  syncing and sending.
- `crates/fob-screens`, `fob-codes`, `fob-vault`, `fob-price`, `fob-chain`,
  `fob-scanner`, `fob-monero`, `fob-words`: the parts `fob-wallet` is built from
  (`docs/decisions.md`, decision 339).
- `crates/fob-ffi`: fob-wallet for Kotlin (later Swift) through UniFFI. Types
  only.
- `crates/fob-app`: what a client draws: unlocking, the open wallet, syncing.
- `crates/fob-gtk`: the Linux desktop app, in Relm4 and libadwaita.
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

Every `v*` tag publishes signed APKs on the
[Releases](https://github.com/OceanEilonwy/monokulo-fob/releases) page, with a
`SHA256SUMS` file, so [Obtainium](https://obtainium.imranr.dev/) can install Fob
and keep it up to date from there, with no store:

- `fob-<version>-arm64-v8a.apk`: for phones, about half the size of the
  universal one. This is the one most people want.
- `fob-<version>-x86_64.apk`: for the emulator and x86 Chromebooks.
- `fob-<version>-universal.apk`: both in one, for when you don't know.

In Obtainium, tap **Add App**, paste
`https://github.com/OceanEilonwy/monokulo-fob` as the source, and add. Or open
this link on the phone: [obtainium://add/https://github.com/OceanEilonwy/monokulo-fob](obtainium://add/https://github.com/OceanEilonwy/monokulo-fob).
With three APKs on a release, Obtainium asks which; set the APK filter to
`arm64-v8a` in the app's settings to pick the phone one every time. The tag
(`v0.1.4`) is the version, as the app's own is. Every release is signed with the
same key and the APKs share a version code, so any installs over another
(package `com.monokulo.fob`). The debug build above is a different package and
sits beside it.

## On the desktop

`crates/fob-gtk/install.sh` builds Fob and adds it to your menu; it needs Rust,
GTK 4.18 and libadwaita 1.7 or newer. It starts on stagenet, like the debug
Android build. Your wallets are sealed under a passphrase you choose, in
`~/.local/share/fob`, and lock after 15 minutes left alone. A camera isn't used:
a code is pasted or read from a picture. `crates/fob-gtk/scenes.sh` checks the
windows on a headless display, and its `walk` scene presses through the app.

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
cargo test --release -p fob-wallet --test stagenet_over_http -- --ignored
cargo run --release -p fob-wallet --example fetch_speed [node] [blocks]
```
