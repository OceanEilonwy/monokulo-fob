// The bars round Fob's screens (CORE-030): the tab bar's tint and symbols,
// the navigation bar drawn on the page's own black, the orange back button
// with the page below's name ("‹ Wallet", PhoneCoins), and the toolbar's icon
// buttons with their VoiceOver names (Main, MacHome). Every word is
// fob-words' (`labels.tabs`, `labels.chrome`).

import FobCore
import SwiftUI

// MARK: - Tab bar

extension AppTab {
    /// The tab's SF Symbol, as the boards draw it: a wallet, two people, a cog.
    public var symbol: String {
        switch self {
        case .wallet: "wallet.bifold"
        case .contacts: "person.2"
        case .settings: "gearshape"
        }
    }

    /// The tab's name in fob-words: Wallet, Contacts, Settings.
    public func title(_ labels: TabLabels) -> String {
        switch self {
        case .wallet: labels.wallet
        case .contacts: labels.contacts
        case .settings: labels.settings
        }
    }

    /// The tab's accessibility identifier: "tab-wallet".
    public var accessibilityID: String { "tab-\(rawValue)" }
}

extension View {
    /// The tab bar as Main draws it: the selected tab in orange text
    /// (#FF8F45 on OLED, #B34700 on Light). Put it on the TabView.
    public func fobTabBar() -> some View {
        modifier(FobTabBar())
    }

    /// A page's bars drawn on its own paper: the navigation bar shows the
    /// page through it, and on iPhone the tab bar sits on `paper` held by
    /// the system's hairline (OLED black). Put it on each page's root view.
    public func fobBars() -> some View {
        modifier(FobBars())
    }
}

private struct FobTabBar: ViewModifier {
    @Environment(\.fobColors) private var colors

    func body(content: Content) -> some View {
        content.tint(colors.orangeText)
    }
}

private struct FobBars: ViewModifier {
    @Environment(\.fobColors) private var colors

    func body(content: Content) -> some View {
        #if os(iOS)
        content
            .toolbarBackground(.hidden, for: .navigationBar)
            .toolbarBackground(colors.paper, for: .tabBar)
            .toolbarBackgroundVisibility(.visible, for: .tabBar)
        #else
        content
        #endif
    }
}

// MARK: - Back

/// The back button the boards draw: an orange chevron and the name of the
/// page below ("Wallet", "Coins", "Send") in 17pt orange text.
public struct FobBackButton: View {
    let title: String
    let action: @MainActor () -> Void

    @Environment(\.fobColors) private var colors

    public init(_ title: String, action: @escaping @MainActor () -> Void) {
        self.title = title
        self.action = action
    }

    public var body: some View {
        Button(action: action) {
            HStack(spacing: 4) {
                Image(systemName: "chevron.left")
                    .font(.body.weight(.semibold))
                Text(title)
                    .font(.body)
            }
            .foregroundStyle(colors.orangeText)
            .frame(minWidth: 44, minHeight: 44, alignment: .leading)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("back")
    }
}

extension View {
    /// Swaps the system back button for Fob's ("‹ Wallet"), which goes back
    /// with the environment's dismiss. The page keeps its swipe back on
    /// iPhone only where the app's NavigationStack keeps it; use this on
    /// pages the boards draw with a named back button.
    public func fobBackButton(_ title: String) -> some View {
        modifier(FobBackModifier(title: title))
    }
}

private struct FobBackModifier: ViewModifier {
    let title: String
    @Environment(\.dismiss) private var dismiss

    func body(content: Content) -> some View {
        content
            .navigationBarBackButtonHidden(true)
            .toolbar {
                ToolbarItem(placement: .navigation) {
                    FobBackButton(title) { dismiss() }
                }
                .sharedBackgroundVisibility(.hidden)
            }
    }
}

// MARK: - Toolbar icon buttons

/// The toolbar's icon buttons, each named for VoiceOver as the boards do.
public enum FobToolbarIcon: String, Sendable, CaseIterable {
    /// The phone's Scan a payment code, top right of Wallet.
    case scan
    /// The iPad's and Mac's Contacts.
    case contacts
    /// The Mac's Refresh.
    case refresh
    /// The iPad's and Mac's Settings.
    case settings

    /// Its SF Symbol.
    public var symbol: String {
        switch self {
        case .scan: "qrcode.viewfinder"
        case .contacts: "person.2"
        case .refresh: "arrow.clockwise"
        case .settings: "gearshape"
        }
    }

    /// Its VoiceOver name in fob-words: "Scan a payment code".
    public func label(_ labels: ChromeLabels) -> String {
        switch self {
        case .scan: labels.scanAPaymentCode
        case .contacts: labels.contacts
        case .refresh: labels.refresh
        case .settings: labels.settings
        }
    }

    /// Its accessibility identifier: "toolbar-scan".
    public var accessibilityID: String { "toolbar-\(rawValue)" }
}

/// One of the toolbar's icon buttons: a 36pt `onPaper` circle with an ink
/// symbol on the phone and iPad (Main's Scan), a bare 30 x 28pt muted
/// symbol in the Mac's title bar (MacHome).
public struct FobIconButton: View {
    /// How it's drawn.
    public enum Look: Sendable {
        case circle, bar
    }

    let icon: FobToolbarIcon
    let label: String
    let look: Look
    let action: @MainActor () -> Void

    @Environment(\.fobColors) private var colors

    /// `labels` gives its VoiceOver name (`model.labels.chrome`).
    public init(_ icon: FobToolbarIcon, labels: ChromeLabels, look: Look = .circle,
                action: @escaping @MainActor () -> Void) {
        self.icon = icon
        self.label = icon.label(labels)
        self.look = look
        self.action = action
    }

    public var body: some View {
        Button(action: action) {
            switch look {
            case .circle:
                Image(systemName: icon.symbol)
                    .font(.system(size: 18, weight: .medium))
                    .foregroundStyle(colors.ink)
                    .frame(width: 36, height: 36)
                    .background(colors.onPaper, in: Circle())
                    .frame(width: 44, height: 44)
            case .bar:
                Image(systemName: icon.symbol)
                    .font(.system(size: 14, weight: .medium))
                    .foregroundStyle(colors.muted)
                    .frame(width: 30, height: 28)
            }
        }
        .buttonStyle(.plain)
        .contentShape(Rectangle())
        .accessibilityLabel(label)
        .accessibilityIdentifier(icon.accessibilityID)
    }
}
