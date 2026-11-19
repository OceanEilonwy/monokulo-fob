// Where the app is (CORE-011): a tab and the pages pushed on it. iPhone binds
// `tab` to its tab bar and `path` to the tab's NavigationStack; the iPad and
// the Mac show the same path their own way (a form sheet, pages pushed in
// place). `-FobScene <name>` opens the scene's screen directly, with the
// pages under it that lead there, so Back works in screenshots too.

import Foundation

/// The iPhone's tabs: Wallet, Contacts, Settings. The iPad and the Mac have
/// one screen and open Contacts and Settings as pages.
public enum AppTab: String, Sendable, Hashable, CaseIterable {
    case wallet, contacts, settings
}

/// A screen the app shows.
public enum Route: String, Sendable, Hashable, CaseIterable {
    case wallet, receive, send, sendForm, confirm, payment, contacts, settings
    case welcome, backup, catchup, coins, split, payWho

    /// The scene `name`'s screen: "home" and "home-syncing" open Wallet,
    /// "send-form" the form, "confirm-joel" Confirm. None for a name that
    /// isn't a scene (a gallery's, say).
    public init?(scene name: String) {
        switch name {
        case "home", "home-syncing": self = .wallet
        case "receive": self = .receive
        case "send": self = .send
        case "send-form": self = .sendForm
        case "confirm", "confirm-joel": self = .confirm
        case "pay-who": self = .payWho
        case "payment": self = .payment
        case "contacts": self = .contacts
        case "settings": self = .settings
        case "welcome": self = .welcome
        case "backup": self = .backup
        case "catchup": self = .catchup
        case "coins": self = .coins
        case "split": self = .split
        default: return nil
        }
    }

    /// The tab this screen is under.
    public var tab: AppTab {
        switch self {
        case .contacts: .contacts
        case .settings, .backup: .settings
        default: .wallet
        }
    }

    /// A tab's own screen, not pushed.
    public var isTabRoot: Bool { self == .wallet || self == .contacts || self == .settings }

    /// The pages pushed on its tab to reach this screen, this one last;
    /// empty for a tab's own screen. Confirm comes after the form (paying a
    /// contact, as confirm-joel does; a scanned code skips the form, which
    /// the app does by pushing Confirm on Send itself).
    public var stack: [Route] {
        switch self {
        case .wallet, .contacts, .settings: []
        case .sendForm: [.send, .sendForm]
        case .payWho: [.send, .sendForm, .payWho]
        case .confirm: [.send, .sendForm, .confirm]
        case .split: [.coins, .split]
        default: [self]
        }
    }
}

extension Route {
    /// The scene's screen with the pages that lead to it; Wallet for none.
    static func start(scene name: String?) -> (tab: AppTab, path: [Route]) {
        guard let route = name.flatMap(Route.init(scene:)) else { return (.wallet, []) }
        return (route.tab, route.stack)
    }
}
