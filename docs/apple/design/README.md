# Fob for Apple: design boards

The design source of truth for the Apple apps, copied from the Fob for
Apple canvas: https://claude.ai/artifact/MXtjYeKRGf83k8Gv9MDjtM

Each `*.dc.html` is one self-contained artboard with exact colours, type
and copy; `fob.css` holds the shared tokens (OLED by default, Light on the
`Light` boards). Open a board in a browser to see it (it loads Manrope
from Google Fonts; `support.js` is the canvas's runtime and is not needed
to read the board). Match them.

| file | platform | screen | size (pt) |
|---|---|---|---|
| `Main.dc.html` | iPhone | Wallet (OLED), tab bar Wallet / Contacts / Settings | 390 x 844 |
| `PhoneHomeLight.dc.html` | iPhone | Wallet (Light) | 390 x 844 |
| `PhoneReceive.dc.html` | iPhone | Receive, full-screen page | 390 x 844 |
| `PhoneReceiveLight.dc.html` | iPhone | Receive (Light) | 390 x 844 |
| `PhoneSend.dc.html` | iPhone | Send (camera) | 390 x 844 |
| `PhoneSendForm.dc.html` | iPhone | Send (form pulled up) | 390 x 844 |
| `PhoneConfirm.dc.html` | iPhone | Confirm payment with Face ID | 390 x 844 |
| `PhonePayment.dc.html` | iPhone | Payment & receipt | 390 x 844 |
| `PhoneContacts.dc.html` | iPhone | Contacts tab | 390 x 844 |
| `PhoneSettings.dc.html` | iPhone | Settings tab | 390 x 844 |
| `PhoneWelcome.dc.html` | iPhone | Welcome (first run) | 390 x 844 |
| `PhoneBackup.dc.html` | iPhone | Back up your wallet (seed words) | 390 x 844 |
| `PhoneCatchUp.dc.html` | iPhone | Catch up (scan) | 390 x 844 |
| `PhoneCoins.dc.html` | iPhone | Coins | 390 x 844 |
| `PhoneSplit.dc.html` | iPhone | Split up (tap the notes) | 390 x 844 |
| `PadHome.dc.html` | iPad | Wallet: ONE screen, balance left, activity right | 1180 x 820 |
| `PadSend.dc.html` | iPad | Send, a form sheet | 1180 x 820 |
| `PadSendTo.dc.html` | iPad | Send > "Pay who?" pushed in the sheet (tap To) | 1180 x 820 |
| `MacHome.dc.html` | macOS | Wallet window (OLED), one small window | 480 x 760 |
| `MacSend.dc.html` | macOS | Confirm, pushed in the window (Touch ID) | 480 x 760 |
| `MacSettings.dc.html` | macOS | Settings window | 760 x 560 |

Key rules: OLED is the default drawn (true black #000 page, cards #0C0B0A
held by 1px #2A2723 hairlines, orange #FF6600 tint, orange text #FF8F45);
Light mirrors Android's Light colours
(`android/app/src/main/kotlin/com/monokulo/fob/ui/Theme.kt`); Manrope
ExtraBold for balances and titles, SF Pro for UI text (Dynamic Type); no
filler copy: Fob's words come from `crates/fob-words`.
