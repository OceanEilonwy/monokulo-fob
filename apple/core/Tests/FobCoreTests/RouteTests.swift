// AppModel's route (CORE-011): every scene opens its screen, with the pages
// that lead to it, and the app moves between tabs and pages.

import FobCore
import XCTest

final class RouteTests: XCTestCase {
    @MainActor
    func testEverySceneHasARoute() {
        for name in AppModel.sceneNames {
            XCTAssertNotNil(Route(scene: name), "\(name) has no screen")
            let model = AppModel(arguments: ["-FobScene", name])
            XCTAssertEqual(model.route, Route(scene: name), name)
            XCTAssertNil(model.gallery, name)
        }
    }

    @MainActor
    func testScenesOpenTheirScreens() {
        let expected: [String: (AppTab, [Route])] = [
            "home": (.wallet, []),
            "home-syncing": (.wallet, []),
            "receive": (.wallet, [.receive]),
            "send": (.wallet, [.send]),
            "send-form": (.wallet, [.send, .sendForm]),
            "confirm": (.wallet, [.send, .sendForm, .confirm]),
            "confirm-joel": (.wallet, [.send, .sendForm, .confirm]),
            "pay-who": (.wallet, [.send, .sendForm, .payWho]),
            "payment": (.wallet, [.payment]),
            "contacts": (.contacts, []),
            "settings": (.settings, []),
            "welcome": (.wallet, [.welcome]),
            "backup": (.settings, [.backup]),
            "catchup": (.wallet, [.catchup]),
            "coins": (.wallet, [.coins]),
            "split": (.wallet, [.coins, .split]),
        ]
        XCTAssertEqual(Set(expected.keys), Set(AppModel.sceneNames))
        for (name, start) in expected {
            let (tab, path) = start
            let model = AppModel(arguments: ["-FobScene", name])
            XCTAssertEqual(model.tab, tab, name)
            XCTAssertEqual(model.path, path, name)
        }
        XCTAssertEqual(AppModel(arguments: ["-FobScene", "home"]).route, .wallet)
        XCTAssertEqual(AppModel(arguments: ["-FobScene", "contacts"]).route, .contacts)
    }

    @MainActor
    func testNoSceneOrAGallery() {
        let plain = AppModel(arguments: [])
        XCTAssertEqual(plain.tab, .wallet)
        XCTAssertEqual(plain.path, [])
        XCTAssertEqual(plain.route, .wallet)
        XCTAssertNil(plain.gallery)

        let gallery = AppModel(arguments: ["-FobScene", "gallery-theme", "-FobTheme", "light"])
        XCTAssertEqual(gallery.gallery, "theme")
        XCTAssertEqual(gallery.theme, .light)
        XCTAssertEqual(gallery.route, .wallet)
        XCTAssertNil(Route(scene: "gallery-theme"))
    }

    @MainActor
    func testThemes() {
        XCTAssertEqual(AppModel(arguments: ["-FobTheme", "dark"]).theme, .dark)
        XCTAssertEqual(AppModel(arguments: ["-FobTheme", "light"]).theme, .light)
        XCTAssertEqual(AppModel(arguments: ["-FobTheme", "sepia"]).theme, .oled)
    }

    @MainActor
    func testNavigation() {
        let model = AppModel(arguments: ["-FobScene", "home"])
        model.show(.send)
        model.show(.sendForm)
        XCTAssertEqual(model.route, .sendForm)
        model.show(.sendForm)
        XCTAssertEqual(model.path, [.send, .sendForm], "showing the top page again pushes nothing")
        model.back()
        XCTAssertEqual(model.route, .send)

        model.show(.backup)
        XCTAssertEqual(model.tab, .settings)
        XCTAssertEqual(model.path, [.backup])

        model.show(.contacts)
        XCTAssertEqual(model.tab, .contacts)
        XCTAssertEqual(model.path, [])
        XCTAssertEqual(model.route, .contacts)

        model.select(.wallet)
        model.show(.coins)
        model.show(.split)
        model.popToRoot()
        XCTAssertEqual(model.route, .wallet)
        model.back()
        XCTAssertEqual(model.path, [])
    }
}
