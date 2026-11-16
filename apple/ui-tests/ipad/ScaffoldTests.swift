// The scaffold's proof for ipad: the app opens the "home" scene in OLED and
// shows its balance. The screenshot is the xcresult attachment
// "T-SCAFFOLD-ipad--oled" (apple/ci/extract-screenshots.sh, docs/apple/plan.md §5).

import UIKit
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
        XCTAssertEqual(text(of: balance), "0.4182")
        XCTAssertEqual(text(of: app.staticTexts["fiat"]), "A$148.20")
    }

    /// A text's words: its label on iOS, its value on macOS.
    @MainActor
    private func text(of element: XCUIElement) -> String {
        element.label.isEmpty ? (element.value as? String ?? "") : element.label
    }

    /// The screen as it's seen: XCUIScreen gives a landscape iPad's pixels
    /// in its panel's portrait order with an orientation flag that the PNG
    /// loses, so the image is redrawn upright (drawing applies the flag).
    @MainActor
    private func attach(_ screenshot: XCUIScreenshot, _ name: String) {
        let shot = screenshot.image
        let format = UIGraphicsImageRendererFormat()
        format.scale = shot.scale
        var image = UIGraphicsImageRenderer(size: shot.size, format: format).image { _ in
            shot.draw(in: CGRect(origin: .zero, size: shot.size))
        }
        if image.size.width < image.size.height {
            // No flag, still portrait: turn it a quarter anticlockwise.
            let portrait = image
            let size = CGSize(width: portrait.size.height, height: portrait.size.width)
            image = UIGraphicsImageRenderer(size: size, format: format).image { context in
                context.cgContext.translateBy(x: 0, y: portrait.size.width)
                context.cgContext.rotate(by: -.pi / 2)
                portrait.draw(at: .zero)
            }
        }
        let attachment = XCTAttachment(image: image)
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }
}
