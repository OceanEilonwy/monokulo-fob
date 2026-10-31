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

- [ ] Airplane mode, open Fob on Home → within a few seconds the toast rises above the gesture bar: warning mark, "Couldn't get the USD price", "Kraken didn't answer · using the 14:02 price" (or "No price yet · amounts stay in XMR" on a fresh install), Retry, cog
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
- [ ] Settings while failing → Price from row's line red: "Kraken · over the internet · Not answering"; it turns grey again once a lookup works
- [ ] Change currency (Also show amounts in) while offline → toast shows for the new currency at once, even if one was swiped away
- [ ] Send in money with a price over 5 minutes old, offline → typing an amount shows the toast; the field's line ends "price from 14:02" once the price is 10 minutes old
- [ ] Price over a day old (set the phone's clock a day ahead, offline) → no chip on Send or Receive amounts, line "No USD price now · type the amount in XMR"; Home shows XMR only
- [ ] A payment from a few days ago with no price, history failing → "Couldn't get 29 Oct's USD price", "Kraken didn't answer · shown as not available"
- [ ] CoinGecko busy (Price from opened many times) → toast says "CoinGecko is busy · …"
- [ ] Dark mode → the toast is a light card with dark words, orange Retry readable, green tick on "updated"; light mode → dark card, light words
- [ ] TalkBack → toast read as it appears ("Couldn't get the USD price, Kraken didn't answer…") without moving focus; Retry and "Price settings" reachable, each 48 dp; actions menu offers Dismiss
- [ ] Large font (200%) → title and line wrap inside the card; Retry and the cog stay on screen
- [ ] Landscape → the toast stays inside the safe area, clear of a cutout
