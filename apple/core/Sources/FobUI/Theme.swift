// Fob's colour roles, mirrored by name from Android's Theme.kt
// (android/app/src/main/kotlin/com/monokulo/fob/ui/Theme.kt): FobColors with
// the OLED, Dark and Light palettes, plus the fixed orange.

import FobCore
import SwiftUI

public struct FobColors: Sendable {
    public let oled: Bool
    public let paper, card, sunken, onPaper, line, ink, muted, outline, orangeText: Color
    public let green, greenDot, greenFill, onGreen: Color
    public let highlight, warning, selectedFill, addressMark, onAddressMark: Color
    public let red, onRed, track, scrim: Color
    public let inverse, onInverse, inverseMuted, inverseAccent, inverseGreen: Color

    /// Fob's orange, the tint in every theme.
    public static let orange = Color(argb: 0xFFFF6600)
    public static let onOrange = Color(argb: 0xFF111111)

    public static let light = FobColors(
        oled: false,
        paper: Color(argb: 0xFFF4F1EA), card: Color(argb: 0xFFFFFFFF), sunken: Color(argb: 0xFFEFEAE0),
        onPaper: Color(argb: 0xFFE4DDD0), line: Color(argb: 0xFFE2DCCF), ink: Color(argb: 0xFF1A1917),
        muted: Color(argb: 0xFF5A564C), outline: Color(argb: 0xFF8C8577), orangeText: Color(argb: 0xFFB34700),
        green: Color(argb: 0xFF1F7A45), greenDot: Color(argb: 0xFF1F8A4C), greenFill: Color(argb: 0xFF4CC27A),
        onGreen: Color(argb: 0xFF1A1917), highlight: Color(argb: 0xFFFBF3E8), warning: Color(argb: 0xFFF7D6B4),
        selectedFill: Color(argb: 0xFFFBE3CF), addressMark: Color(argb: 0xFFFBE3CC), onAddressMark: Color(argb: 0xFF7A2F00),
        red: Color(argb: 0xFFB3261E), onRed: Color(argb: 0xFFFFFFFF), track: Color(argb: 0xFFD8D1C3),
        scrim: Color(argb: 0x731A1917), inverse: Color(argb: 0xFF1A1917), onInverse: Color(argb: 0xFFF4F1EA),
        inverseMuted: Color(argb: 0xFFB0A999), inverseAccent: Color(argb: 0xFFFF8F45), inverseGreen: Color(argb: 0xFF5CC88A)
    )

    public static let dark = FobColors(
        oled: false,
        paper: Color(argb: 0xFF141311), card: Color(argb: 0xFF1F1D1A), sunken: Color(argb: 0xFF2A2824),
        onPaper: Color(argb: 0xFF2A2824), line: Color(argb: 0xFF3A3731), ink: Color(argb: 0xFFF2EEE6),
        muted: Color(argb: 0xFFB0A999), outline: Color(argb: 0xFF7A7366), orangeText: Color(argb: 0xFFFF8F45),
        green: Color(argb: 0xFF5CC88A), greenDot: Color(argb: 0xFF5CC88A), greenFill: Color(argb: 0xFF5CC88A),
        onGreen: Color(argb: 0xFF111111), highlight: Color(argb: 0xFF2E2418), warning: Color(argb: 0xFF46301A),
        selectedFill: Color(argb: 0xFF452812), addressMark: Color(argb: 0xFF4A2A12), onAddressMark: Color(argb: 0xFFFFC9A0),
        red: Color(argb: 0xFFF2B8B5), onRed: Color(argb: 0xFF601410), track: Color(argb: 0xFF3A3731),
        scrim: Color(argb: 0x99080706), inverse: Color(argb: 0xFFF2EEE6), onInverse: Color(argb: 0xFF1A1917),
        inverseMuted: Color(argb: 0xFF5A564C), inverseAccent: Color(argb: 0xFFB34700), inverseGreen: Color(argb: 0xFF1F7A45)
    )

    public static let oledBlack = FobColors(
        oled: true,
        paper: Color(argb: 0xFF000000), card: Color(argb: 0xFF0C0B0A), sunken: Color(argb: 0xFF181613),
        onPaper: Color(argb: 0xFF181613), line: Color(argb: 0xFF2A2723), ink: Color(argb: 0xFFF2EEE6),
        muted: Color(argb: 0xFFB0A999), outline: Color(argb: 0xFF6B655A), orangeText: Color(argb: 0xFFFF8F45),
        green: Color(argb: 0xFF5CC88A), greenDot: Color(argb: 0xFF5CC88A), greenFill: Color(argb: 0xFF5CC88A),
        onGreen: Color(argb: 0xFF111111), highlight: Color(argb: 0xFF1C1610), warning: Color(argb: 0xFF3A2814),
        selectedFill: Color(argb: 0xFF3A210F), addressMark: Color(argb: 0xFF3F230E), onAddressMark: Color(argb: 0xFFFFC9A0),
        red: Color(argb: 0xFFF2B8B5), onRed: Color(argb: 0xFF601410), track: Color(argb: 0xFF2A2723),
        scrim: Color(argb: 0xB3000000), inverse: Color(argb: 0xFFF2EEE6), onInverse: Color(argb: 0xFF1A1917),
        inverseMuted: Color(argb: 0xFF5A564C), inverseAccent: Color(argb: 0xFFB34700), inverseGreen: Color(argb: 0xFF1F7A45)
    )

    public static func of(_ theme: ThemeName) -> FobColors {
        switch theme {
        case .oled: oledBlack
        case .dark: dark
        case .light: light
        }
    }

    /// The system appearance the palette sits on.
    public var colorScheme: ColorScheme { self.paper == FobColors.light.paper ? .light : .dark }
}

extension Color {
    /// 0xAARRGGBB, as Theme.kt writes them.
    public init(argb: UInt32) {
        self.init(
            .sRGB,
            red: Double((argb >> 16) & 0xFF) / 255,
            green: Double((argb >> 8) & 0xFF) / 255,
            blue: Double(argb & 0xFF) / 255,
            opacity: Double((argb >> 24) & 0xFF) / 255
        )
    }
}

private struct FobColorsKey: EnvironmentKey {
    static let defaultValue: FobColors = .oledBlack
}

extension EnvironmentValues {
    /// The palette every Fob view draws with.
    public var fobColors: FobColors {
        get { self[FobColorsKey.self] }
        set { self[FobColorsKey.self] = newValue }
    }
}

extension View {
    /// Draws this view and everything in it with `theme`'s colours.
    public func fobTheme(_ theme: ThemeName) -> some View {
        let colors = FobColors.of(theme)
        return environment(\.fobColors, colors)
            .preferredColorScheme(colors.colorScheme)
            .tint(FobColors.orange)
    }
}

/// One of FobColors' roles, named as Theme.kt names it (the gallery's swatches).
public struct FobColorRole: Sendable, Identifiable {
    public let name: String
    public let color: Color
    public var id: String { name }
}

extension FobColors {
    /// Every role, in Theme.kt's order, then the fixed orange.
    public var roles: [FobColorRole] {
        let named: [(String, Color)] = [
            ("paper", paper), ("card", card), ("sunken", sunken), ("onPaper", onPaper),
            ("line", line), ("ink", ink), ("muted", muted), ("outline", outline),
            ("orangeText", orangeText), ("green", green), ("greenDot", greenDot),
            ("greenFill", greenFill), ("onGreen", onGreen), ("highlight", highlight),
            ("warning", warning), ("selectedFill", selectedFill), ("addressMark", addressMark),
            ("onAddressMark", onAddressMark), ("red", red), ("onRed", onRed), ("track", track),
            ("scrim", scrim), ("inverse", inverse), ("onInverse", onInverse),
            ("inverseMuted", inverseMuted), ("inverseAccent", inverseAccent),
            ("inverseGreen", inverseGreen), ("orange", FobColors.orange), ("onOrange", FobColors.onOrange),
        ]
        return named.map { FobColorRole(name: $0.0, color: $0.1) }
    }
}
