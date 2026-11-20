# Core notes

## FobUI components (round 3: CORE-021, CORE-022, CORE-030)

Additive API; nothing existing changed shape.

- `PillButton(title, systemImage:, style: .primary|.secondary|.tinted, size: .regular|.large|.compact|.small) { }`
  and `.buttonStyle(.fobPill(style, size:))` for any Button. Pressed and disabled come from the style.
  `ReceiveSendButtons(labels: model.labels.home, layout: .phone|.mac, receive:, send:)` is Home's pair
  (ids `receive-button`, `send-button`).
- `SyncChipView(model.syncChip, label: model.state.syncLabel, size: .regular|.small) { open card }` (id `sync-chip`),
  `StagenetChip(model.labels.home.stagenet)` (id `stagenet-chip`),
  `WalletChip(model.walletName, accessibilityName: labels.chrome.switchWallet, layout: .phone|.mac) { }` (id `wallet-chip`),
  `FobMark()` (the logo). `SyncChip(SyncStatus)` is now public; `SyncChip.sceneStates` gives one of each look.
- Chrome: `.fobTabBar()` on the TabView (selected tab in orangeText, as Main draws it), `.fobBars()` on each page
  (nav bar on the page's paper, tab bar on paper), `.fobBackButton("Wallet")` (orange "‹ Wallet", id `back`),
  `AppTab.symbol`, `AppTab.title(labels.tabs)`, `AppTab.accessibilityID` ("tab-wallet"),
  `FobIconButton(.scan|.contacts|.refresh|.settings, labels: model.labels.chrome, look: .circle|.bar) { }`
  (ids `toolbar-<icon>`). In iOS 26 toolbars, add `.sharedBackgroundVisibility(.hidden)` to the ToolbarItem.
- New words: `labels.chrome` (scanAPaymentCode, switchWallet, sections, contacts, refresh, settings, wallets) and
  `labelSyncChip(label:)` ("Synced, tap for details").
- Galleries: `-FobScene gallery-pills`, `gallery-chips`, `gallery-chrome`.
