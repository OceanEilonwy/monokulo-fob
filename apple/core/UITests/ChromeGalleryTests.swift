// CORE-030: gallery-chrome shows the tab bar (Wallet selected, in orange),
// Wallet's toolbar with Scan a payment code, the Mac's title-bar icons, then
// Coins pushed with "‹ Wallet", in OLED and in Light.

import XCTest

final class ChromeGalleryTests: GalleryTestCase {
    @MainActor
    func testChromeOLED() throws {
        check(shoot("chrome", theme: "oled", task: "CORE-030"), theme: "oled")
    }

    @MainActor
    func testChromeLight() throws {
        check(shoot("chrome", theme: "light", task: "CORE-030"), theme: "light")
    }

    @MainActor
    private func check(_ app: XCUIApplication, theme: String) {
        let wallet = app.tabBars.buttons["Wallet"]
        XCTAssertTrue(wallet.exists, "no Wallet tab")
        XCTAssertTrue(wallet.isSelected, "Wallet isn't selected")
        XCTAssertTrue(app.tabBars.buttons["Contacts"].exists, "no Contacts tab")
        XCTAssertTrue(app.tabBars.buttons["Settings"].exists, "no Settings tab")
        XCTAssertTrue(app.buttons["Scan a payment code"].exists, "no Scan a payment code")
        XCTAssertTrue(app.buttons["Refresh"].exists, "no Refresh")
        let switcher = app.buttons["Switch wallet"]
        XCTAssertTrue(switcher.exists, "no wallet switcher")
        // The mark, "Spending" and the chevron: far wider than mark + chevron.
        XCTAssertGreaterThan(switcher.frame.width, 90, "the wallet chip lost its name")

        element(app, "open-coins").tap()
        let title = element(app, "coins-title")
        XCTAssertTrue(title.waitForExistence(timeout: 10), "Coins wasn't pushed")
        let back = element(app, "back")
        XCTAssertTrue(back.waitForExistence(timeout: 5), "no back button")
        XCTAssertTrue(app.staticTexts["Wallet"].exists || back.label.contains("Wallet"), "the back button doesn't say Wallet")
        // "‹ Wallet" on one line, not wrapped.
        XCTAssertLessThan(back.frame.height, 50, "the back button's name wrapped")
        XCTAssertGreaterThan(back.frame.width, 60, "the back button's name was squeezed")
        // Let the push finish.
        Thread.sleep(forTimeInterval: 1)
        attach("CORE-030-pushed--\(theme)")

        back.tap()
        XCTAssertTrue(element(app, "open-coins").waitForExistence(timeout: 10), "back didn't go back")
    }
}
