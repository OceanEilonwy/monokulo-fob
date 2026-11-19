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
    /// Face ID / Touch ID before a payment (CORE-040); a stub in scene
    /// mode, answering as `-FobBiometrics pass|fail|unavailable` says.
    public let biometrics: Biometrics

    /// Where the wallet comes from; none while no wallet is open.
    public private(set) var source: (any WalletSource)?
    /// What Home shows now.
    public private(set) var state: WalletState = .empty

    /// The tab shown (the iPhone's tab bar binds to it).
    public var tab: AppTab
    /// The pages pushed on that tab, the top one last (a NavigationStack's
    /// path binds to it).
    public var path: [Route]
    /// `-FobScene gallery-<name>`: the FobUI gallery to show in place of the
    /// app, by its name ("theme"); none otherwise.
    public let gallery: String?

    /// Receive's model for the open wallet (CORE-014).
    public private(set) var receive: ReceiveModel?
    /// Contacts' model for the open wallet (CORE-012).
    public private(set) var contacts: ContactsModel?
    /// Coins' model for the open wallet (CORE-013).
    public private(set) var coins: CoinsModel?
    /// Split up's model for the open wallet (CORE-013).
    public private(set) var split: SplitModel?

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
        let start = Route.start(scene: name)
        tab = start.tab
        path = start.path
        gallery = name.flatMap { $0.hasPrefix("gallery-") ? String($0.dropFirst("gallery-".count)) : nil }
        biometrics = Biometrics(arguments: arguments, sceneMode: name != nil)
        labels = FobCore.labels(device: Self.deviceName, biometry: biometrics.kind.biometry ?? Self.defaultBiometry)
        open(scene)
        receive = name.flatMap(ReceiveModel.init(scene:))
        contacts = name.flatMap(ContactsModel.init(scene:))
        coins = name.flatMap(CoinsModel.init(scene:))
        split = name.flatMap(SplitModel.init(scene:))
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

    /// The screen on top: the last page pushed, or the tab's own.
    public var route: Route { path.last ?? Self.root(of: tab) }

    /// Shows `route`: a tab's own screen selects that tab; any other is
    /// pushed on the tab it belongs to (switching to it first).
    public func show(_ route: Route) {
        if route.isTabRoot {
            select(route.tab)
            return
        }
        if route.tab != tab {
            tab = route.tab
            path = []
        }
        if path.last != route { path.append(route) }
    }

    /// Selects `tab` with nothing pushed on it.
    public func select(_ tab: AppTab) {
        self.tab = tab
        path = []
    }

    /// Back: drops the top page.
    public func back() {
        _ = path.popLast()
    }

    /// Back to the tab's own screen.
    public func popToRoot() {
        path = []
    }

    static func root(of tab: AppTab) -> Route {
        switch tab {
        case .wallet: .wallet
        case .contacts: .contacts
        case .settings: .settings
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

    /// The device's usual kind (Face ID on iPhone, Touch ID on iPad and the
    /// Mac), for the words when biometrics are unavailable and the
    /// passphrase stands in.
    static var defaultBiometry: Biometry {
        Biometrics.deviceKind.biometry ?? .touchId
    }
}
