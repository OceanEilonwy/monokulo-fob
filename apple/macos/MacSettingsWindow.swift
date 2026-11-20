// Settings, a window of its own (MacSettings 760 x 560), opened with the
// title bar's cog or Fob > Settings (Command-comma). Its panes (General,
// Wallets, Syncing, Money, This Mac) come with MAC-006 and MAC-008.

import FobCore
import FobUI
import SwiftUI

struct MacSettingsWindow: View {
    /// The board's window, MacSettings.
    static let size = CGSize(width: 760, height: 560)

    let model: AppModel

    @Environment(\.fobColors) private var colors

    var body: some View {
        colors.paper
            .frame(width: Self.size.width, height: Self.size.height)
            .containerBackground(colors.paper, for: .window)
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("mac-settings")
    }
}
