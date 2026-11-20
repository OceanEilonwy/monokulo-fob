// Wallet, the window's first page (MacHome): the chips, the balance with
// what it's worth and how much of it is ready, and Receive / Send. The
// activity under them comes with MAC-002.

import FobCore
import FobUI
import SwiftUI

struct MacWalletPage: View {
    let model: AppModel

    @Environment(\.fobColors) private var colors

    var body: some View {
        VStack(spacing: 0) {
            if model.hasWallet {
                header
            }
            Spacer(minLength: 0)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(colors.paper)
    }

    private var header: some View {
        let balance = model.balance
        let words = model.labels.home
        return VStack(spacing: 8) {
            HStack(spacing: 6) {
                if model.network == .stagenet {
                    StagenetChip(words.stagenet, size: .small)
                }
                SyncChipView(model.syncChip, label: model.state.syncLabel, size: .small)
            }
            HStack(alignment: .firstTextBaseline, spacing: 6) {
                Text(balance.total)
                    .font(FobFonts.manrope(46))
                    .tracking(-1.8)
                    .monospacedDigit()
                    .foregroundStyle(colors.ink)
                    .accessibilityIdentifier("balance")
                Text(verbatim: "XMR")
                    .font(FobFonts.manrope(18, relativeTo: .title3))
                    .foregroundStyle(colors.muted)
            }
            .padding(.top, 6)
            HStack(spacing: 0) {
                if let fiat = balance.fiat {
                    Text(fiat)
                        .accessibilityIdentifier("fiat")
                    Text(verbatim: " · ")
                }
                Text(verbatim: "\(balance.ready) \(words.readyToSpend)")
                    .accessibilityIdentifier("ready")
            }
            .font(.body)
            .monospacedDigit()
            .foregroundStyle(colors.muted)
            .lineLimit(1)
            ReceiveSendButtons(labels: words, layout: .mac) {
                model.show(.receive)
            } send: {
                model.show(.send)
            }
            .padding(.top, 14)
        }
        .padding(.top, 14)
        .padding(.horizontal, 24)
    }
}
