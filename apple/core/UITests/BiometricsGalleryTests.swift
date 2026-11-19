// CORE-040: gallery-biometrics shows "Pay with Face ID" and "Send with
// Touch ID", in OLED and in Light.

import XCTest

final class BiometricsGalleryTests: GalleryTestCase {
    @MainActor
    func testBiometricsOLED() throws {
        check(shoot("biometrics", theme: "oled", task: "CORE-040"))
    }

    @MainActor
    func testBiometricsLight() throws {
        check(shoot("biometrics", theme: "light", task: "CORE-040"))
    }

    @MainActor
    private func check(_ app: XCUIApplication) {
        XCTAssertTrue(app.descendants(matching: .any)["pay-biometric"].firstMatch.exists, "no Pay with Face ID")
        XCTAssertTrue(app.descendants(matching: .any)["send-biometric"].firstMatch.exists, "no Send with Touch ID")
    }
}
