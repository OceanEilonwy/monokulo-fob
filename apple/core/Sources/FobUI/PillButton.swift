// Fob's pill buttons (CORE-021; fob.css .btn): the orange primary, the
// hairline secondary and the tinted one the Mac uses for Receive, each with
// a leading SF Symbol, in the sizes the boards draw. Pressed and disabled
// states come from the button style, so a plain SwiftUI Button can wear it.

import FobCore
import SwiftUI

/// How a pill is filled.
public enum PillStyle: Sendable, CaseIterable {
    /// Orange #FF6600 fill, near-black text: Send, Review, Pay.
    case primary
    /// No fill, a 1.5pt `outline` ring, ink text: Receive on the phone.
    case secondary
    /// `sunken` fill, ink text: Receive on the Mac, Paste.
    case tinted
}

/// How big a pill is: the boards' heights.
public enum PillSize: Sendable, CaseIterable {
    /// 50pt, 17pt semibold: Home's Receive and Send.
    case regular
    /// 56pt: the form's Review.
    case large
    /// 40pt: the Mac window's Receive and Send.
    case compact
    /// 34pt drawn, 44pt to tap: Paste beside a field.
    case small

    var height: CGFloat {
        switch self {
        case .regular: 50
        case .large: 56
        case .compact: 40
        case .small: 34
        }
    }

    var font: Font {
        switch self {
        case .regular, .large: .headline
        #if os(macOS)
        case .compact, .small: .body.weight(.semibold)
        #else
        case .compact, .small: .subheadline.weight(.semibold)
        #endif
        }
    }

    var padding: CGFloat {
        switch self {
        case .regular, .large: 22
        case .compact: 18
        case .small: 14
        }
    }
}

/// The pill look for any Button: `.buttonStyle(.fobPill(.primary))`.
public struct FobPillButtonStyle: ButtonStyle {
    public let style: PillStyle
    public let size: PillSize

    public init(_ style: PillStyle = .primary, size: PillSize = .regular) {
        self.style = style
        self.size = size
    }

    public func makeBody(configuration: Configuration) -> some View {
        PillBody(style: style, size: size, pressed: configuration.isPressed) {
            configuration.label
        }
    }
}

extension ButtonStyle where Self == FobPillButtonStyle {
    /// Fob's pill: `style` filled, `size` high.
    public static func fobPill(_ style: PillStyle = .primary, size: PillSize = .regular) -> FobPillButtonStyle {
        FobPillButtonStyle(style, size: size)
    }
}

struct PillBody<Content: View>: View {
    let style: PillStyle
    let size: PillSize
    let pressed: Bool
    @ViewBuilder let label: Content

    @Environment(\.fobColors) private var colors
    @Environment(\.isEnabled) private var enabled

    var body: some View {
        label
            .font(size.font)
            .labelStyle(PillLabelStyle())
            .lineLimit(1)
            .minimumScaleFactor(0.8)
            .foregroundStyle(foreground)
            .padding(.horizontal, size.padding)
            .frame(maxWidth: .infinity, minHeight: size.height)
            .background(fill, in: Capsule())
            .overlay {
                if style == .secondary {
                    Capsule().strokeBorder(enabled ? colors.outline : colors.line, lineWidth: 1.5)
                }
            }
            .opacity(pressed ? 0.72 : 1)
            // Paste's 34pt pill still takes a 44pt tap.
            .padding(.vertical, max(0, (44 - size.height) / 2))
            .contentShape(Rectangle())
            .animation(.easeOut(duration: 0.12), value: pressed)
    }

    private var fill: Color {
        switch style {
        case .primary: enabled ? FobColors.orange : colors.track
        case .secondary: pressed ? colors.sunken : .clear
        case .tinted: enabled ? colors.sunken : colors.onPaper
        }
    }

    private var foreground: Color {
        guard enabled else { return colors.outline }
        return style == .primary ? FobColors.onOrange : colors.ink
    }
}

/// Symbol then words, 8pt apart, the symbol a little bolder like the boards'.
private struct PillLabelStyle: LabelStyle {
    func makeBody(configuration: Configuration) -> some View {
        HStack(spacing: 8) {
            configuration.icon
                .fontWeight(.bold)
                .imageScale(.medium)
            configuration.title
        }
    }
}

/// A pill button: words from fob-words, an optional leading SF Symbol.
public struct PillButton: View {
    let title: String
    let systemImage: String?
    let style: PillStyle
    let size: PillSize
    let action: @MainActor () -> Void

    public init(_ title: String, systemImage: String? = nil, style: PillStyle = .primary,
                size: PillSize = .regular, action: @escaping @MainActor () -> Void) {
        self.title = title
        self.systemImage = systemImage
        self.style = style
        self.size = size
        self.action = action
    }

    public var body: some View {
        Button(action: action) {
            if let systemImage {
                Label(title, systemImage: systemImage)
            } else {
                Text(title)
            }
        }
        .buttonStyle(.fobPill(style, size: size))
    }
}

/// Home's Receive / Send pair: side by side and equally wide on the phone
/// (hairline Receive, orange Send; Main), 140pt each and centred in the
/// Mac's window (tinted Receive; MacHome).
public struct ReceiveSendButtons: View {
    /// Where the pair sits.
    public enum Layout: Sendable {
        case phone, mac
    }

    let labels: HomeLabels
    let layout: Layout
    let receive: @MainActor () -> Void
    let send: @MainActor () -> Void

    public init(labels: HomeLabels, layout: Layout, receive: @escaping @MainActor () -> Void,
                send: @escaping @MainActor () -> Void) {
        self.labels = labels
        self.layout = layout
        self.receive = receive
        self.send = send
    }

    /// The symbols the boards draw: an arrow down into the wallet, one up out of it.
    public static let receiveSymbol = "arrow.down"
    public static let sendSymbol = "arrow.up"

    public var body: some View {
        switch layout {
        case .phone:
            HStack(spacing: 12) {
                buttons(receiveStyle: .secondary, size: .regular, width: nil)
            }
        case .mac:
            HStack(spacing: 10) {
                buttons(receiveStyle: .tinted, size: .compact, width: 140)
            }
        }
    }

    @ViewBuilder
    private func buttons(receiveStyle: PillStyle, size: PillSize, width: CGFloat?) -> some View {
        PillButton(labels.receive, systemImage: Self.receiveSymbol, style: receiveStyle, size: size, action: receive)
            .frame(width: width)
            .accessibilityIdentifier("receive-button")
        PillButton(labels.send, systemImage: Self.sendSymbol, style: .primary, size: size, action: send)
            .frame(width: width)
            .accessibilityIdentifier("send-button")
    }
}
