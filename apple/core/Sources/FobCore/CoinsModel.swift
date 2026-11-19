// Coins (CORE-013, board PhoneCoins): the intro, the bar, "Ready to spend"
// with its sum ("0.3870 · A$137.23") and each coin with where it came from,
// then "Not ready · 1 coin" with each coin's "3 of 10" and when it's ready
// ("09:32"). Every word is the core's: a scene's wallet here; the live
// wallet follows in CORE-019.

import Foundation
import Observation

@MainActor
@Observable
public final class CoinsModel {
    /// The screen as the core wrote it.
    public private(set) var coins: Coins

    /// Coins for the scene `name`'s wallet; none for a name that isn't a
    /// scene.
    public init?(scene name: String) {
        guard let wallet = scene(name: name) else { return nil }
        coins = wallet.coins
    }

    /// "Your 0.4182 XMR is held as 4 coins. A payment uses whole coins; …"
    public var intro: String { coins.intro }
    /// The bar, coin by coin.
    public var segments: [CoinSegment] { coins.segments }
    /// What VoiceOver reads for the bar.
    public var barDescription: String { coins.barDescription }
    /// "Ready to spend"
    public var readyTitle: String { coins.readyTitle }
    /// "0.3870 · A$137.23"
    public var readySum: String { coins.readySum }
    /// Largest first.
    public var readyCoins: [Coin] { coins.readyCoins }
    /// "Not ready · 1 coin"
    public var notReadyTitle: String { coins.notReadyTitle }
    /// "0.0312 · A$11.06"
    public var notReadySum: String { coins.notReadySum }
    /// Soonest ready first.
    public var notReadyCoins: [NotReadyPiece] { coins.notReadyCoins }
    /// Split up can be pressed; greyed out otherwise.
    public var canSplit: Bool { coins.canSplit }
}
