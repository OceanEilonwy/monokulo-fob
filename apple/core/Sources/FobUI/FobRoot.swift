// The view both apps put in their window (CORE-020): the app's own screens
// in the colours AppModel.theme names, or, for `-FobScene gallery-<name>`,
// FobUI's gallery of that component in their place, so core can prove its
// components in CI before any screen uses them.

import FobCore
import SwiftUI

public struct FobRoot<Content: View>: View {
    let model: AppModel
    let content: Content

    /// `content` is the app: the iPhone's tab bar, the iPad's one screen,
    /// the Mac's window.
    public init(model: AppModel, @ViewBuilder content: () -> Content) {
        self.model = model
        self.content = content()
    }

    public var body: some View {
        Group {
            if let gallery = model.gallery {
                GalleryView(name: gallery, model: model)
            } else {
                content
            }
        }
        .fobTheme(model.theme)
    }
}
