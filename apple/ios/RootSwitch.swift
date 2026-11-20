// The one place the app picks its layout (IPH-001): the iPhone's tab bar,
// PhoneRoot. The iPad team (IPAD-001) swaps in PadRoot here for a regular
// width; a compact one (Slide Over, a narrow split) keeps the iPhone screens.

import FobCore
import SwiftUI

struct RootSwitch: View {
    let model: AppModel

    var body: some View {
        PhoneRoot(model: model)
    }
}
