// The iPhone's tab bar (IPH-001): Wallet, Contacts, Settings, the selected
// tab in orange text, each tab a NavigationStack. AppModel says where the
// app is: `tab` selects the tab and `path` is the pages pushed on it
// (Receive, Send, Coins and the rest, full-screen), so `-FobScene` opens its
// screen with Back to the pages under it.

import FobCore
import FobUI
import SwiftUI

struct PhoneRoot: View {
    @Bindable var model: AppModel

    var body: some View {
        TabView(selection: $model.selectedTab) {
            ForEach(AppTab.allCases, id: \.self) { tab in
                Tab(tab.title(model.labels.tabs), systemImage: tab.symbol, value: tab) {
                    TabStack(model: model, tab: tab)
                }
                .accessibilityIdentifier(tab.accessibilityID)
            }
        }
        .fobTabBar()
    }
}

/// One tab: its own screen, with the pages AppModel pushes on it.
private struct TabStack: View {
    @Bindable var model: AppModel
    let tab: AppTab

    var body: some View {
        NavigationStack(path: path) {
            TabScreen(model: model, tab: tab)
                .navigationDestination(for: Route.self) { route in
                    RouteScreen(model: model, route: route)
                }
        }
    }

    private var path: Binding<[Route]> {
        switch tab {
        case .wallet: $model.walletPath
        case .contacts: $model.contactsPath
        case .settings: $model.settingsPath
        }
    }
}

/// A tab's own screen.
private struct TabScreen: View {
    let model: AppModel
    let tab: AppTab

    var body: some View {
        switch tab {
        case .wallet: WalletScreen(model: model)
        case .contacts: PageScreen(route: .contacts)
        case .settings: PageScreen(route: .settings)
        }
    }
}

/// A page pushed on a tab.
struct RouteScreen: View {
    let model: AppModel
    let route: Route

    var body: some View {
        PageScreen(route: route)
    }
}

/// A screen not built yet: the page's paper under Fob's bars, named for
/// UI tests by its route ("page-coins").
struct PageScreen: View {
    let route: Route

    @Environment(\.fobColors) private var colors

    var body: some View {
        colors.paper
            .ignoresSafeArea()
            .accessibilityElement()
            .accessibilityIdentifier("page-\(route.rawValue)")
            .fobBars()
    }
}
