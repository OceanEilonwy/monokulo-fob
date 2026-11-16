// AppModel read from the scenes (CORE-010): what Home shows of the "home"
// and "home-syncing" demo wallets, as Main, PadHome and MacHome draw it.
// Runs in the macOS job (scheme FobMacUITests), no app needed.

import FobCore
import XCTest

final class AppModelTests: XCTestCase {
    @MainActor
    func testHomeScene() {
        let model = AppModel(arguments: ["-FobScene", "home", "-FobTheme", "light"])
        XCTAssertTrue(model.hasWallet)
        XCTAssertEqual(model.theme, .light)
        XCTAssertEqual(model.walletName, "Spending")
        XCTAssertEqual(model.network, .stagenet)

        let balance = model.balance
        XCTAssertEqual(balance.total, "0.4182")
        XCTAssertEqual(balance.fiat, "A$148.20")
        XCTAssertEqual(balance.ready, "0.3870")
        XCTAssertEqual(balance.notReady, "0.0312")
        XCTAssertEqual(balance.allReadyAt, "09:32")

        XCTAssertEqual(model.syncChip, .synced(text: "Synced"))

        XCTAssertEqual(model.activity.map(\.label), ["TODAY", "YESTERDAY"])
        let today = model.activity[0].rows
        XCTAssertEqual(today.count, 3)
        XCTAssertEqual(today.map(\.kind), [.sent, .received, .request])
        XCTAssertEqual(today.map(\.title), ["To Corner Bakery", "From Mara", "Request to Joel"])
        XCTAssertEqual(today.map(\.amount), ["−0.0312", "+0.1250", "A$20.00"])
        XCTAssertEqual(today.map(\.fiat), ["A$11.05", "A$44.30", "asked"])
        XCTAssertNil(today[0].confirmations)
        XCTAssertEqual(today[1].confirmations, Confirmations(done: 3, needed: 10))
        XCTAssertEqual(today[1].line, "Concert tickets · not ready · 3 of 10 · 08:40")
        XCTAssertEqual(Set(model.activity.flatMap(\.rows).map(\.id)).count, 5, "row ids are unique")

        XCTAssertEqual(model.notReadyCoins.count, 1)
        XCTAssertEqual(model.notReadyCoins.first?.amount, "0.0312")
        XCTAssertEqual(model.notReadyCoins.first?.confirmationsText, "3 of 10")
    }

    @MainActor
    func testHomeSyncingScene() {
        let model = AppModel(arguments: ["-FobScene", "home-syncing"])
        XCTAssertEqual(model.theme, .oled)
        XCTAssertEqual(model.balance.total, "0.4182")
        guard case .catchingUp(let text, let progress) = model.syncChip else {
            return XCTFail("home-syncing should be catching up, not \(model.syncChip)")
        }
        XCTAssertEqual(text, "4 min left")
        XCTAssertGreaterThan(progress, 0)
        XCTAssertLessThan(progress, 1)
    }

    @MainActor
    func testNoScene() {
        let model = AppModel(arguments: [])
        XCTAssertNil(model.sceneName)
        XCTAssertFalse(model.hasWallet)
        XCTAssertEqual(model.state, .empty)
        XCTAssertEqual(AppModel(arguments: ["-FobScene", "nowhere"]).state, .empty)
    }

    @MainActor
    func testEveryScene() {
        for name in AppModel.sceneNames {
            let model = AppModel(arguments: ["-FobScene", name])
            XCTAssertTrue(model.hasWallet, name)
            XCTAssertEqual(model.walletName, "Spending", name)
        }
    }
}
