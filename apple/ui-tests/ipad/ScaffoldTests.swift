// The scaffold's proof for ipad: the app opens the "home" scene in OLED and
// shows its balance. The screenshot is the xcresult attachment
// "T-SCAFFOLD-ipad--oled" (apple/ci/extract-screenshots.sh, docs/apple/plan.md §5).

import XCTest

final class IPadScaffoldTests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    @MainActor
    func testWalletPlaceholder() throws {
        XCUIDevice.shared.orientation = .landscapeLeft
        let app = XCUIApplication()
        app.launchArguments = ["-FobScene", "home", "-FobTheme", "oled"]
        app.launch()

        let balance = app.staticTexts["balance"]
        XCTAssertTrue(balance.waitForExistence(timeout: 60), "the balance never showed")
        // Let the first frame settle.
        Thread.sleep(forTimeInterval: 1)
        attach(XCUIScreen.main.screenshot(), "T-SCAFFOLD-ipad--oled")
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
