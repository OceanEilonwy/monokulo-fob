// MAC-001, the window shell (MacHome): the one window opens Wallet under its
// title bar (Contacts, the wallet menu, Refresh, Settings) and pushes pages
// in place. Screenshots "MAC-001--oled" (the window, which is what the board
// draws) and "MAC-001-screen--oled" (the whole screen), turned into PNGs by
// apple/ci/extract-screenshots.sh.

import XCTest

final class MacWindowTests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    @MainActor
    func testWindowShell() throws {
        let app = XCUIApplication()
        app.launchArguments = ["-FobScene", "home", "-FobTheme", "oled"]
        app.launch()

        let window = element(app, "mac-window")
        XCTAssertTrue(window.waitForExistence(timeout: 60), "the window never showed")
        let balance = element(app, "balance")
        XCTAssertTrue(balance.waitForExistence(timeout: 10), "the balance never showed")
        // Let the first frame settle.
        Thread.sleep(forTimeInterval: 1)
        // The window alone, as the board draws it (a runner without screen
        // recording rights may give an empty screen, while the window comes
        // from the app), and the whole screen.
        attach(app.windows.firstMatch.screenshot(), "MAC-001--oled")
        attach(XCUIScreen.main.screenshot(), "MAC-001-screen--oled")

        XCTAssertEqual(text(of: balance), "0.4182")
        for id in ["wallet-chip", "toolbar-contacts", "toolbar-refresh", "toolbar-settings"] {
            XCTAssertTrue(element(app, id).exists, "\(id) is missing from the title bar")
        }

        // Contacts is pushed in place, in the same window, and Back returns.
        element(app, "toolbar-contacts").click()
        XCTAssertTrue(element(app, "mac-page-contacts").waitForExistence(timeout: 10),
                      "Contacts wasn't pushed")
        XCTAssertEqual(app.windows.count, 1)
        element(app, "back").click()
        XCTAssertTrue(balance.waitForExistence(timeout: 10), "Back didn't return to Wallet")
    }

    /// The element with accessibility identifier `id`, whatever it is.
    @MainActor
    private func element(_ app: XCUIApplication, _ id: String) -> XCUIElement {
        app.descendants(matching: .any)[id].firstMatch
    }

    /// A text's words: its label on iOS, its value on macOS.
    @MainActor
    private func text(of element: XCUIElement) -> String {
        element.label.isEmpty ? (element.value as? String ?? "") : element.label
    }

    @MainActor
    private func attach(_ screenshot: XCUIScreenshot, _ name: String) {
        let attachment = XCTAttachment(screenshot: screenshot)
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }
}
