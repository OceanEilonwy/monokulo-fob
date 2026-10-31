# Round 7 device checklist

Added to by each step of `round-7-plan.md`, walked through on an emulator once
every step is in. Each item: what to do, what should happen.

## Step 1 · Currencies and providers

The data side only; most of it shows once step 3's screens are in, some
through step 1's stopgap dialogs in Settings.

- [x] Install over a wallet made before this build with Show prices off → Settings says "Also show amounts in · XMR only"; no money anywhere; no price request (Fob idle, no "Price from" row)
- [x] Same with Show prices on in euros → "Euro", amounts in €, Price from Kraken; a payment priced before still shows its € value (no payment priced before in that wallet to see)
- [x] Also show amounts in → the list: XMR only first, this phone's currency (set the emulator to en-AU: Australian dollar), then all 45 A–Z, each with its providers ("AUD · Kraken, CoinGecko"), the chosen one ticked
- [x] Phone set to a currency Fob hasn't (fr-SN, CFA franc) → no "this phone's" group
- [x] Choose Australian dollar on Kraken → stays on Kraken; Home's balance in A$ within a few percent of CoinGecko's (Price from shows both rates)
- [x] Choose New Zealand dollar on Kraken → "Kraken doesn't price New Zealand dollars, so Fob now asks CoinGecko"; Price from CoinGecko; amounts in NZ$
- [x] Choose UAE dirham → CoinGecko only (Kraken's USD/AED never trades), amounts "… AED"
- [x] Choose Japanese yen → amounts with no decimals ("¥93,477"); typing "1500.5" yen on Send says "That isn't an amount"; Receive with ¥1500 asks a sensible XMR amount (typed on Receive, which shares the field)
- [x] Choose Kuwaiti dinar → three decimals ("56.123 KWD")
- [x] Price from with AUD → both providers with a rate (A$…); turn networking off and open it again → both "Not answering", greyed, can't be picked
- [x] Price from many times in a minute on CoinGecko → at some point "Busy, try again soon", not a raw error
- [x] Back to US dollar from NZD → stays on CoinGecko; Kraken pickable again; payments priced in dollars earlier show their $ again
- [x] XMR only → Home, Send, Receive show XMR only; no Show first, no Price from (Home and Send seen; Receive uses the same field)
- [x] Search (step 3): "zloty", "ZŁOTY", "nzd", "A$", "¥" (yuan and yen) find what they say; "qqq" says "No currency matches “qqq”" ("ZŁOTY" and "¥" can't be typed through adb; Rust's search test has them)
- [x] Long names (New Taiwan dollar, Israeli new shekel, South African rand) fit the rows; TalkBack reads each row's name and providers, and each Price from line ("Kraken, web only, A$845.10") (from the accessibility tree; New Taiwan dollar fits)
- [x] Light and dark mode: list, Price from and the move toast readable in both

## Step 2 · Price failures

- [x] Airplane mode, open Fob on Home → within a few seconds the toast rises above the gesture bar: warning mark, "Couldn't get the USD price", "Kraken didn't answer · using the 14:02 price" (or "Kraken didn't answer · no USD price yet" on a fresh install, step 3), Retry, cog (once the last price is 5 minutes old; a fresh one isn't asked again)
- [x] Same, with the toast up, open Send and pull the sheet up → the toast sits above Review, not over it; fold the sheet → it drops to the bottom
- [x] Receive → above Copy and Share; tap the amount field (keyboard up) → stays above the buttons and keyboard — fixed in 6cf5e71: above the keyboard it covered the amount being typed, so it now steps aside while the keyboard is up
- [x] A payment's screen, Coins (above Split up), Settings, Contacts → the same toast in the same place (Coins, Settings, Contacts; no payment to open)
- [x] Swipe it down → gone; swipe another sideways → gone; a short swipe → springs back
- [x] Swiped away, stay in airplane mode for 10 minutes on Home → it doesn't come back (once per run) (10 minutes on Home after swiping it, two background lookups, nothing)
- [x] Then pull to refresh → it shows again at once
- [x] Retry, still offline → spinner, "Trying again…", "Asking Kraken for the USD price", then the failure again — fixed in 6cf5e71: offline the failure came back within a frame, so "Trying again…" now shows for at least 0.8 s
- [x] Retry after turning airplane mode off → "USD price updated", "1 XMR = $…" with a green tick, gone after 2 s
- [x] Toast up, turn networking back on and wait for the 5-minute refresh (or pull) → the toast goes by itself
- [x] Cog from Send → Price from opens; Back → Send as it was (To and amount kept); same from Home and Receive — fixed in 6cf5e71: Back from the cog found Send and Receive empty; their forms are now kept
- [x] Settings while failing → Price from row's line red with the toast's words (step 3): "Kraken didn't answer · using the 14:02 price", and Try again; it turns grey again once a lookup works
- [x] Change currency (Also show amounts in) while offline → toast shows for the new currency at once, even if one was swiped away
- [x] Send in money with a price over 5 minutes old, offline → typing an amount shows the toast; the field's line ends "price from 14:02" once the price is 10 minutes old (the toast shows once the keyboard goes, decision 240)
- [~] Price over a day old (set the phone's clock a day ahead, offline) → no chip on Send or Receive amounts, line "No USD price now · type the amount in XMR"; Home shows XMR only (needs the phone's clock a day ahead, which the emulator won't take without root; the same "No EUR price now · type the amount in XMR" was seen for a currency with no price yet)
- [~] A payment from a few days ago with no price, history failing → "Couldn't get 29 Oct's USD price", "Kraken didn't answer · shown as not available" (needs a funded wallet with an old payment)
- [x] CoinGecko busy (Price from opened many times) → toast says "CoinGecko is busy · …"
- [x] Dark mode → the toast is a light card with dark words, orange Retry readable, green tick on "updated"; light mode → dark card, light words
- [x] TalkBack → toast read as it appears ("Couldn't get the USD price, Kraken didn't answer…") without moving focus; Retry and "Price settings" reachable, each 48 dp; actions menu offers Dismiss (read from the accessibility tree: polite live region, Dismiss action, Retry and the cog 48 dp; TalkBack itself not run)
- [x] Large font (200%) → title and line wrap inside the card; Retry and the cog stay on screen
- [x] Landscape → the toast stays inside the safe area, clear of a cutout

## Step 3 · Settings and money screens

- [x] Settings, light and dark → Wallet (profile row, Contacts, Backup and keys, each with a round icon), Display (Theme only), Money, Syncing, Network, Receipts, This phone; no Prices card
- [x] New wallet with no profile → profile row reads only "Add a shareable Fob contact" with a person outline, no second line; set only a photo (if the editor allows) → still that (the editor won't save a photo without a name)
- [x] Profile named "Henry" → "Your profile" over "Henry · share as a code" with the photo; rename to "Henrietta Montgomery-Fairweather" → "Henrietta Montg… · share as a code" (suffix whole); same at 200% font
- [x] Backup and keys → a screen with Seed phrase and Wallet code; each opens as before (seed words, wallet code QR, screenshots blocked there); on a watch-only wallet only Wallet code, and the Settings line says "Wallet code" (now step 7's hub)
- [x] Money with US dollar → "Also show amounts in · Uses today's XMR price · US dollar ›", Show first XMR | USD, "Price from · Kraken · over the internet ›" — fixed in 7dcc039: the row said "Kraken · over the internet"; now "Kraken · web"
- [x] XMR only → the Money card has only "Also show amounts in · Fob never looks up a price · XMR only"
- [x] Also show amounts in → full-screen list: back arrow, title, search pill "Search: name, code or symbol", NONE (ɱ XMR only), THIS PHONE’S, ALL 45; the chosen one ticked; signs in circles (A$, CHF smaller)
- [x] Type "zloty" → "1 OF 45", Polish złoty; clear button empties it; "qqq" → "No currency matches “qqq”"; keyboard's Search key closes the keyboard; the list scrolls above the keyboard
- [x] Choose New Zealand dollar while on Kraken → back to Settings, a dark notice above the bottom: "Kraken doesn't price New Zealand dollars, so Fob now asks CoinGecko", gone after about 4 s (tap dismisses it sooner); no system toast
- [x] TalkBack on the list → search field "Search currencies"; group titles as headings; each row "Australian dollar, AUD · Kraken, CoinGecko", selected or not, the sign not read (from the accessibility tree: name and providers per row, the sign not read)
- [x] Long names (New Taiwan dollar, South African rand, Israeli new shekel) fit their rows; in Settings the value wraps rather than pushing the title
- [x] Price from → "Prices in **US dollars**, checked as this page opens. Tap a provider to pick it."; a spinner row "Asking each provider…", then Kraken (globe) and CoinGecko (globe, onion) on one line each with rates on the right
- [x] Pick CoinGecko → stays on Price from, its radio filled; Back → Settings says "CoinGecko · over the internet" — fixed in 7dcc039: "CoinGecko · web"
- [x] Ask over → Web "The provider sees this phone's IP address" chosen; Tor greyed: with Kraken "Kraken doesn't answer over Tor", with CoinGecko "Hides this phone's IP address · not built into Fob yet"; tapping Tor does nothing
- [x] Airplane mode, open Price from → both lines greyed, "Not answering" in red, can't be picked; TalkBack reads "Kraken, web only, not answering"
- [x] Airplane mode on a fresh wallet → toast "Couldn't get the USD price", "Kraken didn't answer · no USD price yet"; Settings' Price from line the same in red with Try again (seen as "no EUR price yet" on a currency never priced)
- [x] Swipe the toast away, then Try again in Settings → the toast comes back saying "Trying again…", then the failure again (offline) or "USD price updated" (online), and the row turns grey
- [x] With a price, stay offline 10+ minutes → Home's money reads "$412.06 · 12 min old" at the line's end; with money first, the big "$412.06" with "12 min old" small beside it; after an hour "1 h old"; Send's ready line "… ready to spend · $308.70 · 1 h old" ("A$0.00 · 10 min old" seen; the hour-old reading wasn't waited for)
- [~] Coins › Split up in Japanese yen → notes ¥1,000 to ¥50,000; in Vietnamese dong ₫100,000 to ₫5,000,000, each label fitting its tile; in Kuwaiti dinar 1 KWD to 50 KWD; in US dollars $5 to $200 as before (needs coins to split)
- [x] Dark mode → Price from's radios, badges and red failures readable; the notice light on dark

## Step 4 · Sync chip

- [x] Restore a wallet from far back → chip "Starting" with an empty ring for about 3 s, then "N min left" with the ring filling; no bar or "Up to …" line under the balance; "Balance so far" still above it
- [x] Watch the chip for a minute while catching up → the time only goes down ("7 min left", "6 min left"…), never back up for a small wobble; near the end "20 s left", then a green dot and "Synced" (it rose twice, 2 → 3 min and, from block 1, 52 min → 2 h 25 min, each when the pace fell by a third or more, as decision 205 allows; never for a small wobble)
- [x] Open Fob a few blocks behind → "Starting", then "Synced", no time shown
- [x] Tap the chip while catching up → a card under it at the right edge: "Catching up" with the percent, an orange bar, Time left "about 4 min", Payments up to "29 Oct, 14:20", Blocks "3,360 of 9,880 · 28/s", Link speed "1.8 MB/s", "You can still receive and send while Fob catches up.", Node settings
- [x] Leave the card open → its numbers move with the chip's
- [x] Tap outside the card, press Back, or tap the chip again → it closes (the chip doesn't reopen it)
- [x] Node settings on the card → the card closes and Node opens; Back → Home with the card closed
- [x] Synced, tap the chip → "Synced" card with Payments up to ("Today, 14:20") and Latest block, no bar
- [x] Airplane mode → red dot "Can't reach node"; card titled "Can't reach node" with "Fob keeps trying on its own." and the error in red; no error line under the balance — fixed in 53e35dc: the red line was the transport's debug text; now "Address not found" and the like
- [x] Watch-only wallet catching up → card says "You can still receive while Fob catches up."
- [x] TalkBack → chip reads "Catching up, 34%, about 6 minutes left, button, collapsed"; double tap opens the card and focus can move into it (title read as a heading, each row as one item, e.g. "Time left, about 4 min"); "expanded" when open (from the accessibility tree)
- [x] Dark mode → chip on the dark sunken colour, ring track and arc readable, card on the dark card colour with its shadow, Node settings in the orange text colour
- [x] Font size 200% → chip text on one line; card rows wrap their values rather than overlapping the labels — fixed in 7a6aece: at 200% on a narrow phone the card covered the chip and lost Node settings, and values broke mid-word
- [x] Phone in landscape → the card stays under the chip and on screen

## Step 5 · Send sheet

- [x] Tall phone (e.g. 6.7"), Send, pull the sheet up → it is just as tall as To, Amount, Note, the ready line and Review, no gap under Review; the camera fills the rest with a 200 dp frame centred in it
- [x] Small phone (or display size largest) → the sheet stops under the top bar; the form scrolls, Review stays at the bottom; the frame hides when the room above is too small (display size large on a 360 dp phone at 200%; the folded sheet scrolls past its 62% cap)
- [x] Landscape → folded sheet keeps a strip of camera with "Point at a payment code"; pulled up, the form scrolls under the top bar with Review visible; no frame
- [x] Font size 200% → both stops fit their content; pulled up, the form scrolls and Review stays visible
- [x] Pulled up, tap Amount → the keyboard rises, Review sits just above it, the field scrolls into view; tap Note → it scrolls into view too
- [x] Drag the handle slowly up and down → the sheet's top edge stays under the finger the whole way; the form appears as soon as it rises above folded; the camera frame grows and shrinks with it
- [x] Slow drag to just under halfway and let go → it eases back to folded in about 300 ms; just over halfway → it eases up (under halfway eases back; over halfway not tried by hand)
- [x] Quick flick up from a little above folded → it opens; quick flick down from a little under open → it folds
- [x] Tap the handle → the same 300 ms move both ways
- [x] Back with the sheet up → it folds with the same move; Back again → Send closes
- [x] Scan an address while folded → the sheet rises with the form filled; scan a different code with To filled → "Use this code instead?" appears and the sheet grows to fit it (a monero: link fills the form with the sheet up; "Use this code instead?" needs the real camera)
- [x] Review with too little ready → the error shows above Review and the sheet grows to show it
- [~] Scan another Fob's profile → its sheet with the large frame over it; Pay → the form rises filled (needs the real camera)
- [~] Review → Confirm → Back → the form as it was, pulled up, sheet fitting it (needs a funded wallet)
- [x] Price toast showing while pulled up → it sits above Review, not over it
- [x] TalkBack on the handle → "Type the payment instead, collapsed" / "Back to the camera, expanded"; double tap moves the sheet (from the accessibility tree)
- [x] Dark mode → the sheet's paper and handle in dark colours, no light gap below Review while it moves

## Step 6 · Home people
- [x] New wallet, no profile → person outline in a sunken circle between the chip and the cog; TalkBack "Add a shareable Fob contact, button"
- [x] Tap it with no profile → the profile setup screen (as Settings' row); save a name → back on Home the initial shows at once, no outline flash
- [x] Profile with a photo → the photo at 32 dp in the bar; TalkBack "Your profile, button"; tap → Your profile — fixed in 8c3fe95: no photo could be picked at all on Android 16 ("couldn't be made small enough"); now it shows
- [x] Edit the photo or name in Your profile, go back → Home's avatar changed without reopening Fob
- [x] Photo set but name cleared (if possible) → outline, as Settings' row (the editor won't save a photo without a name, so the outline stays)
- [x] Activity card → "Contacts" chip with the people icon before Coins, both 40 dp pills; Contacts opens Contacts, Coins opens Coins
- [x] 360 dp phone (or display size largest), stagenet, catching up with "6 min left" → "Fob" word hidden, badge, chip with words, avatar and cog all whole, nothing clipped
- [x] Same at font size 200% → the chip shows only its ring, the badge stays; TalkBack on the chip still reads the time left
- [~] Mainnet, synced, normal phone → mark, "Fob", chip "Synced", avatar, cog as the board (no mainnet wallet made on the emulator)
- [x] Activity header at font size 200% → chips move under "Activity", one per line if needed; nothing cut
- [~] Catching up with some activity → "Still finding older ones" on its own line under the header (needs payments)
- [x] Dark mode → the outline circle, chips and badge in dark colours; avatar initial readable
- [x] RTL language (e.g. Arabic) → the bar mirrors: mark at the right, cog at the left (mirrored; Rust's English words read right to left, "h left 29", as everywhere in an RTL language)
- [x] Sync chip tapped with the avatar beside it → details card still opens under the chip

## Step 7 · Backup and keys
- [x] Settings → Wallet card → "Backup and keys" reads "Seed phrase, wallet code, keys" (view-only wallet: "Wallet code, keys")
- [x] Tap it → fingerprint / screen lock prompt titled "Backup and keys"; succeed → the hub as the board: warning card, Back up (Seed phrase, Wallet code), For other wallets (Keys), footer line
- [x] Cancel the prompt (Back or Cancel) → still on Settings, nothing opened; tap again → asked again
- [x] Phone with no screen lock → the hub opens straight away
- [x] Change font size while the prompt is up, then succeed → the hub opens once, not twice
- [x] Double-tap the row quickly → one prompt, one hub
- [x] Hub → Keys → Back → Seed phrase → Back → Wallet code → no prompt in between; Back to Settings, open again → asked again — fixed in 618aa71: also, leaving Fob from any of them and coming back asks again
- [x] New wallet before "I've written them down" → Seed phrase line "16 words that restore everything · not written down yet"; after → "· backed up"; restored from 25 words → "25 words that restore everything" (restored from 16 words: "16 words that restore everything"; no 25-word seed to hand)
- [x] Keys: Secret card (Secret spend key with orange "Spends money", Secret view key "Sees payments"), Public card (Main address, Restore height "<height> · <date>", Public view key, Public spend key), every value in full
- [x] Copy each key, paste into a notes app → exactly the value shown; Restore height pastes the number alone
- [x] After Copy → tick icon and "Copied" under the value for about 2 s; TalkBack says "Copied"
- [x] Android 13+: the system's clipboard notice hides the copied key; the keyboard's clipboard suggestion doesn't show it (Android 16: "••••••")
- [x] Screenshot on the hub, Keys, Seed phrase and Wallet code → blocked; recents thumbnail blank (the window is SECURE and the phone's own screenshot is black; the emulator's host capture still sees through it)
- [x] Watch-only wallet → hub has Wallet code and Keys only, its own warning and footer; Keys has no Secret spend key row and all four public values
- [x] Old restore height (a wallet restored from 0 or an old block) → the number alone, no date
- [x] Dark mode → warning card, sunken icon circles and badges in dark colours; orange "Spends money" readable
- [x] TalkBack → each hub row read as one ("Seed phrase, 16 words …"); Copy buttons read "Copy the secret spend key", "Copy the main address" … (from the accessibility tree)
- [x] Small phone, font size 200% → keys wrap inside the card at any character, Copy stays 48 dp at the right, badge beside the title not clipped — fixed in 5852ab2: the title broke inside its words beside the badge; the badge now goes under it

## Step 8 · Node
- [x] Settings → Syncing → Node reads "Working · stagenet.xmr-tw.org:38081" (after syncing or opening Node); tap it → Node screen: Back, "Node", refresh icon at the top right
- [x] Node opens → icon spins, card "Checking…" with rows filling in one by one; then a green tick, "Working", the address with no http://, Answers "<n> ms", Right network "stagenet", Up to date "block <n> · <n> min ago", Sends blocks to Fob "<n> blocks/s · <n> MB/s" (a slow public node reads "38 kB/s")
- [x] Tap refresh → spins again, checks run again; tapping while it spins does nothing
- [x] Change node: each other default node with a dot, its address and "Checking…", then "<ms> ms · in sync" (one "· fastest") with Use, green dot
- [x] Tap Use on another node → it becomes the card's node and is checked; the old one is now in the list; Home keeps syncing from the new one
- [x] Unreachable node: add "10.255.255.1:38081" → Answers "didn't answer in 5 s", the rest without results, verdict "Doesn't answer", Use greyed; closed, it shows in the list as "Didn't answer in 5 s" with a red dot and no Use
- [x] Closed port (your node's host, an unused port) → "refused to connect"; a made-up host → "address not found"
- [x] Mainnet node on a stagenet wallet (e.g. node.sethforprivacy.com:18089) → Right network "mainnet, not stagenet", verdict "Wrong network", list line "Mainnet node: this wallet is stagenet"
- [~] Lagging node (a node still syncing, or one stopped for an hour) → Up to date red "<n> h behind" / "<n> days behind", Sends blocks still checked; sheet button "Use it anyway" works and the Working card shows "Not up to date" (needs a lagging node)
- [x] https node with a self-signed certificate, box unticked → Certificate "self-signed, not accepted" first, nothing else runs, verdict "Certificate not accepted" (https://node.monerodevs.org:38089, a real self-signed monerod)
- [x] Same node, tick "Accept self-signed certificates" → checks clear to "Check"; Check → Certificate "self-signed, accepted", the rest run; Use → syncing works through it; close and reopen Fob → still on it and still working — fixed in 3e3e156: with the box ticked it still said "couldn't connect" for monerod's own (version 1) certificate
- [x] https public node (e.g. node.monerodevs.org:18089 on mainnet) → Certificate "valid" (https://node.sethforprivacy.com:443 on a stagenet wallet: "valid", then Wrong network)
- [x] Sheet: keyboard up → field, box and buttons stay above it; tap outside or Back → sheet closes, checks stop
- [x] Sync chip card → "Node settings" still opens Node
- [x] Dark mode → card, tick/"!"/dots circles, red and green dots, checkbox and sheet in dark colours; nothing black-on-dark
- [x] TalkBack → refresh reads "Check again" (and "Checking" while spinning); the verdict announced when it changes; each check read as one ("Up to date: block …", "Answers: not checked"); each list row read with its line, Use read "Use <address>"; the checkbox read as a checkbox with its state (from the accessibility tree)
- [x] Font size 200% → check results wrap at the right, list lines wrap, Use stays 48 dp — fixed in 2f3d4c0: results broke a character a line; they keep whole words and the title wraps first
- [x] Onion address in the sheet → Answers "needs Tor, not in Fob yet", nothing looked up

## Emulator pass

2026-10-31, emulator `Pixel_9_Pro_qa` (Android 16, 1280 × 2856, punch-hole,
gesture navigation), stagenet. First the round 3 wallet already on it,
installed over (with Show prices off, then a copy of its file from before
with euros), then a view-only wallet restored from its keys (from a recent
height for the sync chip, then from block 1 for "Starting" and the
undated restore height), then its 16 words restored for Send. Prices from
Kraken and CoinGecko over the real network; nodes were the public
stagenet ones, node.monerodevs.org's https port (a real monerod with its
own self-signed certificate) and node.sethforprivacy.com:443 (a valid
one). Airplane mode for failures; `cmd locale set-app-locales` for en-AU
and Arabic; `wm size 1080x2340` for a 360 dp phone; font scale 2.0; a PIN
through `locksettings` for the lock. TalkBack items were read from the
accessibility tree (what uiautomator sees), not listened to. Screens
behind FLAG_SECURE were captured with the emulator's own screenshot.

130 of 138 passed (12 of them after a fix), 8 can't be checked here.

Fixed in this pass:
- 7dcc039 Settings' Price from row reads "Kraken · web" / "CoinGecko · Tor", the Ask over row's name (known issue a).
- 53e35dc The sync card's error line in the checks' words, not the transport's debug text; Settings' Node line says "Can't reach node" whenever syncing fails, as the chip (known issue d).
- 3e3e156 A node marked to accept self-signed certificates works with monerod's own (version 1) certificate.
- 618aa71 Leaving Fob closes Backup and keys, Keys, Seed phrase and Wallet code (known issue b); Home's back-up reminder asks for the lock (known issue c); Settings' Node line redraws as syncing fails or recovers.
- 6cf5e71 The price toast's cog keeps Send's and Receive's forms; the toast steps aside while the keyboard is up; "Trying again…" shows for at least 0.8 s.
- 8231fa1 The Money card's value sits at the row's end by its chevron.
- 7a6aece The sync card fits under its chip at a big font, scrolling, with whole words.
- 8c3fe95 Photos work again on Android 16, whose WebP encoder adds a colour profile Fob refused.
- 5852ab2 Keys' badges go under the title on a small phone or big font.
- 2f3d4c0 Node's check results keep whole words at a big font (one row layout shared with the sync card); the node's address shows whole.

Seen and left as they are:
- The time left can rise when the pace falls a lot (decision 205): 2 → 3 min once, and from block 1, where early blocks fly by, 52 min → 2 h 25 min.
- In Arabic the bar and card mirror, but Rust's English words are laid out right to left ("h left 29").
- A node added over https with the same host and port as a default http one shows as the same "host:port" twice in the list (decision 232 drops the scheme).
- Home's "· 10 min old" appears with the next redraw (a status or price change), not on the minute.
