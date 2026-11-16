// The scaffold's proof for macos: the app opens the "home" scene in OLED and
// shows its balance. The screenshot is the xcresult attachment
// "T-SCAFFOLD-macos--oled" (apple/ci/extract-screenshots.sh, docs/apple/plan.md §5).

import XCTest

final class MacScaffoldTests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    @MainActor
    func testWalletPlaceholder() throws {
        let app = XCUIApplication()
        app.launchArguments = ["-FobScene", "home", "-FobTheme", "oled"]
        app.launch()

        let balance = app.staticTexts["balance"]
        XCTAssertTrue(balance.waitForExistence(timeout: 60), "the balance never showed")
        // Let the first frame settle.
        Thread.sleep(forTimeInterval: 1)
        // The whole screen, as on iOS; and the window alone, which is what the
        // design boards draw (a runner without screen recording rights may
        // give an empty screen, while the window comes from the app).
        attach(XCUIScreen.main.screenshot(), "T-SCAFFOLD-macos--oled-screen")
        attach(app.windows.firstMatch.screenshot(), "T-SCAFFOLD-macos--oled")
        XCTAssertEqual(balance.label, "0.4182")
        XCTAssertEqual(app.staticTexts["fiat"].label, "A$148.20")
    }

    @MainActor
    private func attach(_ screenshot: XCUIScreenshot, _ name: String) {
        let attachment = XCTAttachment(screenshot: screenshot)
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }
}
