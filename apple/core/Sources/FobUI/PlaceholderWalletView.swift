// The scaffold's Wallet screen: the scene's wallet name and balance in Fob's
// colours, until the device teams build the real ones (docs/apple/design).
// It sits in FobRoot, so `-FobScene gallery-<name>` shows the gallery in the
// scaffold apps too.

import FobCore
import SwiftUI

public struct PlaceholderWalletView: View {
    let model: AppModel

    public init(model: AppModel) {
        self.model = model
    }

    public var body: some View {
        FobRoot(model: model) {
            WalletCard(wallet: model.wallet)
        }
    }
}

private struct WalletCard: View {
    let wallet: SceneWallet?
    @Environment(\.fobColors) private var colors

    var body: some View {
        ZStack(alignment: .top) {
            colors.paper.ignoresSafeArea()
            VStack(alignment: .leading, spacing: 16) {
                if let wallet {
                    HStack {
                        Text(wallet.walletName)
                            .font(FobFonts.manrope(22, relativeTo: .title2))
                            .foregroundStyle(colors.ink)
                            .accessibilityIdentifier("walletName")
                        Spacer()
                        Text(wallet.syncText)
                            .font(.subheadline.weight(.semibold))
                            .foregroundStyle(colors.green)
                    }
                    VStack(alignment: .leading, spacing: 6) {
                        HStack(alignment: .firstTextBaseline, spacing: 8) {
                            Text(wallet.balance.totalText)
                                .font(FobFonts.manrope(48))
                                .monospacedDigit()
                                .foregroundStyle(colors.ink)
                                .accessibilityIdentifier("balance")
                            Text("XMR")
                                .font(.title3.weight(.semibold))
                                .foregroundStyle(colors.muted)
                        }
                        if let fiat = wallet.balance.fiatText {
                            Text(fiat)
                                .font(.title3.weight(.semibold))
                                .foregroundStyle(colors.orangeText)
                                .accessibilityIdentifier("fiat")
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(20)
                    .background(colors.card, in: RoundedRectangle(cornerRadius: 20, style: .continuous))
                    .overlay(RoundedRectangle(cornerRadius: 20, style: .continuous).strokeBorder(colors.line, lineWidth: 1))
                }
                Spacer(minLength: 0)
            }
            .padding(16)
        }
    }
}
