// CORE-028: gallery-ring shows the confirmation ring in Coins' not-ready row
// (3 of 10), at both sizes from none to all, and beside an Activity line, in
// OLED and in Light.

import XCTest

final class RingGalleryTests: GalleryTestCase {
    @MainActor
    func testRingOLED() throws {
        check(shoot("ring", theme: "oled", task: "CORE-028"))
    }

    @MainActor
    func testRingLight() throws {
        check(shoot("ring", theme: "light", task: "CORE-028"))
    }

    @MainActor
    private func check(_ app: XCUIApplication) {
        let rings = app.descendants(matching: .any).matching(identifier: "confirmation-ring")
        XCTAssertGreaterThanOrEqual(rings.count, 10, "rings missing: Coins' row, eight along the way and the Activity line's")
        XCTAssertTrue(rings.matching(NSPredicate(format: "label == %@", "3 of 10 confirmations")).firstMatch.exists,
                      "no ring reads \"3 of 10 confirmations\"")
    }
}
