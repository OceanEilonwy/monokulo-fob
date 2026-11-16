// The app's model. Launched with `-FobScene <name>` (docs/apple/plan.md §5)
// it draws a demo wallet from crates/fob-scenes, with no node, no clock and
// no randomness, so every screenshot shows the same data on iPhone, iPad and
// macOS. Its wallet comes from a WalletSource; the screens read `state` (or
// the shortcuts below) and `labels` for the words around it.

import Foundation
import Observation
#if canImport(UIKit)
import UIKit
#endif

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
    /// Every screen's own words, for this device.
    public let labels: Labels

    /// Where the wallet comes from; none while no wallet is open.
    public private(set) var source: (any WalletSource)?
    /// What Home shows now.
    public private(set) var state: WalletState = .empty

    public init(arguments: [String] = ProcessInfo.processInfo.arguments) {
        func value(_ flag: String) -> String? {
            guard let i = arguments.firstIndex(of: flag), i + 1 < arguments.count else { return nil }
            return arguments[i + 1]
        }
        let name = value("-FobScene")
        let scene = name.flatMap(SceneSource.init(name:))
        sceneName = name
        wallet = scene?.wallet
        theme = value("-FobTheme").flatMap(ThemeName.init(rawValue:)) ?? .oled
        labels = FobCore.labels(device: Self.deviceName, biometry: Self.defaultBiometry)
        open(scene)
    }

    /// Shows `source`'s wallet and follows its changes; none closes it.
    public func open(_ source: (any WalletSource)?) {
        self.source = source
        state = source?.state ?? .empty
        source?.observe { [weak self, weak source] state in
            guard let self, let source, self.source === source else { return }
            self.state = state
        }
    }

    /// A wallet is open.
    public var hasWallet: Bool { source != nil }

    // Home's parts, as the screens name them.
    public var walletName: String { state.walletName }
    public var network: WalletNetwork { state.network }
    public var balance: HomeBalance { state.balance }
    public var syncChip: SyncChip { state.syncChip }
    public var activity: [ActivitySection] { state.activity }
    public var notReadyCoins: [NotReadyCoin] { state.notReadyCoins }

    /// Every scene fob-scenes knows.
    public static var sceneNames: [String] { FobCore.sceneNames() }

    /// "iPhone", "iPad" or "Mac", for the words that name the device.
    static var deviceName: String {
        #if os(macOS)
        "Mac"
        #else
        UIDevice.current.userInterfaceIdiom == .pad ? "iPad" : "iPhone"
        #endif
    }

    /// How the device checks it's you until LocalAuthentication says
    /// (CORE-040): Face ID on iPhone, Touch ID on iPad and the Mac.
    static var defaultBiometry: Biometry {
        #if os(macOS)
        .touchId
        #else
        UIDevice.current.userInterfaceIdiom == .phone ? .faceId : .touchId
        #endif
    }
}
