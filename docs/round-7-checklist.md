# Round 7 device checklist

Added to by each step of `round-7-plan.md`, walked through on an emulator once
every step is in. Each item: what to do, what should happen.

## Step 1 · Currencies and providers

The data side only; most of it shows once step 3's screens are in, some
through step 1's stopgap dialogs in Settings.

- [ ] Install over a wallet made before this build with Show prices off → Settings says "Also show amounts in · XMR only"; no money anywhere; no price request (Fob idle, no "Price from" row)
- [ ] Same with Show prices on in euros → "Euro", amounts in €, Price from Kraken; a payment priced before still shows its € value
- [ ] Also show amounts in → the list: XMR only first, this phone's currency (set the emulator to en-AU: Australian dollar), then all 45 A–Z, each with its providers ("AUD · Kraken, CoinGecko"), the chosen one ticked
- [ ] Phone set to a currency Fob hasn't (fr-SN, CFA franc) → no "this phone's" group
- [ ] Choose Australian dollar on Kraken → stays on Kraken; Home's balance in A$ within a few percent of CoinGecko's (Price from shows both rates)
- [ ] Choose New Zealand dollar on Kraken → "Kraken doesn't price New Zealand dollars, so Fob now asks CoinGecko"; Price from CoinGecko; amounts in NZ$
- [ ] Choose UAE dirham → CoinGecko only (Kraken's USD/AED never trades), amounts "… AED"
- [ ] Choose Japanese yen → amounts with no decimals ("¥93,477"); typing "1500.5" yen on Send says "That isn't an amount"; Receive with ¥1500 asks a sensible XMR amount
- [ ] Choose Kuwaiti dinar → three decimals ("56.123 KWD")
- [ ] Price from with AUD → both providers with a rate (A$…); turn networking off and open it again → both "Not answering", greyed, can't be picked
- [ ] Price from many times in a minute on CoinGecko → at some point "Busy, try again soon", not a raw error
- [ ] Back to US dollar from NZD → stays on CoinGecko; Kraken pickable again; payments priced in dollars earlier show their $ again
- [ ] XMR only → Home, Send, Receive show XMR only; no Show first, no Price from
- [ ] Search (step 3): "zloty", "ZŁOTY", "nzd", "A$", "¥" (yuan and yen) find what they say; "qqq" says "No currency matches “qqq”"
- [ ] Long names (New Taiwan dollar, Israeli new shekel, South African rand) fit the rows; TalkBack reads each row's name and providers, and each Price from line ("Kraken, web only, A$845.10")
- [ ] Light and dark mode: list, Price from and the move toast readable in both

## Step 2 · Price failures

- [ ] Airplane mode, open Fob on Home → within a few seconds the toast rises above the gesture bar: warning mark, "Couldn't get the USD price", "Kraken didn't answer · using the 14:02 price" (or "Kraken didn't answer · no USD price yet" on a fresh install, step 3), Retry, cog
- [ ] Same, with the toast up, open Send and pull the sheet up → the toast sits above Review, not over it; fold the sheet → it drops to the bottom
- [ ] Receive → above Copy and Share; tap the amount field (keyboard up) → stays above the buttons and keyboard
- [ ] A payment's screen, Coins (above Split up), Settings, Contacts → the same toast in the same place
- [ ] Swipe it down → gone; swipe another sideways → gone; a short swipe → springs back
- [ ] Swiped away, stay in airplane mode for 10 minutes on Home → it doesn't come back (once per run)
- [ ] Then pull to refresh → it shows again at once
- [ ] Retry, still offline → spinner, "Trying again…", "Asking Kraken for the USD price", then the failure again
- [ ] Retry after turning airplane mode off → "USD price updated", "1 XMR = $…" with a green tick, gone after 2 s
- [ ] Toast up, turn networking back on and wait for the 5-minute refresh (or pull) → the toast goes by itself
- [ ] Cog from Send → Price from opens; Back → Send as it was (To and amount kept); same from Home and Receive
- [ ] Settings while failing → Price from row's line red with the toast's words (step 3): "Kraken didn't answer · using the 14:02 price", and Try again; it turns grey again once a lookup works
- [ ] Change currency (Also show amounts in) while offline → toast shows for the new currency at once, even if one was swiped away
- [ ] Send in money with a price over 5 minutes old, offline → typing an amount shows the toast; the field's line ends "price from 14:02" once the price is 10 minutes old
- [ ] Price over a day old (set the phone's clock a day ahead, offline) → no chip on Send or Receive amounts, line "No USD price now · type the amount in XMR"; Home shows XMR only
- [ ] A payment from a few days ago with no price, history failing → "Couldn't get 29 Oct's USD price", "Kraken didn't answer · shown as not available"
- [ ] CoinGecko busy (Price from opened many times) → toast says "CoinGecko is busy · …"
- [ ] Dark mode → the toast is a light card with dark words, orange Retry readable, green tick on "updated"; light mode → dark card, light words
- [ ] TalkBack → toast read as it appears ("Couldn't get the USD price, Kraken didn't answer…") without moving focus; Retry and "Price settings" reachable, each 48 dp; actions menu offers Dismiss
- [ ] Large font (200%) → title and line wrap inside the card; Retry and the cog stay on screen
- [ ] Landscape → the toast stays inside the safe area, clear of a cutout

## Step 3 · Settings and money screens

- [ ] Settings, light and dark → Wallet (profile row, Contacts, Backup and keys, each with a round icon), Display (Theme only), Money, Syncing, Network, Receipts, This phone; no Prices card
- [ ] New wallet with no profile → profile row reads only "Add a shareable Fob contact" with a person outline, no second line; set only a photo (if the editor allows) → still that
- [ ] Profile named "Henry" → "Your profile" over "Henry · share as a code" with the photo; rename to "Henrietta Montgomery-Fairweather" → "Henrietta Montg… · share as a code" (suffix whole); same at 200% font
- [ ] Backup and keys → a screen with Seed phrase and Wallet code; each opens as before (seed words, wallet code QR, screenshots blocked there); on a watch-only wallet only Wallet code, and the Settings line says "Wallet code"
- [ ] Money with US dollar → "Also show amounts in · Uses today's XMR price · US dollar ›", Show first XMR | USD, "Price from · Kraken · over the internet ›"
- [ ] XMR only → the Money card has only "Also show amounts in · Fob never looks up a price · XMR only"
- [ ] Also show amounts in → full-screen list: back arrow, title, search pill "Search: name, code or symbol", NONE (ɱ XMR only), THIS PHONE’S, ALL 45; the chosen one ticked; signs in circles (A$, CHF smaller)
- [ ] Type "zloty" → "1 OF 45", Polish złoty; clear button empties it; "qqq" → "No currency matches “qqq”"; keyboard's Search key closes the keyboard; the list scrolls above the keyboard
- [ ] Choose New Zealand dollar while on Kraken → back to Settings, a dark notice above the bottom: "Kraken doesn't price New Zealand dollars, so Fob now asks CoinGecko", gone after about 4 s (tap dismisses it sooner); no system toast
- [ ] TalkBack on the list → search field "Search currencies"; group titles as headings; each row "Australian dollar, AUD · Kraken, CoinGecko", selected or not, the sign not read
- [ ] Long names (New Taiwan dollar, South African rand, Israeli new shekel) fit their rows; in Settings the value wraps rather than pushing the title
- [ ] Price from → "Prices in **US dollars**, checked as this page opens. Tap a provider to pick it."; a spinner row "Asking each provider…", then Kraken (globe) and CoinGecko (globe, onion) on one line each with rates on the right
- [ ] Pick CoinGecko → stays on Price from, its radio filled; Back → Settings says "CoinGecko · over the internet"
- [ ] Ask over → Web "The provider sees this phone's IP address" chosen; Tor greyed: with Kraken "Kraken doesn't answer over Tor", with CoinGecko "Hides this phone's IP address · not built into Fob yet"; tapping Tor does nothing
- [ ] Airplane mode, open Price from → both lines greyed, "Not answering" in red, can't be picked; TalkBack reads "Kraken, web only, not answering"
- [ ] Airplane mode on a fresh wallet → toast "Couldn't get the USD price", "Kraken didn't answer · no USD price yet"; Settings' Price from line the same in red with Try again
- [ ] Swipe the toast away, then Try again in Settings → the toast comes back saying "Trying again…", then the failure again (offline) or "USD price updated" (online), and the row turns grey
- [ ] With a price, stay offline 10+ minutes → Home's money reads "$412.06 · 12 min old" at the line's end; with money first, the big "$412.06" with "12 min old" small beside it; after an hour "1 h old"; Send's ready line "… ready to spend · $308.70 · 1 h old"
- [ ] Coins › Split up in Japanese yen → notes ¥1,000 to ¥50,000; in Vietnamese dong ₫100,000 to ₫5,000,000, each label fitting its tile; in Kuwaiti dinar 1 KWD to 50 KWD; in US dollars $5 to $200 as before
- [ ] Dark mode → Price from's radios, badges and red failures readable; the notice light on dark

## Step 4 · Sync chip

- [ ] Restore a wallet from far back → chip "Starting" with an empty ring for about 3 s, then "N min left" with the ring filling; no bar or "Up to …" line under the balance; "Balance so far" still above it
- [ ] Watch the chip for a minute while catching up → the time only goes down ("7 min left", "6 min left"…), never back up for a small wobble; near the end "20 s left", then a green dot and "Synced"
- [ ] Open Fob a few blocks behind → "Starting", then "Synced", no time shown
- [ ] Tap the chip while catching up → a card under it at the right edge: "Catching up" with the percent, an orange bar, Time left "about 4 min", Payments up to "29 Oct, 14:20", Blocks "3,360 of 9,880 · 28/s", Link speed "1.8 MB/s", "You can still receive and send while Fob catches up.", Node settings
- [ ] Leave the card open → its numbers move with the chip's
- [ ] Tap outside the card, press Back, or tap the chip again → it closes (the chip doesn't reopen it)
- [ ] Node settings on the card → the card closes and Node opens; Back → Home with the card closed
- [ ] Synced, tap the chip → "Synced" card with Payments up to ("Today, 14:20") and Latest block, no bar
- [ ] Airplane mode → red dot "Can't reach node"; card titled "Can't reach node" with "Fob keeps trying on its own." and the error in red; no error line under the balance
- [ ] Watch-only wallet catching up → card says "You can still receive while Fob catches up."
- [ ] TalkBack → chip reads "Catching up, 34%, about 6 minutes left, button, collapsed"; double tap opens the card and focus can move into it (title read as a heading, each row as one item, e.g. "Time left, about 4 min"); "expanded" when open
- [ ] Dark mode → chip on the dark sunken colour, ring track and arc readable, card on the dark card colour with its shadow, Node settings in the orange text colour
- [ ] Font size 200% → chip text on one line; card rows wrap their values rather than overlapping the labels
- [ ] Phone in landscape → the card stays under the chip and on screen

## Step 5 · Send sheet

- [ ] Tall phone (e.g. 6.7"), Send, pull the sheet up → it is just as tall as To, Amount, Note, the ready line and Review, no gap under Review; the camera fills the rest with a 200 dp frame centred in it
- [ ] Small phone (or display size largest) → the sheet stops under the top bar; the form scrolls, Review stays at the bottom; the frame hides when the room above is too small
- [ ] Landscape → folded sheet keeps a strip of camera with "Point at a payment code"; pulled up, the form scrolls under the top bar with Review visible; no frame
- [ ] Font size 200% → both stops fit their content; pulled up, the form scrolls and Review stays visible
- [ ] Pulled up, tap Amount → the keyboard rises, Review sits just above it, the field scrolls into view; tap Note → it scrolls into view too
- [ ] Drag the handle slowly up and down → the sheet's top edge stays under the finger the whole way; the form appears as soon as it rises above folded; the camera frame grows and shrinks with it
- [ ] Slow drag to just under halfway and let go → it eases back to folded in about 300 ms; just over halfway → it eases up
- [ ] Quick flick up from a little above folded → it opens; quick flick down from a little under open → it folds
- [ ] Tap the handle → the same 300 ms move both ways
- [ ] Back with the sheet up → it folds with the same move; Back again → Send closes
- [ ] Scan an address while folded → the sheet rises with the form filled; scan a different code with To filled → "Use this code instead?" appears and the sheet grows to fit it
- [ ] Review with too little ready → the error shows above Review and the sheet grows to show it
- [ ] Scan another Fob's profile → its sheet with the large frame over it; Pay → the form rises filled
- [ ] Review → Confirm → Back → the form as it was, pulled up, sheet fitting it
- [ ] Price toast showing while pulled up → it sits above Review, not over it
- [ ] TalkBack on the handle → "Type the payment instead, collapsed" / "Back to the camera, expanded"; double tap moves the sheet
- [ ] Dark mode → the sheet's paper and handle in dark colours, no light gap below Review while it moves

## Step 6 · Home people
- [ ] New wallet, no profile → person outline in a sunken circle between the chip and the cog; TalkBack "Add a shareable Fob contact, button"
- [ ] Tap it with no profile → the profile setup screen (as Settings' row); save a name → back on Home the initial shows at once, no outline flash
- [ ] Profile with a photo → the photo at 32 dp in the bar; TalkBack "Your profile, button"; tap → Your profile
- [ ] Edit the photo or name in Your profile, go back → Home's avatar changed without reopening Fob
- [ ] Photo set but name cleared (if possible) → outline, as Settings' row
- [ ] Activity card → "Contacts" chip with the people icon before Coins, both 40 dp pills; Contacts opens Contacts, Coins opens Coins
- [ ] 360 dp phone (or display size largest), stagenet, catching up with "6 min left" → "Fob" word hidden, badge, chip with words, avatar and cog all whole, nothing clipped
- [ ] Same at font size 200% → the chip shows only its ring, the badge stays; TalkBack on the chip still reads the time left
- [ ] Mainnet, synced, normal phone → mark, "Fob", chip "Synced", avatar, cog as the board
- [ ] Activity header at font size 200% → chips move under "Activity", one per line if needed; nothing cut
- [ ] Catching up with some activity → "Still finding older ones" on its own line under the header
- [ ] Dark mode → the outline circle, chips and badge in dark colours; avatar initial readable
- [ ] RTL language (e.g. Arabic) → the bar mirrors: mark at the right, cog at the left
- [ ] Sync chip tapped with the avatar beside it → details card still opens under the chip

## Step 7 · Backup and keys
- [ ] Settings → Wallet card → "Backup and keys" reads "Seed phrase, wallet code, keys" (view-only wallet: "Wallet code, keys")
- [ ] Tap it → fingerprint / screen lock prompt titled "Backup and keys"; succeed → the hub as the board: warning card, Back up (Seed phrase, Wallet code), For other wallets (Keys), footer line
- [ ] Cancel the prompt (Back or Cancel) → still on Settings, nothing opened; tap again → asked again
- [ ] Phone with no screen lock → the hub opens straight away
- [ ] Change font size while the prompt is up, then succeed → the hub opens once, not twice
- [ ] Double-tap the row quickly → one prompt, one hub
- [ ] Hub → Keys → Back → Seed phrase → Back → Wallet code → no prompt in between; Back to Settings, open again → asked again
- [ ] New wallet before "I've written them down" → Seed phrase line "16 words that restore everything · not written down yet"; after → "· backed up"; restored from 25 words → "25 words that restore everything"
- [ ] Keys: Secret card (Secret spend key with orange "Spends money", Secret view key "Sees payments"), Public card (Main address, Restore height "<height> · <date>", Public view key, Public spend key), every value in full
- [ ] Copy each key, paste into a notes app → exactly the value shown; Restore height pastes the number alone
- [ ] After Copy → tick icon and "Copied" under the value for about 2 s; TalkBack says "Copied"
- [ ] Android 13+: the system's clipboard notice hides the copied key; the keyboard's clipboard suggestion doesn't show it
- [ ] Screenshot on the hub, Keys, Seed phrase and Wallet code → blocked; recents thumbnail blank
- [ ] Watch-only wallet → hub has Wallet code and Keys only, its own warning and footer; Keys has no Secret spend key row and all four public values
- [ ] Old restore height (a wallet restored from 0 or an old block) → the number alone, no date
- [ ] Dark mode → warning card, sunken icon circles and badges in dark colours; orange "Spends money" readable
- [ ] TalkBack → each hub row read as one ("Seed phrase, 16 words …"); Copy buttons read "Copy the secret spend key", "Copy the main address" …
- [ ] Small phone, font size 200% → keys wrap inside the card at any character, Copy stays 48 dp at the right, badge beside the title not clipped
