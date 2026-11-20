// AppModel as the iPhone's tab bar binds it (IPH-001): picking a tab shows
// that tab's own screen, and each tab's NavigationStack holds AppModel's
// `path` while its tab is the one shown (nothing pushed on the others).

import FobCore

extension AppModel {
    /// The tab bar's selection: a new tab opens with nothing pushed on it.
    var selectedTab: AppTab {
        get { tab }
        set { if newValue != tab { select(newValue) } }
    }

    var walletPath: [Route] {
        get { path(on: .wallet) }
        set { setPath(newValue, on: .wallet) }
    }

    var contactsPath: [Route] {
        get { path(on: .contacts) }
        set { setPath(newValue, on: .contacts) }
    }

    var settingsPath: [Route] {
        get { path(on: .settings) }
        set { setPath(newValue, on: .settings) }
    }

    private func path(on tab: AppTab) -> [Route] {
        self.tab == tab ? path : []
    }

    private func setPath(_ newPath: [Route], on tab: AppTab) {
        if self.tab == tab { path = newPath }
    }
}
