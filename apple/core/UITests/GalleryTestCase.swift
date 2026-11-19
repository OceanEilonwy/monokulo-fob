// FobCoreUITests (docs/apple/coordination.md, "Components"): core proves
// FobUI's components in the iPhone app's gallery, `-FobScene gallery-<name>`,
// and attaches each screenshot as "<task_id>--<theme>"
// (apple/ci/extract-screenshots.sh). Runs in the FobiPhoneUITests scheme.

import XCTest

class GalleryTestCase: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    /// Opens gallery `name` in `theme`, waits for it and screenshots it as
    /// "<task>--<theme>". Returns the app for further checks.
    @MainActor
    @discardableResult
    func shoot(_ name: String, theme: String, task: String) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["-FobScene", "gallery-\(name)", "-FobTheme", theme]
        app.launch()
        let gallery = app.descendants(matching: .any)["gallery"].firstMatch
        XCTAssertTrue(gallery.waitForExistence(timeout: 60), "gallery-\(name) never showed")
        // Let the first frame settle.
        Thread.sleep(forTimeInterval: 1)
        let attachment = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        attachment.name = "\(task)--\(theme)"
        attachment.lifetime = .keepAlways
        add(attachment)
        return app
    }
}
