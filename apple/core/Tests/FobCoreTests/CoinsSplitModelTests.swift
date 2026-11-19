// Coins and Split up read from the "coins" and "split" scenes (CORE-013),
// as PhoneCoins and PhoneSplit draw them; Split up's taps go through the
// core, which says what the summary becomes.

import FobCore
import XCTest

final class CoinsSplitModelTests: XCTestCase {
    @MainActor
    func testCoinsScene() throws {
        let model = try XCTUnwrap(AppModel(arguments: ["-FobScene", "coins"]).coins)
        XCTAssertTrue(model.intro.hasPrefix("Your 0.4182 XMR is held as 4 coins."))
        XCTAssertEqual(model.readyTitle, "Ready to spend")
        XCTAssertEqual(model.readySum, "0.3870 · A$137.23")
        XCTAssertEqual(model.readyCoins.map(\.amountText), ["0.2000", "0.1250", "0.0620"])
        XCTAssertEqual(model.readyCoins.first?.source, "Received · 15 Nov")
        XCTAssertEqual(model.notReadyTitle, "Not ready · 1 coin")
        let waiting = try XCTUnwrap(model.notReadyCoins.first)
        XCTAssertEqual(waiting.confirmationsText, "3 of 10")
        XCTAssertEqual(waiting.ready, "09:32")
        XCTAssertTrue(model.canSplit)
        XCTAssertFalse(model.segments.isEmpty)
    }

    @MainActor
    func testSplitOpensAsTheBoard() throws {
        let model = try XCTUnwrap(AppModel(arguments: ["-FobScene", "split"]).split)
        XCTAssertEqual(model.counts, [2, 3, 4, 0, 0, 0])
        XCTAssertEqual(model.view.tiles.map(\.label), ["A$5", "A$10", "A$20", "A$50", "A$100", "A$200"])
        XCTAssertEqual(model.view.summary, "9 coins · A$120.00")
        XCTAssertEqual(model.view.button, "Split into 9 coins")
        XCTAssertNil(model.refusal)
    }

    @MainActor
    func testTapA20Twice() throws {
        let model = try XCTUnwrap(SplitModel(scene: "split"))
        // A$120 of A$137 is in coins already: the core says no.
        model.add(2)
        XCTAssertEqual(model.counts, [2, 3, 4, 0, 0, 0])
        XCTAssertNotNil(model.refusal)

        model.clear()
        XCTAssertNil(model.refusal)
        XCTAssertEqual(model.view.pieces, 0)
        XCTAssertFalse(model.view.ready)

        model.add(2)
        model.add(2)
        XCTAssertEqual(model.counts, [0, 0, 2, 0, 0, 0])
        XCTAssertEqual(model.view.summary, "2 coins · A$40.00")
        XCTAssertEqual(model.view.button, "Split into 2 coins")
        XCTAssertTrue(model.view.ready)

        model.remove(2)
        XCTAssertEqual(model.counts, [0, 0, 1, 0, 0, 0])
        XCTAssertEqual(model.view.summary, "1 coin · A$20.00")
    }

    @MainActor
    func testAuto() throws {
        let model = try XCTUnwrap(SplitModel(scene: "split"))
        model.clear()
        guard let mix = model.view.auto?.counts else { return }
        model.auto()
        XCTAssertEqual(model.counts, mix)
        XCTAssertTrue(model.view.isAuto)
    }
}
