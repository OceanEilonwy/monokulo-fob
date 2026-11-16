// Manrope (the same file Android ships, android/app/src/main/res/font/
// manrope.ttf) for balances and titles; SF Pro with Dynamic Type for the rest.

import CoreText
import SwiftUI

public enum FobFonts {
    private static let registered: Bool = {
        guard let url = Bundle.module.url(forResource: "Manrope", withExtension: "ttf") else { return false }
        return CTFontManagerRegisterFontsForURL(url as CFURL, .process, nil)
    }()

    /// Makes Manrope available to the process; call once as the app starts.
    @discardableResult
    public static func register() -> Bool { registered }

    /// Manrope ExtraBold at `size`, scaled with Dynamic Type like `style`.
    public static func manrope(_ size: CGFloat, relativeTo style: Font.TextStyle = .largeTitle) -> Font {
        _ = registered
        return .custom("Manrope", size: size, relativeTo: style).weight(.heavy)
    }
}
