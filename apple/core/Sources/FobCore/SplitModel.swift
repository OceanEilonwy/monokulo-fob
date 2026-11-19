// Split up (CORE-013, board PhoneSplit): tap a note to add a coin of it,
// tap its minus to take one away, Auto fills the mix learned from the last
// 30 days, Clear empties it. After every tap the core reads the counts
// again (fob-ffi's SplitView): the summary ("9 coins · A$120.00"), what's
// left as one coin, the button and the fee line. The mix shown is whatever
// Fob's splitting logic gives, not a board's. A scene's split never sends;
// the live wallet's follows in CORE-019.

import Foundation
import Observation

@MainActor
@Observable
public final class SplitModel {
    /// The screen as the core read the counts: keep its counts.
    public private(set) var view: SplitView
    /// Why the last tap didn't add a coin ("Not enough ready to spend for
    /// another A$20 coin"); cleared by the next tap.
    public private(set) var refusal: String?

    private let sceneName: String

    /// Split up for the scene `name`'s wallet, as the board opens it; none
    /// for a name that isn't a scene.
    public init?(scene name: String) {
        guard let wallet = scene(name: name) else { return nil }
        sceneName = name
        view = wallet.split
    }

    /// How many of each note, in the order of `view.tiles`.
    public var counts: [UInt32] { view.counts }

    /// One more coin of note `index`, if there's room.
    public func add(_ index: Int) {
        do {
            show(try sceneSplitAdd(name: sceneName, counts: counts, index: UInt32(index)))
        } catch {
            refusal = Self.reason(error)
        }
    }

    /// One fewer coin of note `index`.
    public func remove(_ index: Int) {
        show(sceneSplitRemove(name: sceneName, counts: counts, index: UInt32(index)))
    }

    /// Auto: the mix learned from the last 30 days. Does nothing without it.
    public func auto() {
        guard let mix = view.auto?.counts else { return }
        show(mix)
    }

    /// Clear: no coins of any note.
    public func clear() {
        show(Array(repeating: 0, count: view.tiles.count))
    }

    private func show(_ counts: [UInt32]) {
        refusal = nil
        if let read = sceneSplitView(name: sceneName, counts: counts) { view = read }
    }

    /// The words the core gave for `error`.
    static func reason(_ error: any Error) -> String {
        guard let error = error as? FobError else { return error.localizedDescription }
        return switch error {
        case .Invalid(let reason): reason
        case .NotEnoughFunds(let reason, _): reason
        default: String(describing: error)
        }
    }
}
