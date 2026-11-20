// The Mac's one window (MAC-001, MacHome): Wallet under a unified title bar
// with the wallet menu in the middle, Contacts on the left and Refresh and
// Settings on the right, and every other page pushed in place over it with
// a bare back chevron (MacSend). Settings opens its own window.

import FobCore
import FobUI
import SwiftUI

struct MacWindow: View {
    /// The boards' window, MacHome.
    static let size = CGSize(width: 480, height: 760)
    /// As small as the pages still fit.
    static let minSize = CGSize(width: 420, height: 560)
    /// No wider than a page needs: the window is a column, like the GTK app's.
    static let maxWidth: CGFloat = 560

    let model: AppModel

    @Environment(\.fobColors) private var colors
    @Environment(\.openSettings) private var openSettings

    var body: some View {
        NavigationStack(path: pages) {
            MacWalletPage(model: model)
                .toolbar { toolbar }
                .navigationDestination(for: Route.self) { route in
                    MacPage(route: route, model: model)
                }
        }
        .toolbar(removing: .title)
        .toolbarBackgroundVisibility(.hidden, for: .windowToolbar)
        .containerBackground(colors.paper, for: .window)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("mac-window")
    }

    /// The window's pages over Wallet. The Mac has no tab bar: Contacts is
    /// a page pushed on Wallet, so the Contacts tab shows as Contacts with
    /// its own pages over it. Settings is a window of its own, so its tab
    /// adds no page here. Going back to Wallet selects the Wallet tab.
    private var pages: Binding<[Route]> {
        Binding {
            model.tab == .contacts ? [.contacts] + model.path : model.path
        } set: { pages in
            if pages.first == .contacts {
                model.tab = .contacts
                model.path = Array(pages.dropFirst())
            } else {
                model.tab = .wallet
                model.path = pages
            }
        }
    }

    @ToolbarContentBuilder
    private var toolbar: some ToolbarContent {
        let chrome = model.labels.chrome
        ToolbarItem(placement: .navigation) {
            FobIconButton(.contacts, labels: chrome, look: .bar) {
                model.show(.contacts)
            }
        }
        .sharedBackgroundVisibility(.hidden)
        ToolbarItem(placement: .principal) {
            if model.hasWallet {
                WalletChip(model.walletName, accessibilityName: chrome.wallets, layout: .mac)
            }
        }
        .sharedBackgroundVisibility(.hidden)
        ToolbarItem(placement: .primaryAction) {
            FobIconButton(.refresh, labels: chrome, look: .bar) {}
        }
        .sharedBackgroundVisibility(.hidden)
        ToolbarItem(placement: .primaryAction) {
            FobIconButton(.settings, labels: chrome, look: .bar) {
                openSettings()
            }
        }
        .sharedBackgroundVisibility(.hidden)
    }
}
