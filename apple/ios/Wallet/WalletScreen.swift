// The Wallet tab until IPH-002 draws Main.dc.html in full: the wallet's
// name, its balance in Manrope ExtraBold with what it's worth, and the
// Receive / Send pair, which push their pages.

import FobCore
import FobUI
import SwiftUI

struct WalletScreen: View {
    let model: AppModel

    @Environment(\.fobColors) private var colors

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 24) {
                if model.hasWallet {
                    WalletChip(model.walletName, accessibilityName: model.labels.chrome.switchWallet) {}
                    VStack(alignment: .leading, spacing: 6) {
                        HStack(alignment: .firstTextBaseline, spacing: 8) {
                            Text(model.balance.total)
                                .font(FobFonts.manrope(48))
                                .monospacedDigit()
                                .foregroundStyle(colors.ink)
                                .accessibilityIdentifier("balance")
                            Text(verbatim: "XMR")
                                .font(.title3.weight(.semibold))
                                .foregroundStyle(colors.muted)
                        }
                        if let fiat = model.balance.fiat {
                            Text(fiat)
                                .font(.title3.weight(.semibold))
                                .foregroundStyle(colors.orangeText)
                                .accessibilityIdentifier("fiat")
                        }
                    }
                    ReceiveSendButtons(labels: model.labels.home, layout: .phone,
                                       receive: { model.show(.receive) },
                                       send: { model.show(.send) })
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(16)
        }
        .background(colors.paper.ignoresSafeArea())
        .fobBars()
    }
}
