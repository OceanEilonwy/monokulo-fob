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
