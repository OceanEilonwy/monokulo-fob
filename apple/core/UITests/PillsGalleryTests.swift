// CORE-021: gallery-pills shows Home's Receive / Send pair on the phone and
// the Mac, Review and Paste, and a pressed and a disabled pill of each style,
// in OLED and in Light.

import XCTest

final class PillsGalleryTests: GalleryTestCase {
    @MainActor
    func testPillsOLED() throws {
        check(shoot("pills", theme: "oled", task: "CORE-021"))
    }

    @MainActor
    func testPillsLight() throws {
        check(shoot("pills", theme: "light", task: "CORE-021"))
    }

    @MainActor
    private func check(_ app: XCUIApplication) {
        XCTAssertEqual(app.buttons.matching(identifier: "receive-button").count, 2, "Receive on the phone and the Mac")
        XCTAssertEqual(app.buttons.matching(identifier: "send-button").count, 2, "Send on the phone and the Mac")
        XCTAssertTrue(app.buttons["Review"].exists, "no Review")
        XCTAssertTrue(app.buttons["Paste"].exists, "no Paste")
        XCTAssertGreaterThanOrEqual(app.buttons["Receive"].firstMatch.frame.height, 44, "Receive under 44pt")
        let disabled = element(app, "pills-disabled").buttons
        XCTAssertGreaterThan(disabled.count, 0, "no disabled pills")
        XCTAssertFalse(disabled.firstMatch.isEnabled, "the disabled pill is enabled")
    }
}
