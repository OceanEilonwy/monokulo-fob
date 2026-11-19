// Receive's model read from the "receive" scene (CORE-014), as PhoneReceive
// draws it: 19 groups starting 5AqHt JB65J 3Lu7M, the A$ AUD field and
// "Who is it for?".

import FobCore
import XCTest

final class ReceiveModelTests: XCTestCase {
    @MainActor
    func testReceiveScene() throws {
        let model = try XCTUnwrap(AppModel(arguments: ["-FobScene", "receive"]).receive)
        XCTAssertEqual(model.groupTexts.count, 19)
        XCTAssertEqual(Array(model.groupTexts.prefix(3)), ["5AqHt", "JB65J", "3Lu7M"])
        XCTAssertEqual(model.groupTexts.last, "cUDet")
        XCTAssertEqual(model.groups.indices.filter { model.groups[$0].highlighted }, [0, 9, 18])
        XCTAssertEqual(model.groupTexts.joined(), model.address)
        XCTAssertEqual(model.pattern.dots.count, 25)

        XCTAssertEqual(model.unit, .fiat)
        XCTAssertEqual(model.entry?.sign, "A$")
        XCTAssertEqual(model.entry?.unit, "AUD")
        XCTAssertEqual(model.code.uri, "monero:\(model.address)")
        XCTAssertNil(model.code.headline)
        XCTAssertNotNil(model.qr)
    }

    @MainActor
    func testAmountAndSwap() throws {
        let model = try XCTUnwrap(ReceiveModel(scene: "receive"))
        model.amountText = "20.00"
        XCTAssertEqual(model.entry?.line, "≈ 0.0564 XMR · 1 XMR = A$354.60")
        XCTAssertEqual(model.code.headline, "A$20.00")
        XCTAssertTrue(model.code.uri.hasPrefix("monero:\(model.address)?tx_amount=0.0564"), model.code.uri)

        model.switchUnit()
        XCTAssertEqual(model.unit, .xmr)
        XCTAssertEqual(model.amountText, "0.0564")
        XCTAssertEqual(model.entry?.unit, "XMR")

        model.amountText = "not an amount"
        XCTAssertEqual(model.code.uri, "monero:\(model.address)", "an amount that isn't one is left out")
    }

    @MainActor
    func testWhoIsItForMakesARequest() throws {
        let model = try XCTUnwrap(ReceiveModel(scene: "receive"))
        XCTAssertNil(model.makeRequest(), "no request without a name")
        model.amountText = "20.00"
        model.label = "Joel"
        XCTAssertTrue(model.code.uri.contains("Joel"), model.code.uri)
        let request = try XCTUnwrap(model.makeRequest())
        XCTAssertEqual(request.label, "Joel")
        XCTAssertEqual(request.headline, "A$20.00")
        XCTAssertEqual(request.uri, model.code.uri)
        XCTAssertEqual(model.requests, [request])
        XCTAssertNil(ReceiveModel(scene: "nowhere"))
    }
}
