# macOS notes

## Window shell (MAC-001)

- `apple/macos/FobMacApp.swift`: one `Window` ("main", 480 x 760 default, 420-560 wide, unified title bar with
  no title) and a `Settings` scene (`MacSettingsWindow`, 760 x 560), both inside `FobRoot`.
- `MacWindow`: `NavigationStack` over `MacWalletPage`, bound to `AppModel.tab`/`path`: the Mac has no tab bar,
  so the Contacts tab shows as a Contacts page pushed on Wallet; back to Wallet selects the Wallet tab.
  Title bar: `toolbar-contacts` (pushes Contacts), `wallet-chip` (principal), `toolbar-refresh`,
  `toolbar-settings` (opens the Settings window). Window id `mac-window`.
- `MacPage`: a pushed page; its title bar has only `MacBackButton` (id `back`, Command-[). Pages without their
  own task yet are the window's paper, id `mac-page-<route>`.
- Refresh and the wallet menu have no action yet (no refresh or wallet list in AppModel).
