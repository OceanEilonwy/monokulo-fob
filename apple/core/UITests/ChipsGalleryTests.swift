// CORE-022: gallery-chips shows the sync chip synced, catching up and
// offline, the Stagenet chip and the wallet switcher, at the phone's size and
// the Mac's, in OLED and in Light.

import XCTest

final class ChipsGalleryTests: GalleryTestCase {
    @MainActor
    func testChipsOLED() throws {
        check(shoot("chips", theme: "oled", task: "CORE-022"))
    }

    @MainActor
    func testChipsLight() throws {
        check(shoot("chips", theme: "light", task: "CORE-022"))
    }

    @MainActor
    private func check(_ app: XCUIApplication) {
        XCTAssertGreaterThanOrEqual(app.buttons.matching(identifier: "sync-chip").count, 7, "sync chips missing: Main's, three looks at the phone's size and three at the Mac's")
        XCTAssertTrue(element(app, "stagenet-chip").exists, "no Stagenet chip")
        XCTAssertTrue(app.buttons["Switch wallet"].exists, "no wallet switcher")
        XCTAssertTrue(app.buttons["Wallets"].exists, "no Mac wallet menu")
    }
}
