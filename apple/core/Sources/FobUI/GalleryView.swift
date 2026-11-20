// FobUI's component gallery (CORE-020; docs/apple/coordination.md,
// "Components"): `-FobScene gallery-<name>` shows one component, or the
// theme itself, alone on the page, for the core-owned UI tests in
// apple/core/UITests to screenshot. Each component task adds its page here.

import FobCore
import SwiftUI

public struct GalleryView: View {
    /// The page: "theme", or a component's name ("pills").
    public let name: String
    /// The app's model, for pages that draw a scene's data.
    let model: AppModel?

    @Environment(\.fobColors) private var colors

    public init(name: String, model: AppModel? = nil) {
        self.name = name
        self.model = model
    }

    /// The pages there are.
    public static let names = ["theme", "biometrics", "pills", "chips", "chrome", "ring", "coinbar"]

    /// The words, the app's when there is one.
    private var words: Labels {
        model?.labels ?? labels(device: "iPhone", biometry: .faceId)
    }

    public var body: some View {
        if name == "chrome" {
            // The bars themselves: a whole tab bar and navigation stack.
            ChromeGallery(words: words)
                .accessibilityElement(children: .contain)
                .accessibilityIdentifier("gallery")
        } else {
            page
        }
    }

    private var page: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                switch name {
                case "theme":
                    ThemeGallery()
                case "biometrics":
                    BiometricsGallery()
                case "pills":
                    PillsGallery(words: words)
                case "chips":
                    ChipsGallery(words: words)
                case "ring":
                    RingGallery()
                case "coinbar":
                    CoinBarGallery(words: words)
                default:
                    Text(verbatim: "gallery-\(name)?")
                        .fobFont(.headline)
                        .foregroundStyle(colors.red)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(16)
        }
        .background(colors.paper.ignoresSafeArea())
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("gallery")
    }
}

/// gallery-theme: a swatch of every colour role, then the type ramp.
private struct ThemeGallery: View {
    @Environment(\.fobColors) private var colors

    private let columns = [GridItem(.adaptive(minimum: 64), spacing: 8, alignment: .top)]

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(verbatim: "colours")
                .fobFont(.sectionLabel)
                .foregroundStyle(colors.muted)
            LazyVGrid(columns: columns, alignment: .leading, spacing: 8) {
                ForEach(colors.roles) { role in
                    Swatch(role: role)
                }
            }
        }
        VStack(alignment: .leading, spacing: 6) {
            Text(verbatim: "type")
                .fobFont(.sectionLabel)
                .foregroundStyle(colors.muted)
            VStack(alignment: .leading, spacing: 4) {
                ForEach(FobTextStyle.allCases) { style in
                    Text(verbatim: style.cssName)
                        .fobFont(style)
                        .foregroundStyle(colors.ink)
                        .lineLimit(1)
                        .accessibilityIdentifier("type-\(style.rawValue)")
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(16)
            .background(colors.card, in: RoundedRectangle(cornerRadius: 22, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 22, style: .continuous).strokeBorder(colors.line, lineWidth: 1))
        }
    }
}

private struct Swatch: View {
    let role: FobColorRole
    @Environment(\.fobColors) private var colors

    var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            RoundedRectangle(cornerRadius: 8, style: .continuous)
                .fill(role.color)
                .frame(height: 30)
                .overlay(
                    RoundedRectangle(cornerRadius: 8, style: .continuous)
                        .strokeBorder(colors.line, lineWidth: 1)
                )
            Text(verbatim: role.name)
                .font(.caption2)
                .foregroundStyle(colors.muted)
                .lineLimit(1)
                .minimumScaleFactor(0.6)
        }
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("swatch-\(role.name)")
    }
}

/// gallery-biometrics (CORE-040): the buttons that pay, as the phone says
/// it ("Pay with Face ID") and as the Mac does ("Send with Touch ID"), and
/// the passphrase fallback. The words are fob-words'.
private struct BiometricsGallery: View {
    @Environment(\.fobColors) private var colors

    var body: some View {
        let phone = labels(device: "iPhone", biometry: .faceId).confirm
        let mac = labels(device: "Mac", biometry: .touchId).confirm
        VStack(spacing: 16) {
            PillButton(phone.payWith, systemImage: BiometricKind.faceID.symbol) {}
                .accessibilityIdentifier("pay-biometric")
            PillButton(mac.sendWith, systemImage: BiometricKind.touchID.symbol) {}
                .accessibilityIdentifier("send-biometric")
            Text(phone.usePassphrase)
                .fobFont(.headline)
                .foregroundStyle(colors.orangeText)
                .frame(minHeight: 44)
                .accessibilityIdentifier("use-passphrase")
        }
        .frame(maxWidth: .infinity)
    }
}
