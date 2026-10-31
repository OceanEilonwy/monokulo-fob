# Round 7 plan

The work agreed after the round 3 build went on the phone, in build order.
Designs are on the Fob canvas (https://claude.ai/artifact/MK3rLvr8rcGa6WA5CZrXat),
page **Round 7**, named here by board number and title. Every step keeps
wallet logic in fob-core; the Android app only draws, runs OS services and
calls Rust.

## Decisions taken before building

1. **Sync chip A**: a ring for progress and the time left ("Starting", "6 min left", "20 s left"), then "Synced" or "Can't reach node". The bar under Home's balance goes. Time left shows once the speed has settled (about 3 s), is rounded, never counts up; a catch-up of a few seconds shows only Starting, then Synced.
2. **Tapping the chip** opens a details card: percent, a bar, time left, payments up to, blocks and blocks per second, link speed in MB/s (bytes fetched from the node over recent seconds, measured in Rust's fetcher), and a link to Node.
3. **Send sheet**: pulled up, it is exactly as tall as its form (Review straight under the ready line); the camera keeps the rest and its frame centres there. The sheet follows the finger while dragging and settles to the nearer stop (or the fling's way) with a 300 ms ease; tapping the handle makes the same move.
4. **Home**: your photo (or initial, or a person outline before there's a profile) in the top bar before Settings opens Your profile; a Contacts chip beside Coins on the Activity card opens Contacts. No People card.
5. **Backup and keys**: one Settings row replaces Back up seed phrase and Show wallet code. Its screen: Seed phrase and Wallet code under "Back up", Keys under "For other wallets". Keys shows every key in full at once with a Copy button each: secret spend and view keys first (each saying what it lets someone do), then main address, restore height, public view and public spend keys. Opening Backup and keys asks for the fingerprint (device credential) and blocks screenshots; a watch-only wallet has no secret spend key row.
6. **Money**: no on/off switch. "Also show amounts in" opens a full-screen list: search, XMR only (Fob never looks up a price), this phone's currency, then all currencies A–Z, each row saying which providers price it. With XMR only the card shows just that row. Display and Prices merge into one Money card: Also show amounts in, Show first (segmented, as before), Price from.
7. **Currencies and providers** are separate tables baked into the app, with a currency → providers mapping. 45 currencies (code, name, symbol made unique: A$, C$, NZ$, HK$, S$…; decimals: 0 for JPY, KRW, VND, CLP and the like). Kraken prices 8: USD and EUR directly; GBP, AUD, CAD, CHF, JPY, AED as XMR/USD times Kraken's own USD exchange pair (two requests; history the same way). CoinGecko (keyless public API) prices all 45 directly, history from its daily market chart. Each provider says whether it supports web and Tor (until Tor is built: Kraken web only, CoinGecko web and Tor, to be confirmed when Tor exists).
8. **Price from**: one line per provider (radio, name, globe icon for web and onion icon for Tor, current rate on the right). Opening it checks every provider for the chosen currency; one that fails is greyed with "Not answering" and can't be picked. "Ask over" Web / Tor; Tor is greyed with "<provider> doesn't answer over Tor" when the chosen provider lacks it. Picking a currency your provider can't price moves you to one that can; picking a provider without Tor while on Tor moves you to Web; both say so.
9. **Price failures** show one toast, the same on every screen: rising from the bottom above the gesture bar and any bottom button, saying what failed and what Fob uses instead, with Retry and a cog. Retry shows "Trying again…", then "<CUR> price updated" for 2 s or the failure again. The cog opens Price from; Back returns to where you were. It stays until swiped, retried, or a later lookup works. The 5-minute background refresh shows it once per run of failures; anything the user did (pull, changing currency or provider, typing a money amount) always shows it. Amounts keep the last price with its age; with no price, or one over a day old, money entry on Send and Receive is off. Settings' Price from row turns red with the reason.
10. **Node**: a Working card on top (verdict: Working, Checking…, or the step that failed; the node's address with no network; four checks with results: Answers (latency), Right network, Up to date (height, age), Sends blocks to Fob (blocks/s · MB/s)). A refresh icon at the top right of the header runs the checks again and spins; no "Checked at" line. Below, "Change node": other nodes with a dot, their result in words and Use on the ones that pass; "Add your own node" opens a sheet: address, "Accept self-signed certificates" checkbox (accepts any certificate from that node, still encrypted), the same checks (a Certificate check first for https), Check again and Use / Use it anyway (when only Up to date fails). No long explanatory text.
11. **Profile row** (Rust's settings_row): with no name, only "Add a shareable Fob contact" (a photo alone doesn't count); with a name, "Your profile" over "<name> · share as a code", the name cut with an ellipsis so "· share as a code" always shows.

## Steps

1. **Currencies and providers (Rust)**: decisions 6–8 data side. Currency table, provider table and mapping; settings model "also show amounts in" (none or a currency) replacing on/off, old wallet files migrated; Kraken through exchange pairs; CoinGecko provider; provider health check with web/Tor support; moving provider/transport when they don't fit. Boards 9, 13, 14.
2. **Price failures**: decision 9. Rust failure state (background once per run, always for user actions), retry; Android toast host on every screen, cog to Price from and back. Boards 15–20.
3. **Settings, Money, currency list, Price from, profile row (Android + Rust words)**: decisions 5 (Settings row only), 6, 8, 11. Boards 6, 9, 10, 13, 14.
4. **Sync chip and details**: decisions 1–2, link speed in Rust. Boards 1, 2.
5. **Send sheet**: decision 3. Boards 3, 4.
6. **Home people**: decision 4. Board 5.
7. **Backup and keys**: decision 5. Boards 7, 8.
8. **Node**: decision 10. Boards 11, 12.

Each step is committed on its own with unit tests where they apply, and adds
what to check on the device to `docs/round-7-checklist.md`, which is walked
through on an emulator once every step is in. Decisions made while building
go on in `docs/decisions.md`.
