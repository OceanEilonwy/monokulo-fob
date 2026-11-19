// Fob's type ramp (docs/apple/design/fob.css): Manrope ExtraBold for
// balances and titles, SF Pro for everything else. Every style scales with
// Dynamic Type: the SF ones are system text styles, the Manrope ones are
// sized relative to the text style they stand in for.

import SwiftUI

/// A style on fob.css's ramp, by the name the boards give it.
public enum FobTextStyle: String, Sendable, CaseIterable, Identifiable {
    /// The balance on Home: Manrope 48.
    case balance
    /// Page titles: Manrope 34 (`.large-title`).
    case largeTitle
    /// Card and wallet titles: Manrope 22 (`.title2`).
    case title2
    /// SF 17 semibold (`.headline`).
    case headline
    /// SF 17 (`.body`).
    case body
    /// SF 16 (`.callout`).
    case callout
    /// SF 15 (`.subhead`).
    case subheadline
    /// SF 13 (`.footnote`).
    case footnote
    /// SF 12 (`.caption`).
    case caption
    /// Grouped-list section labels: SF 13 semibold, upper case (`.section-label`).
    case sectionLabel

    public var id: String { rawValue }

    /// The font, Dynamic Type included.
    public var font: Font {
        switch self {
        case .balance: FobFonts.manrope(48, relativeTo: .largeTitle)
        case .largeTitle: FobFonts.manrope(34, relativeTo: .largeTitle)
        case .title2: FobFonts.manrope(22, relativeTo: .title2)
        case .headline: .headline
        case .body: .body
        case .callout: .callout
        case .subheadline: .subheadline
        case .footnote: .footnote
        case .caption: .caption
        case .sectionLabel: .footnote.weight(.semibold)
        }
    }

    /// Manrope's titles are drawn tighter, as the boards' letter-spacing.
    public var tracking: CGFloat {
        switch self {
        case .balance, .largeTitle: -1
        case .title2: -0.4
        case .sectionLabel: 0.3
        default: 0
        }
    }

    /// Section labels are upper case.
    public var uppercased: Bool { self == .sectionLabel }

    /// Its name as fob.css gives it, for the gallery.
    public var cssName: String {
        switch self {
        case .balance: "balance"
        case .largeTitle: "large-title"
        case .title2: "title2"
        case .headline: "headline"
        case .body: "body"
        case .callout: "callout"
        case .subheadline: "subhead"
        case .footnote: "footnote"
        case .caption: "caption"
        case .sectionLabel: "section-label"
        }
    }
}

extension View {
    /// Sets this text in `style`: its font, tracking and case.
    public func fobFont(_ style: FobTextStyle) -> some View {
        font(style.font)
            .tracking(style.tracking)
            .textCase(style.uppercased ? .uppercase : nil)
    }
}
