// The biometrics stub (CORE-040): a scene never prompts and answers as
// -FobBiometrics says, pass unless told; the words name the device's kind.

import FobCore
import XCTest

final class BiometricsTests: XCTestCase {
    @MainActor
    func testSceneModeStubs() async throws {
        let pass = Biometrics(arguments: ["-FobScene", "confirm"], sceneMode: true)
        XCTAssertTrue(pass.isStub)
        XCTAssertEqual(pass.mode, .stub(.ok))
        XCTAssertEqual(pass.kind, .touchID, "the Mac's kind")
        let ok = await pass.confirm(reason: "Pay Joel")
        XCTAssertEqual(ok, .ok)

        let fail = Biometrics(arguments: ["-FobBiometrics", "fail"], sceneMode: true)
        let failed = await fail.confirm(reason: "Pay Joel")
        XCTAssertEqual(failed, .failed)
        XCTAssertEqual(fail.kind, .touchID)

        let none = Biometrics(arguments: ["-FobBiometrics", "unavailable"], sceneMode: true)
        XCTAssertEqual(none.kind, .none)
        XCTAssertNil(none.kind.biometry)
        let unavailable = await none.confirm(reason: "Pay Joel") { true }
        XCTAssertEqual(unavailable, .unavailable, "a stub never calls the passphrase")
    }

    @MainActor
    func testOutsideAScene() {
        let real = Biometrics(arguments: ["-FobBiometrics", "fail"], sceneMode: false)
        XCTAssertEqual(real.mode, .system)
        XCTAssertFalse(real.isStub)
    }

    @MainActor
    func testAppModelWords() {
        let model = AppModel(arguments: ["-FobScene", "confirm-joel"])
        XCTAssertEqual(model.biometrics.mode, .stub(.ok))
        XCTAssertEqual(model.labels.confirm.sendWith, "Send with Touch ID")
        let none = AppModel(arguments: ["-FobScene", "confirm-joel", "-FobBiometrics", "unavailable"])
        XCTAssertEqual(none.biometrics.kind, .none)
        XCTAssertEqual(none.labels.confirm.sendWith, "Send with Touch ID")
    }

    func testKinds() {
        XCTAssertEqual(BiometricKind.faceID.biometry, .faceId)
        XCTAssertEqual(BiometricKind.opticID.symbol, "opticid")
        XCTAssertEqual(BiometricKind.allCases.count, 4)
    }
}
