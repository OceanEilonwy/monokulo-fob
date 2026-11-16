// Fob for macOS: one small window, like the GTK app (docs/apple/design,
// MacHome 480 x 760). The scaffold shows the Wallet placeholder.

import FobCore
import FobUI
import SwiftUI

@main
struct FobMacApp: App {
    @State private var model = AppModel()

    init() {
        FobFonts.register()
    }

    var body: some Scene {
        WindowGroup {
            PlaceholderWalletView(model: model)
                .frame(minWidth: 400, idealWidth: 480, minHeight: 600, idealHeight: 760)
        }
        .defaultSize(width: 480, height: 760)
        .windowResizability(.contentMinSize)
    }
}
