// Contacts (CORE-012, board PhoneContacts): the people this wallet pays,
// each with a line the core wrote ("Concerts, coffee · paid 6 times, last
// today", "Half set up", "Not paid yet"), and "Paid, not saved": addresses
// paid that aren't contacts yet, each with Save. A scene's saves stay in
// memory; the live wallet's follow in CORE-019.

import Foundation
import Observation

/// A contact in the list.
public struct ContactItem: Sendable, Equatable, Identifiable {
    public var id: UInt64
    /// "Mara Okafor"
    public var name: String
    /// The avatar's letter: "M".
    public var initial: String
    /// "Concerts, coffee · paid 6 times, last today", "Half set up".
    public var line: String
    /// Where they're paid; none until it's set.
    public var sendAddress: String?

    public init(id: UInt64, name: String, initial: String, line: String, sendAddress: String?) {
        self.id = id
        self.name = name
        self.initial = initial
        self.line = line
        self.sendAddress = sendAddress
    }

    /// No send address yet: "Half set up", its line drawn in orange.
    public var halfSetUp: Bool { sendAddress == nil }
}

/// An address paid that isn't a contact: "4AdU nnb4 … q2Ws".
public struct UnsavedPayee: Sendable, Equatable, Identifiable {
    /// The address paid.
    public var address: String
    /// It shortened, as the row's title: "4AdU nnb4 … q2Ws".
    public var name: String
    /// "paid 2 times, last 3 Nov"
    public var line: String
    public var id: String { address }

    public init(address: String, name: String, line: String) {
        self.address = address
        self.name = name
        self.line = line
    }
}

@MainActor
@Observable
public final class ContactsModel {
    /// Contacts, in the core's order.
    public private(set) var contacts: [ContactItem]
    /// Paid, not saved.
    public private(set) var unsaved: [UnsavedPayee]

    /// Contacts for the scene `name`'s wallet; none for a name that isn't a
    /// scene.
    public convenience init?(scene name: String) {
        guard let wallet = scene(name: name) else { return nil }
        self.init(contacts: wallet.contacts.map(ContactItem.init),
                  unsaved: wallet.unsaved.map(UnsavedPayee.init))
    }

    public init(contacts: [ContactItem], unsaved: [UnsavedPayee]) {
        self.contacts = contacts
        self.unsaved = unsaved
    }

    /// Saves `payee` as a contact called `name`, paid at its address, with
    /// its payments as its line; returns it. Nothing for a blank name or a
    /// payee not in the list.
    @discardableResult
    public func save(_ payee: UnsavedPayee, name: String) -> ContactItem? {
        let name = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !name.isEmpty, let index = unsaved.firstIndex(of: payee) else { return nil }
        let contact = ContactItem(id: (contacts.map(\.id).max() ?? 0) + 1, name: name,
                                  initial: contactInitial(name: name), line: payee.line,
                                  sendAddress: payee.address)
        unsaved.remove(at: index)
        contacts.append(contact)
        return contact
    }
}

extension ContactItem {
    init(_ row: ContactRow) {
        self.init(id: row.id, name: row.name, initial: row.initial, line: row.line,
                  sendAddress: row.sendAddress)
    }
}

extension UnsavedPayee {
    init(_ paid: PaidBefore) {
        self.init(address: paid.address, name: paid.name, line: paid.line)
    }
}
