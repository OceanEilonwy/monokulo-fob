// A page pushed in place in the window (MacSend): the title bar shows a bare
// back chevron on the left in place of the toolbar. Each page's own content
// comes with its task (MAC-004 on); until then the page is the window's
// paper, so pushing and going back already work.

import FobCore
import FobUI
import SwiftUI

struct MacPage: View {
    let route: Route
    let model: AppModel

    @Environment(\.fobColors) private var colors

    var body: some View {
        colors.paper
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("mac-page-\(route.rawValue)")
            .navigationBarBackButtonHidden(true)
            .toolbar {
                ToolbarItem(placement: .navigation) {
                    MacBackButton(label: model.labels.backup.back)
                }
                .sharedBackgroundVisibility(.hidden)
            }
    }
}

/// The title bar's back chevron (MacSend): a bare 30 x 28pt muted chevron,
/// like the toolbar's icon buttons, that drops the top page.
struct MacBackButton: View {
    /// Its VoiceOver name, fob-words' "Back".
    let label: String

    @Environment(\.dismiss) private var dismiss
    @Environment(\.fobColors) private var colors

    var body: some View {
        Button {
            dismiss()
        } label: {
            Image(systemName: "chevron.left")
                .font(.system(size: 14, weight: .semibold))
                .foregroundStyle(colors.muted)
                .frame(width: 30, height: 28)
        }
        .buttonStyle(.plain)
        .contentShape(Rectangle())
        .accessibilityLabel(label)
        .accessibilityIdentifier("back")
        .keyboardShortcut("[", modifiers: .command)
    }
}
