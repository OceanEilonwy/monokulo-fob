// CORE-027: gallery-coinbar shows Home's settling card (0.3870 ready,
// 0.0312 not yet, 09:32, the coins bar) and the coins bar alone, in OLED and
// in Light.

import XCTest

final class CoinBarGalleryTests: GalleryTestCase {
    @MainActor
    func testCoinBarOLED() throws {
        check(shoot("coinbar", theme: "oled", task: "CORE-027"))
    }

    @MainActor
    func testCoinBarLight() throws {
        check(shoot("coinbar", theme: "light", task: "CORE-027"))
    }

    @MainActor
    private func check(_ app: XCUIApplication) {
        let card = element(app, "settling-card")
        XCTAssertTrue(card.exists, "no settling card")
        XCTAssertEqual(card.label, "0.3870 XMR ready to spend, 0.0312 XMR not yet, all ready about 09:32")
        XCTAssertGreaterThan(card.frame.height, 40, "the settling card is squashed")
        let bar = element(app, "coin-bar")
        XCTAssertTrue(bar.exists, "no coins bar")
        XCTAssertFalse(bar.label.isEmpty, "the coins bar isn't read out")
    }
}
