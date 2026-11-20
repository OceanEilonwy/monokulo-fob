// What Home draws, ready to show: the wallet's name and network, the
// balance, the sync chip, Activity and the coins not ready yet. A
// WalletSource (a scene, or the live wallet) fills it from the core's
// records; the screens only read it. Every word is the core's.

import Foundation

/// The network a wallet is on: Stagenet shows a chip beside its name.
public enum WalletNetwork: Sendable, Equatable {
    case mainnet, stagenet
}

/// The balance card.
public struct HomeBalance: Sendable, Equatable {
    /// All of it, in XMR: "0.4182".
    public var total: String
    /// What it's worth now: "A$148.20"; none without a price.
    public var fiat: String?
    /// Ready to spend, in XMR: "0.3870".
    public var ready: String
    /// Not ready yet, in XMR: "0.0312"; none when all of it is ready.
    public var notReady: String?
    /// When the last of it should be ready: "09:32"; none when all of it is.
    public var allReadyAt: String?
    /// "0.3870 XMR ready to spend · A$137.23", under Send.
    public var readyLine: String
    /// The card read as one by VoiceOver while some isn't ready: "0.3870
    /// XMR ready to spend, 0.0312 XMR not yet, all ready about 09:32".
    public var notReadyDescription: String?

    public init(total: String, fiat: String?, ready: String, notReady: String?, allReadyAt: String?,
                readyLine: String, notReadyDescription: String?) {
        self.total = total
        self.fiat = fiat
        self.ready = ready
        self.notReady = notReady
        self.allReadyAt = allReadyAt
        self.readyLine = readyLine
        self.notReadyDescription = notReadyDescription
    }
}

/// The sync chip beside the wallet's name.
public enum SyncChip: Sendable, Equatable {
    /// "Synced", with a green dot.
    case synced(text: String)
    /// "4 min left", with a ring `progress` (0 to 1) of the way round.
    case catchingUp(text: String, progress: Double)
    /// "Can't reach node".
    case offline(text: String)

    /// The chip's words.
    public var text: String {
        switch self {
        case .synced(let text), .catchingUp(let text, _), .offline(let text): text
        }
    }
}

/// One row of Activity: a payment, a request or a share of your profile.
public struct ActivityItem: Sendable, Equatable, Identifiable {
    public enum Kind: Sendable, Equatable {
        case sent, received, request, share
    }

    /// Stable within its list: the transaction, request or share.
    public var id: String
    public var kind: Kind
    /// "To Corner Bakery", "From Mara", "Request to Joel".
    public var title: String
    /// "Receipt, 2 items · 09:12", "Concert tickets · not ready · 3 of 10 · 08:40".
    public var line: String
    /// With its sign: "−0.0312", "+0.1250"; a request's ask: "A$20.00".
    public var amount: String
    /// "A$11.05"; "asked" under a request's.
    public var fiat: String?
    /// A payment still short of its confirmations: (3, 10) for "3 of 10".
    public var confirmations: Confirmations?

    public init(id: String, kind: Kind, title: String, line: String, amount: String, fiat: String?,
                confirmations: Confirmations?) {
        self.id = id
        self.kind = kind
        self.title = title
        self.line = line
        self.amount = amount
        self.fiat = fiat
        self.confirmations = confirmations
    }
}

/// How far a payment is along: `done` of `needed`.
public struct Confirmations: Sendable, Equatable {
    public var done: Int
    public var needed: Int

    public init(done: Int, needed: Int) {
        self.done = done
        self.needed = needed
    }

    /// 0 to 1, for its ring.
    public var progress: Double { needed > 0 ? min(1, Double(done) / Double(needed)) : 1 }
}

/// A day of Activity under its label: "TODAY", "YESTERDAY", "27 OCT".
public struct ActivitySection: Sendable, Equatable, Identifiable {
    public var label: String
    public var rows: [ActivityItem]
    public var id: String { label }

    public init(label: String, rows: [ActivityItem]) {
        self.label = label
        self.rows = rows
    }
}

/// A coin not ready to spend yet, on Coins and the settling card.
public struct NotReadyCoin: Sendable, Equatable {
    /// "0.0312"
    public var amount: String
    /// "A$11.06"
    public var fiat: String?
    /// "Change · Corner Bakery"
    public var source: String
    public var confirmations: Confirmations
    /// "3 of 10"
    public var confirmationsText: String
    /// When it should be ready: "09:32".
    public var readyAt: String

    public init(amount: String, fiat: String?, source: String, confirmations: Confirmations,
                confirmationsText: String, readyAt: String) {
        self.amount = amount
        self.fiat = fiat
        self.source = source
        self.confirmations = confirmations
        self.confirmationsText = confirmationsText
        self.readyAt = readyAt
    }
}

/// Everything Home shows of a wallet.
public struct WalletState: Sendable, Equatable {
    /// "Spending"
    public var walletName: String
    public var network: WalletNetwork
    public var balance: HomeBalance
    public var syncChip: SyncChip
    /// VoiceOver's words for the chip: "Catching up, 34%, about 6 minutes left".
    public var syncLabel: String
    /// Newest day first.
    public var activity: [ActivitySection]
    /// Soonest ready first; empty when all of it is ready.
    public var notReadyCoins: [NotReadyCoin]

    public init(walletName: String, network: WalletNetwork, balance: HomeBalance, syncChip: SyncChip,
                syncLabel: String, activity: [ActivitySection], notReadyCoins: [NotReadyCoin]) {
        self.walletName = walletName
        self.network = network
        self.balance = balance
        self.syncChip = syncChip
        self.syncLabel = syncLabel
        self.activity = activity
        self.notReadyCoins = notReadyCoins
    }

    /// No wallet open.
    public static let empty = WalletState(
        walletName: "", network: .mainnet,
        balance: HomeBalance(total: "0", fiat: nil, ready: "0", notReady: nil, allReadyAt: nil,
                             readyLine: "", notReadyDescription: nil),
        syncChip: .offline(text: ""), syncLabel: "", activity: [], notReadyCoins: []
    )
}

// MARK: - From the core's records

extension WalletState {
    /// Home as a scene's demo wallet draws it.
    init(scene w: SceneWallet) {
        self.init(
            walletName: w.walletName,
            network: WalletNetwork(w.network),
            balance: HomeBalance(balance: w.balance, notReady: w.notReady),
            syncChip: SyncChip(w.sync),
            syncLabel: w.sync.chipLabel,
            activity: w.activity.map(ActivitySection.init),
            notReadyCoins: w.coins.notReadyCoins.map(NotReadyCoin.init)
        )
    }
}

extension WalletNetwork {
    init(_ network: Network) {
        switch network {
        case .mainnet: self = .mainnet
        case .stagenet: self = .stagenet
        }
    }
}

extension HomeBalance {
    init(balance b: Balance, notReady n: NotReady?) {
        self.init(
            total: b.totalText,
            fiat: b.fiatText,
            ready: b.unlockedText,
            notReady: n.map { Self.xmrAmount($0.incomingXmr) },
            allReadyAt: n?.allReady,
            readyLine: b.readyLine,
            notReadyDescription: n?.description
        )
    }

    /// "0.0312 XMR" without its unit: the card puts "not yet" after it.
    static func xmrAmount(_ text: String) -> String {
        text.hasSuffix(" XMR") ? String(text.dropLast(4)) : text
    }
}

extension SyncChip {
    /// The chip a core sync status draws (a scene's, or the live wallet's).
    public init(_ s: SyncStatus) {
        switch s.chipLook {
        case .synced: self = .synced(text: s.chipText)
        case .ring: self = .catchingUp(text: s.chipText, progress: Double(s.progress))
        case .offline: self = .offline(text: s.chipText)
        }
    }

    /// One of each look (synced, catching up, offline) with its VoiceOver
    /// words, from fob-scenes: what FobUI's gallery shows.
    public static var sceneStates: [(chip: SyncChip, label: String)] {
        sceneSyncStates().map { (SyncChip($0), $0.chipLabel) }
    }
}

extension ActivitySection {
    init(_ day: ActivityDay) {
        self.init(label: day.label, rows: day.rows.enumerated().map { ActivityItem($1, index: $0) })
    }
}

extension ActivityItem {
    init(_ row: ActivityRow, index: Int) {
        let kind: Kind
        let id: String
        var confirmations: Confirmations?
        if let payment = row.activity {
            kind = payment.outgoing ? .sent : .received
            id = "tx-\(payment.tx)"
            switch payment.status {
            case .notReady(let done, let needed), .confirming(let done, let needed):
                confirmations = Confirmations(done: Int(done), needed: Int(needed))
            default:
                confirmations = nil
            }
        } else if let request = row.request {
            kind = .request
            id = "request-\(request)"
        } else if let share = row.share {
            kind = .share
            id = "share-\(share)"
        } else {
            kind = .share
            id = "row-\(index)"
        }
        self.init(id: id, kind: kind, title: row.title, line: row.line, amount: row.amountText,
                  fiat: row.fiatText, confirmations: confirmations)
    }
}

extension NotReadyCoin {
    init(_ p: NotReadyPiece) {
        self.init(
            amount: p.amountText,
            fiat: p.fiatText,
            source: p.source,
            confirmations: Confirmations(done: Int(p.confirmations), needed: Int(p.needed)),
            confirmationsText: p.confirmationsText,
            readyAt: p.ready
        )
    }
}
