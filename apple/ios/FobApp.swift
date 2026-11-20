// Fob for iPhone and iPad (IPH-001). The app reads `-FobScene` / `-FobTheme`
// into its AppModel and puts RootSwitch in FobRoot, which draws it in the
// theme's colours (or a FobUI gallery for `-FobScene gallery-<name>`). The
// iPhone team owns apple/ios, the iPad team apple/ios/Pad.

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
            FobRoot(model: model) {
                RootSwitch(model: model)
            }
        }
    }
}
