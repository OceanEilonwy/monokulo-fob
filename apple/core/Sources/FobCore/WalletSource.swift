// Where AppModel's wallet comes from: a scene's demo wallet for screenshots
// (SceneSource), or the real wallet with its sync observer (LiveSource,
// CORE-019). The model only reads `state` and is told when it changes.

import Foundation

@MainActor
public protocol WalletSource: AnyObject {
    /// What Home shows now.
    var state: WalletState { get }
    /// Calls `onChange` with each new state (a block scanned, a payment
    /// arriving) for as long as the source lives.
    func observe(_ onChange: @escaping @MainActor (WalletState) -> Void)
}

/// A demo wallet from crates/fob-scenes: fixed data, no node, no clock, so
/// it never changes.
@MainActor
public final class SceneSource: WalletSource {
    /// The scene's records, for the screens that read more than Home.
    public let wallet: SceneWallet
    public let state: WalletState

    public init(wallet: SceneWallet) {
        self.wallet = wallet
        state = WalletState(scene: wallet)
    }

    /// The scene called `name`; none for a name that isn't one.
    public convenience init?(name: String) {
        guard let wallet = scene(name: name) else { return nil }
        self.init(wallet: wallet)
    }

    public func observe(_ onChange: @escaping @MainActor (WalletState) -> Void) {}
}
