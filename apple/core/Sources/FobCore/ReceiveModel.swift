// Receive (CORE-014, boards PhoneReceive and PhoneReceiveLight): the
// address in groups of five with the ones people check marked, its pattern
// badge, the payment code for the QR, the amount field with its currency
// swap (A$ AUD <-> XMR) and "Who is it for?", which makes a request. Every
// word and number comes from the core: a scene's wallet through the
// scene_* functions (fixed price, no node); the live wallet follows in
// CORE-019.

import Foundation
import Observation

/// A request made on Receive: who it's for, and the code to show them.
public struct ReceiveRequest: Sendable, Equatable, Identifiable {
    public var id: Int
    /// "Joel"
    public var label: String
    /// The payment code: `monero:<address>?tx_amount=…&tx_description=…`.
    public var uri: String
    /// "A$20.00" or "0.5 XMR"; none without an amount.
    public var headline: String?
    /// "0.0564 XMR · Joel", or just the label.
    public var detail: String?

    public init(id: Int, label: String, uri: String, headline: String?, detail: String?) {
        self.id = id
        self.label = label
        self.uri = uri
        self.headline = headline
        self.detail = detail
    }
}

@MainActor
@Observable
public final class ReceiveModel {
    /// The address payments come to.
    public let address: String
    /// It in groups of five (19 for a standard address), the first, middle
    /// and last highlighted: the ones people check, marked on the board.
    public let groups: [AddressGroup]
    /// The pattern badge across the QR card's top edge.
    public let pattern: AddressPattern

    /// What's typed in the amount field.
    public var amountText: String { didSet { refresh() } }
    /// What it's typed in: money (A$) as the board opens it, or XMR.
    public private(set) var unit: AmountUnit
    /// The field's words: sign "A$", chip "AUD", the line under it
    /// ("≈ 0.0564 XMR · 1 XMR = A$354.60"), its labels.
    public private(set) var entry: AmountEntry? = nil
    /// "Who is it for?": the note the payer sees, and the request's name.
    public var label: String { didSet { refresh() } }
    /// The code the QR shows: the address with the amount and label when
    /// they're set (an amount that isn't one is left out).
    public private(set) var code: ReceiveCode
    /// Requests made here, newest last.
    public private(set) var requests: [ReceiveRequest] = []

    private let sceneName: String

    /// Receive for the scene `name`'s wallet; none for a name that isn't a
    /// scene.
    public init?(scene name: String) {
        guard let wallet = scene(name: name),
              let blank = try? sceneReceiveCode(name: name, amount: nil, note: nil) else { return nil }
        sceneName = name
        address = wallet.receiveAddress
        groups = addressGroups(address: wallet.receiveAddress)
        pattern = addressPattern(address: wallet.receiveAddress)
        amountText = ""
        label = ""
        code = blank
        let money = sceneAmountEntry(name: name, amount: .fiat(text: ""))
        unit = money?.canSwitch == true ? .fiat : .xmr
        refresh()
    }

    /// The groups' text: ["5AqHt", "JB65J", …].
    public var groupTexts: [String] { groups.map(\.text) }

    /// The QR's modules for `code`; none if it can't be drawn.
    public var qr: QrCode? { try? qrCode(text: code.uri) }

    /// The chip: switches the field between money and XMR, converting
    /// what's typed. Does nothing without a price.
    public func switchUnit() {
        guard entry?.canSwitch == true else { return }
        let other: AmountUnit = unit == .xmr ? .fiat : .xmr
        let converted = sceneConvertAmount(name: sceneName, text: amountText, to: other)
        unit = other
        amountText = converted
    }

    /// Makes a request for `label` with the amount typed, if any; none
    /// without a label.
    @discardableResult
    public func makeRequest() -> ReceiveRequest? {
        let name = label.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !name.isEmpty else { return nil }
        let request = ReceiveRequest(id: (requests.last?.id ?? 0) + 1, label: name, uri: code.uri,
                                     headline: code.headline, detail: code.detail)
        requests.append(request)
        return request
    }

    private var typed: TypedAmount {
        unit == .xmr ? .xmr(text: amountText) : .fiat(text: amountText)
    }

    private func refresh() {
        entry = sceneAmountEntry(name: sceneName, amount: typed)
        let note = label.isEmpty ? nil : label
        let made = (try? sceneReceiveCode(name: sceneName, amount: typed, note: note))
            ?? (try? sceneReceiveCode(name: sceneName, amount: nil, note: note))
        if let made { code = made }
    }
}
