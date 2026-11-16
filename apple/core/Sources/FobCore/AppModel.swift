// The app's model. For now it only knows how to open a scene: launched with
// `-FobScene <name>` (docs/apple/plan.md §5) it draws a demo wallet from
// crates/fob-scenes, with no node, no clock and no randomness, so every
// screenshot shows the same data on iPhone, iPad and macOS.

import Foundation
import Observation

/// The colours the app draws in. OLED is the default drawn.
public enum ThemeName: String, Sendable, CaseIterable {
    case oled, dark, light
}

@MainActor
@Observable
public final class AppModel {
    /// The scene asked for with `-FobScene`, if any.
    public let sceneName: String?
    /// The demo wallet of that scene; none without one (or for a name that
    /// isn't a scene).
    public let wallet: SceneWallet?
    /// `-FobTheme oled|dark|light`, OLED unless told.
    public let theme: ThemeName

    public init(arguments: [String] = ProcessInfo.processInfo.arguments) {
        func value(_ flag: String) -> String? {
            guard let i = arguments.firstIndex(of: flag), i + 1 < arguments.count else { return nil }
            return arguments[i + 1]
        }
        let name = value("-FobScene")
        sceneName = name
        wallet = name.flatMap { scene(name: $0) }
        theme = value("-FobTheme").flatMap(ThemeName.init(rawValue:)) ?? .oled
    }

    /// Every scene fob-scenes knows.
    public static var sceneNames: [String] { FobCore.sceneNames() }
}
