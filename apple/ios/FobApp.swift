// Fob for iPhone and iPad. The scaffold shows the Wallet placeholder; the
// iPhone team owns apple/ios (the iPad team apple/ios/Pad).

import FobCore
import FobUI
import SwiftUI

@main
struct FobApp: App {
    @State private var model = AppModel()

    init() {
        FobFonts.register()
    }

    var body: some Scene {
        WindowGroup {
            PlaceholderWalletView(model: model)
        }
    }
}
