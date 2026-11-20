// Fob for macOS: one small window, like the GTK app (docs/apple/design,
// MacHome 480 x 760), with its pages pushed in place, and Settings in a
// window of its own (MacSettings 760 x 560).

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
        Window("Fob", id: "main") {
            FobRoot(model: model) {
                MacWindow(model: model)
            }
            .frame(minWidth: MacWindow.minSize.width, idealWidth: MacWindow.size.width,
                   maxWidth: MacWindow.maxWidth,
                   minHeight: MacWindow.minSize.height, idealHeight: MacWindow.size.height)
        }
        .defaultSize(MacWindow.size)
        .windowResizability(.contentSize)
        .windowToolbarStyle(.unified(showsTitle: false))

        Settings {
            FobRoot(model: model) {
                MacSettingsWindow(model: model)
            }
        }
        .windowResizability(.contentSize)
    }
}
