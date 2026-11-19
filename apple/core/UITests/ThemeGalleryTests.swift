// CORE-020: gallery-theme shows a swatch of every colour role and the type
// ramp, in OLED and in Light.

import XCTest

final class ThemeGalleryTests: GalleryTestCase {
    @MainActor
    func testThemeOLED() throws {
        check(shoot("theme", theme: "oled", task: "CORE-020"))
    }

    @MainActor
    func testThemeLight() throws {
        check(shoot("theme", theme: "light", task: "CORE-020"))
    }

    @MainActor
    private func check(_ app: XCUIApplication) {
        for role in ["paper", "card", "line", "ink", "orangeText", "orange"] {
            XCTAssertTrue(app.descendants(matching: .any)["swatch-\(role)"].firstMatch.exists, "no \(role) swatch")
        }
        XCTAssertTrue(app.descendants(matching: .any)["type-balance"].firstMatch.exists, "no type ramp")
    }
}
