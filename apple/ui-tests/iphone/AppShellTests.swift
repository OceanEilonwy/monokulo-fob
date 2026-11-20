// IPH-001: the app shell. `-FobScene home` opens the Wallet tab under the
// tab bar (Wallet, Contacts, Settings, Wallet selected in orange) on the
// OLED black page. The screenshot is the xcresult attachment
// "IPH-001--oled" (apple/ci/extract-screenshots.sh).

import XCTest

final class AppShellTests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    @MainActor
    func testTabBarOLED() throws {
        let app = XCUIApplication()
        app.launchArguments = ["-FobScene", "home", "-FobTheme", "oled"]
        app.launch()

        let walletTab = app.descendants(matching: .any)["tab-wallet"].firstMatch
        let walletByName = app.tabBars.buttons["Wallet"]
        XCTAssertTrue(walletTab.waitForExistence(timeout: 60) || walletByName.exists, "the tab bar never showed")
        XCTAssertTrue(app.staticTexts["balance"].waitForExistence(timeout: 10), "the Wallet tab isn't shown")

        // Let the first frame settle.
        Thread.sleep(forTimeInterval: 1)
        attach(XCUIScreen.main.screenshot(), "IPH-001--oled")

        XCTAssertTrue(walletTab.exists, "the Wallet tab has no id tab-wallet")
        XCTAssertTrue(walletByName.isSelected, "Wallet isn't selected")
        XCTAssertTrue(app.tabBars.buttons["Contacts"].exists, "no Contacts tab")
        XCTAssertTrue(app.tabBars.buttons["Settings"].exists, "no Settings tab")
    }

    @MainActor
    private func attach(_ screenshot: XCUIScreenshot, _ name: String) {
        let attachment = XCTAttachment(screenshot: screenshot)
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }
}
