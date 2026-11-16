# Fob for Apple: plan

Fob on iPhone, iPad and macOS, from the same Rust core Android uses. Nobody
on the team has an Apple device: the only way to see the app is GitHub
Actions on macOS runners, which build it, run UI tests and hand back
screenshots. So every piece of work is a small sub-goal that a UI test can
prove with a screenshot.

This file belongs to the core team. Other teams append to
`docs/apple/notes/<team>.md` and talk through the coordination database
(`docs/apple/coordination.md`, written by core).

## 1. Architecture

```
crates/fob-wallet ─┐
crates/fob-words  ─┼─> crates/fob-ffi (UniFFI 0.32.2) ──> FobFFI.xcframework (static libs, per slice)
crates/fob-screens ┘          │                              + generated Swift (fob_ffi.swift)
                              │                                        │
crates/fob-scenes ────────────┘ (deterministic sample data)            v
                                                     apple/core: FobCore Swift package
                                                     (bindings, AppModel + view models,
                                                      FobUI theme + shared components)
                                                        │                       │
                                         apple/ios: one iOS app target      apple/macos: macOS app target
                                         (iPhone layouts; apple/ios/Pad/    (one small window, pages
                                          switches to iPad layouts by        pushed in place, like the
                                          size class)                        GTK app)
```

- **Rust core.** `crates/fob-ffi` is already the whole app-facing surface
  (Android uses it; the GTK app reaches it through `crates/fob-app`). It is
  exported with `uniffi::setup_scaffolding!()` and proc-macros only:
  objects `WalletList` and `FobWallet` (create/restore/open, `screens()`,
  receive, send `prepare`/`preview`/`payment`, contacts, requests, sync,
  prices, node, backup), many records (`Screens`, `Balance`, `Activity*`,
  `Coins`, `SyncStatus`, `SplitView`, `Receipt`...), and foreign traits
  (`with_foreign`, lib.rs:1200-1240: the Trezor link/prompts and the sync
  observer). `uniffi.toml` only has `[bindings.kotlin]`; core adds
  `[bindings.swift]` (module name `FobFFI`, ffi module `fob_ffiFFI`).
- **What must change in fob-ffi for Apple** (core): `crate-type` is
  `["cdylib", "lib"]`; Apple needs `staticlib` added (the till's
  `till-ffi` uses `["cdylib", "staticlib"]`). Bindings are generated from a
  host build (`--library target/release/libfob_ffi.dylib`), as Android's
  `uniffiBindings` Gradle task does.
- **crates/fob-scenes** (new, core): deterministic sample wallets for
  screenshots: fixed clock, balance 0.4182 XMR / A$148.20, the contacts,
  activity and copy the mocks show, exported through fob-ffi as
  `scene(name) -> ...` so the Swift app draws its real screens from it with
  no node and no randomness. It plays the role that
  `crates/fob-gtk/src/scene.rs` plays for the README (`FOB_SCENE=home|receive|send|contacts|coins|settings|split|...`)
  and that `crates/till-app/src/preview_setup.rs` plays in the till.
  The scene names should match the GTK ones where a screen exists in both.
- **FobCore** (`apple/core/`): a Swift package with a `binaryTarget`
  `FobFFI` (the xcframework) and a `FobCore` target holding the generated
  Swift, `AppModel` and view models, and `FobUI` (theme tokens, fonts,
  shared components). Linker settings as the till's TillCore:
  `Security`, `SystemConfiguration`, `CoreFoundation`, `Foundation`.
  Neither the xcframework nor the generated Swift is committed (built
  every time, like Android's bindings).
- **Apps** (`apple/project.yml`, XcodeGen, no committed .xcodeproj):
  `Fob` (iOS, `TARGETED_DEVICE_FAMILY: "1,2"`), `FobMac` (macOS), and UI
  test bundles `FobUITestsPhone`, `FobUITestsPad`, `FobUITestsMac` with one
  scheme per platform so CI can run each alone.
- **Launch arguments.** `-FobScene <scene>` loads a fob-scenes wallet
  instead of the real store; `-FobTheme <oled|light>` forces the theme.
  Both are read only by the app's scene loader; absent, the app runs
  normally.

### Theme

From `android/app/src/main/kotlin/com/monokulo/fob/ui/Theme.kt`
(`FobColors` roles: paper, card, sunken, onPaper, line, ink, muted,
outline, orangeText, green, greenDot, greenFill, onGreen, highlight,
warning, selectedFill, addressMark, onAddressMark, red, onRed, track,
scrim, inverse*, plus `orange = #FF6600`):

| role | OLED (default drawn) | Light |
|---|---|---|
| paper | #000000 | #F4F1EA |
| card | #0C0B0A | #FFFFFF |
| line | #2A2723 (1px hairline) | #E2DCCF |
| ink | #F2EEE6 | #1A1917 |
| muted | #B0A999 | #5A564C |
| orangeText | #FF8F45 | #B34700 |
| tint | #FF6600 | #FF6600 |

FobUI mirrors every role by name (`OledColors`, `LightColors`, and
`DarkColors` later). Manrope ExtraBold (`android/app/src/main/res/font/manrope.ttf`,
reuse it) for balances and titles; SF Pro with Dynamic Type for UI text.
All words come from `crates/fob-words` through fob-ffi: no copy is written
in Swift beyond what the mocks show as platform chrome.

## 2. Folder ownership

Each team edits only its folders; anything else is a request to the owner
through the coordination database.

| team | worktree / branch | owns |
|---|---|---|
| core | `.claude/worktrees/apple-core` on `apple/core` | `apple/core/**` (FobCore package: UniFFI bindings + xcframework build, AppModel/view models, FobUI theme + shared components), `crates/fob-ffi/**` additions for Apple, `crates/fob-scenes/**`, `apple/project.yml` (device teams request target changes), `docs/apple/plan.md`, `docs/apple/coordination.md` |
| iphone | `.claude/worktrees/apple-iphone` on `apple/iphone` | `apple/ios/**` except `apple/ios/Pad/**`, `apple/ui-tests/iphone/**` |
| ipad | `.claude/worktrees/apple-ipad` on `apple/ipad` | `apple/ios/Pad/**` (iPad-only layouts built on iPhone's screens; size-class switches live here), `apple/ui-tests/ipad/**` |
| macos | `.claude/worktrees/apple-macos` on `apple/macos` | `apple/macos/**`, `apple/ui-tests/macos/**` |
| ci | `.claude/worktrees/apple-ci` on `apple/ci`; builds `apple/integration` | `.github/workflows/apple.yml`, `apple/ci/**` |
| everyone | | may append to `docs/apple/notes/<team>.md` |

Rules: never push to or modify `main`; never touch `android/`,
`.github/workflows/{rust,android,release}.yml` or release tooling. Start
each shift with `git fetch origin && git merge origin/apple/main`. Create a
missing worktree with
`git worktree add .claude/worktrees/apple-<team> -B apple/<team> origin/apple/main`.
Commits end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## 3. Design source of truth

`docs/apple/design/*.dc.html` (see `design/README.md`), copied from the
Fob for Apple canvas. Key rules: OLED is the default drawn (true black
page, cards #0C0B0A held by 1px #2A2723 hairlines, orange #FF6600 tint,
orange text #FF8F45); Light mirrors Android's Light; iPhone has a tab bar
Wallet / Contacts / Settings with Receive and Send as full-screen pages;
iPad is ONE screen (balance left, activity right) and Send is a form sheet
that pushes "Pay who?"; macOS is ONE small window like the GTK app with
pages pushed in place; payments are confirmed with Face ID / Touch ID.

## 4. CI design (`.github/workflows/apple.yml`, implemented by the scaffold/ci team)

Triggers: `push` to `apple/integration` and `apple/main`, and
`workflow_dispatch`. `concurrency: apple-${{ github.ref }}` with
cancel-in-progress. `permissions: contents: read`.

Jobs are **independent** so one platform failing never blocks another: no
`needs:` between them, no shared prerequisite job, no matrix fail-fast.

| job | runner | builds | runs |
|---|---|---|---|
| `check` | `ubuntu-latest` | host | `cargo check` + `cargo test -p fob-ffi -p fob-scenes --locked` |
| `iphone` | `xcode-27` | core slice `aarch64-apple-ios-sim` | `xcodebuild test -scheme FobUITestsPhone` on an iPhone simulator |
| `ipad` | `xcode-27` | core slice `aarch64-apple-ios-sim` | `xcodebuild test -scheme FobUITestsPad` on an iPad simulator |
| `macos` | `xcode-27` | core slice `aarch64-apple-darwin` | `xcodebuild test -scheme FobUITestsMac -destination 'platform=macOS'` |

Each macOS job:

1. `actions/checkout`; select the newest `/Applications/Xcode_27*.app`
   and fail with `::error::` if `xcodebuild -version` is not Xcode 27 (as
   the till's ios.yml does).
2. `rustup target add` only the slice it needs.
3. sccache: `mozilla-actions/sccache-action`, env
   `SCCACHE_GHA_ENABLED=true`, `RUSTC_WRAPPER=sccache`, with a per-target
   cache key (`sccache-<target>-<deployment target>`), so iphone and ipad
   share the sim slice's objects and macos keeps its own.
4. `actions/cache` for SwiftPM (`~/Library/Caches/org.swift.swiftpm` and
   `apple/core/.build`) and DerivedData, keyed on the Xcode version +
   `hashFiles('apple/project.yml', '**/Package.resolved')`.
5. `brew install xcodegen`; `apple/ci/build-core.sh <slice>` (the
   xcframework with just that slice + Swift bindings); `xcodegen generate`
   in `apple/`.
6. Pick the simulator by JSON (`xcrun simctl list devices available -j`),
   as the till does: the newest iPhone on the newest iOS 27 runtime; an
   iPad on the same (or the oldest supported 26.x).
7. `xcodebuild test` for that platform's UI-test scheme only, with
   `-resultBundlePath $RUNNER_TEMP/<platform>.xcresult` and
   `CODE_SIGNING_ALLOWED=NO`. Retry once only when the log shows a
   simulator failure and no test failure (the till's grep list: "hung
   before establishing connection|never began executing|lost connection to
   the test|Early unexpected exit|Unable to boot|failed to boot").
8. `if: always()`: `apple/ci/extract-screenshots.sh <xcresult> <outdir>`,
   then upload artifacts `shots-<platform>` (PNGs) and
   `xcresult-<platform>` (14 days), and append a job summary (count of
   PNGs, link to the artifact via `steps.<id>.outputs.artifact-url`, the
   failed tests).

Expect each macOS job to take about 20-40 minutes (the till: core ~7.5
min cold-ish with rust-cache, tests ~28 min on two simulators). macOS
minutes cost about ten times Linux's, so keep test counts small and
scenes cheap.

## 5. Screenshot conventions

- A UI test that proves a sub-goal launches the app with
  `app.launchArguments = ["-FobScene", "<scene>", "-FobTheme", "<oled|light>"]`,
  waits for a known element, takes `XCUIScreen.main.screenshot()`, and adds
  an `XCTAttachment` with `lifetime = .keepAlways` and
  `name = "<task_id>--<theme>"` (task id from the coordination database).
- `apple/ci/extract-screenshots.sh` exports the xcresult's attachments
  (`xcrun xcresulttool export attachments --path <xcresult> --output-path <dir>`
  on Xcode 16+, which writes a `manifest.json` mapping each exported file to
  its attachment's suggested name) and renames them to
  `<task_id>--<theme>.png`, in `shots-<platform>/`.
- Scenes are deterministic: fixed clock, no network, data from
  fob-scenes. Status bar: `xcrun simctl status_bar <udid> override --time 9:41`
  before the tests on iOS.

## 6. What the till (monokulo-till, OceanEilonwy/monokular) does

References are to `origin/main` of `/home/henry/Downloads/monokular`.

- **Workflow** `.github/workflows/ios.yml`: one job `test` on
  `runs-on: xcode-27` (GitHub-hosted macOS 27 arm64; `macos-26` tops out at
  Xcode 26.6), `timeout-minutes: 75`. Steps: select newest Xcode 27 or
  fail; `rustup target add aarch64-apple-ios-sim aarch64-apple-ios`;
  `Swatinem/rust-cache@v2` with `shared-key: ios-<IPHONEOS_DEPLOYMENT_TARGET>`
  (read with sed from build-core.sh); `brew install xcodegen`;
  `ios/Scripts/build-core.sh --sim`; `xcodegen generate`; pick simulators
  with an inline Python over `simctl list devices available -j` (newest
  iPhone on iOS 27; an iPad on the oldest iOS >= 26.7, falling back to 27
  with a warning); one `xcodebuild test` with both `-destination id=...`;
  upload `till-ios-test-results` (xcresult) and `till-ios-screenshots`;
  summary with `artifact-url`. Path filters on `ios/**`, `crates/**`,
  `Cargo.*`.
- **Run times** (ios.yml on main, run 43549435073, 2026-11-14): 37.5 min
  total; core build 7.4 min; tests 27.8 min; other runs 23-34 min.
- **Core build** `ios/Scripts/build-core.sh`: `IPHONEOS_DEPLOYMENT_TARGET=26.7`,
  wipes the iOS target dirs when it changes (stamp file); `cargo build
  --locked --release -p till-ffi --target <each>`; a host release build
  for bindgen; `uniffi-bindgen generate --library libtill_ffi.dylib
  --language swift`; copies `*.swift` into the package's `Generated/`,
  `*.h` into headers and concatenates `*.modulemap` into one
  `module.modulemap`; `xcodebuild -create-xcframework -library ... -headers ...`.
- **Package** `ios/Packages/TillCore/Package.swift`: swift-tools 6.2,
  `binaryTarget` + target with system framework linker settings.
- **Project** `ios/project.yml`: XcodeGen, `deploymentTarget iOS 26.7`,
  Swift 6 with complete strict concurrency, local package dependency,
  `TARGETED_DEVICE_FAMILY "1,2"`, generated Info.plist, unit-test bundle
  hosted in the app, scheme `Till` testing `TillTests`.
- **Screenshots** (PR #73 "Screenshots of every key screen on iPhone, iPad
  and Android, side by side", branch `ios-pr-screenshots`, merged
  2026-11-12): NOT XCUITest. `ios/TillTests/ScreenshotTests.swift` is a
  hosted unit test (Swift Testing, `.serialized`, enabled only when
  `SNAPSHOT_DIR` is set; CI passes `TEST_RUNNER_SNAPSHOT_DIR`, which
  xcodebuild forwards to the test process without the prefix). It builds
  each screen from the engine's previews (`crates/till-app/src/preview_setup.rs`:
  `previewShift`, `previewSale`, `previewOnboarding`, `previewSettings`)
  through `Core(showing:)` (a Core with no engine), puts it in a fresh
  `UIWindow` with `overrideUserInterfaceStyle`, waits 2 s, renders with
  `drawHierarchy(afterScreenUpdates:)` and writes
  `<name>-light.png`/`-dark.png` straight to disk under `iphone/` or
  `ipad/`, plus `shots.txt` and `device.txt`. CI then indexes them with
  `tools/screenshot-compare` and uploads the folder. `.github/workflows/screenshots.yml`
  fetches the iOS artifact (`tools/screenshot-compare/fetch-ios.sh`) to set
  it beside Android's.
- **Flakiness**: the till hit "test runner hung before establishing
  connection" on main; ios.yml retries once when the log matches a
  simulator failure and no test failure.

What Fob does differently and why: Fob uses XCUITest +
`XCUIScreen.main.screenshot()` + xcresult attachments (one process per
scene, real launch path, works for macOS too, where `UIWindow` tricks do
not), so screenshot extraction goes through `xcresulttool` instead of a
shared directory. Fob splits iPhone, iPad and macOS into independent jobs
(the till runs both simulators in one job), and uses sccache instead of
rust-cache so per-target caches do not evict each other.

## 7. Risks

- `fob-ffi` has no `staticlib` crate type yet; `fob-wallet` uses rusqlite
  `bundled-sqlcipher-vendored-openssl`, which must cross-compile OpenSSL for
  `aarch64-apple-ios-sim` (needs the iOS SDK's clang; slow cold build: cache
  it).
- `fob-scanner` depends on the git crate `monokulo-8ar`; CI must be able
  to fetch it (it does on rust.yml today).
- XCUITest on macOS runners needs the test runner to be allowed
  accessibility/automation; unsigned (`CODE_SIGNING_ALLOWED=NO`) macOS UI
  tests may need ad-hoc signing (`CODE_SIGN_IDENTITY=-`) instead.
- Face ID / Touch ID cannot be exercised on CI: the app needs a scene-mode
  stub (`-FobScene` skips LocalAuthentication, or the simulator's
  `simctl` biometric enrolment is used on iOS only).
- `xcresulttool export attachments` exists from Xcode 16; older
  `get --legacy` paths must not be relied on.
- macOS runner minutes cost ~10x Linux: three macOS jobs per push of
  ~20-40 min each.
