// Contacts' model read from the "contacts" scene (CORE-012), as
// PhoneContacts draws it.

import FobCore
import XCTest

final class ContactsModelTests: XCTestCase {
    @MainActor
    func testContactsScene() throws {
        let model = try XCTUnwrap(AppModel(arguments: ["-FobScene", "contacts"]).contacts)
        XCTAssertEqual(model.contacts.map(\.name), ["Mara Okafor", "Joel", "Priya", "Sam Whitlock"])
        XCTAssertEqual(model.contacts.map(\.initial), ["M", "J", "P", "S"])
        XCTAssertEqual(model.contacts.map(\.line), [
            "Concerts, coffee · paid 6 times, last today",
            "Flatmate · paid 14 times, last yesterday",
            "Half set up",
            "Not paid yet",
        ])
        XCTAssertEqual(model.contacts.map(\.halfSetUp), [false, false, true, false])

        XCTAssertEqual(model.unsaved.count, 1)
        XCTAssertEqual(model.unsaved.first?.name, "4AdU nnb4 … q2Ws")
        XCTAssertEqual(model.unsaved.first?.line, "paid 2 times, last 3 Nov")
    }

    @MainActor
    func testSave() throws {
        let model = try XCTUnwrap(ContactsModel(scene: "contacts"))
        let payee = try XCTUnwrap(model.unsaved.first)
        XCTAssertNil(model.save(payee, name: "   "), "a blank name saves nothing")
        let saved = try XCTUnwrap(model.save(payee, name: " Ana "))
        XCTAssertEqual(saved.name, "Ana")
        XCTAssertEqual(saved.initial, "A")
        XCTAssertEqual(saved.sendAddress, payee.address)
        XCTAssertEqual(saved.id, 5)
        XCTAssertEqual(model.contacts.last, saved)
        XCTAssertTrue(model.unsaved.isEmpty)
        XCTAssertNil(model.save(payee, name: "Ana"), "it's saved once")
        XCTAssertNil(ContactsModel(scene: "nowhere"))
    }
}
