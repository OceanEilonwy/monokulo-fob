// The chips beside the wallet's name (CORE-022; fob.css .chip, .net-chip,
// .dot): the sync chip (a green dot when synced, a ring filled as far as
// the scan with the time left while catching up, a red dot when the node
// doesn't answer, as Android's SyncChip draws them), the Stagenet chip and
// the wallet switcher ("Spending" with a chevron). The phone draws them at
// the boards' full size (Main), the Mac's title bar smaller (MacHome).

import FobCore
import SwiftUI

/// How big a chip is drawn.
public enum ChipSize: Sendable, CaseIterable {
    /// The phone's and iPad's: 32pt sync chip, 24pt Stagenet chip.
    case regular
    /// The Mac window's: 22pt, 12pt words.
    case small
}

extension ChipSize {
    var syncHeight: CGFloat { self == .regular ? 32 : 22 }
    var netHeight: CGFloat { self == .regular ? 24 : 22 }
    var syncPadding: CGFloat { self == .regular ? 12 : 9 }
    var netPadding: CGFloat { self == .regular ? 9 : 9 }
    var gap: CGFloat { self == .regular ? 7 : 6 }
    var dot: CGFloat { self == .regular ? 8 : 7 }
    var ring: CGFloat { self == .regular ? 14 : 10 }

    /// 15pt semibold on the phone, 12pt on the Mac.
    var syncFont: Font {
        switch self {
        case .regular: .subheadline.weight(.semibold)
        #if os(macOS)
        case .small: .callout.weight(.semibold)
        #else
        case .small: .caption.weight(.semibold)
        #endif
        }
    }

    /// 13pt semibold on the phone, 12pt on the Mac.
    var netFont: Font {
        switch self {
        case .regular: .footnote.weight(.semibold)
        #if os(macOS)
        case .small: .callout.weight(.semibold)
        #else
        case .small: .caption.weight(.semibold)
        #endif
        }
    }
}

/// The sync chip: a button that opens the sync card. `label` is the
/// chip's VoiceOver words (SyncStatus.chipLabel, WalletState.syncLabel);
/// VoiceOver reads them as fob-words' "Synced, tap for details".
public struct SyncChipView: View {
    let chip: SyncChip
    let label: String
    let size: ChipSize
    let action: @MainActor () -> Void

    @Environment(\.fobColors) private var colors

    public init(_ chip: SyncChip, label: String, size: ChipSize = .regular,
                action: @escaping @MainActor () -> Void = {}) {
        self.chip = chip
        self.label = label
        self.size = size
        self.action = action
    }

    public var body: some View {
        Button(action: action) {
            HStack(spacing: size.gap) {
                mark
                Text(chip.text)
                    .lineLimit(1)
            }
            .font(size.syncFont)
            .foregroundStyle(colors.ink)
            .padding(.horizontal, size.syncPadding)
            .frame(height: size.syncHeight)
            .background(colors.onPaper, in: Capsule())
            // A 44pt target round the smaller chip.
            .frame(minHeight: size == .regular ? 44 : nil)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(labelSyncChip(label: label))
        .accessibilityIdentifier("sync-chip")
    }

    @ViewBuilder
    private var mark: some View {
        switch chip {
        case .synced:
            Circle().fill(colors.greenDot).frame(width: size.dot, height: size.dot)
        case .catchingUp(_, let progress):
            SyncRing(progress: progress, track: colors.track, arc: colors.orangeText)
                .frame(width: size.ring, height: size.ring)
        case .offline:
            Circle().fill(colors.red).frame(width: size.dot, height: size.dot)
        }
    }
}

/// The catching-up ring: `track` all round, `arc` from the top as far as
/// `progress` (never less than a sliver, so it reads as started).
struct SyncRing: View {
    let progress: Double
    let track: Color
    let arc: Color

    var body: some View {
        GeometryReader { proxy in
            let line = max(2, proxy.size.width * 0.17)
            ZStack {
                Circle().stroke(track, lineWidth: line)
                Circle()
                    .trim(from: 0, to: min(1, max(0.02, progress)))
                    .stroke(arc, style: StrokeStyle(lineWidth: line, lineCap: .round))
                    .rotationEffect(.degrees(-90))
            }
            .padding(line / 2)
        }
        .accessibilityHidden(true)
    }
}

/// "Stagenet": orange words on the highlight colour, beside the balance
/// (phone) or the sync chip (Mac). Shown only off mainnet.
public struct StagenetChip: View {
    let text: String
    let size: ChipSize

    @Environment(\.fobColors) private var colors

    /// `text` is fob-words' (`labels.home.stagenet`).
    public init(_ text: String, size: ChipSize = .regular) {
        self.text = text
        self.size = size
    }

    public var body: some View {
        Text(text)
            .font(size.netFont)
            .foregroundStyle(colors.orangeText)
            .lineLimit(1)
            .padding(.horizontal, size.netPadding)
            .frame(height: size.netHeight)
            .background(colors.highlight, in: Capsule())
            .accessibilityIdentifier("stagenet-chip")
    }
}

/// The wallet switcher: Fob's mark, the wallet's name and a chevron on the
/// phone (Main's top left), the name and a chevron in the Mac's title bar.
/// Tapping it opens the wallet switcher.
public struct WalletChip: View {
    /// Where it sits.
    public enum Layout: Sendable {
        case phone, mac
    }

    let name: String
    let accessibilityName: String
    let layout: Layout
    let action: @MainActor () -> Void

    @Environment(\.fobColors) private var colors

    /// `name` is the wallet's ("Spending"); `accessibilityName` the
    /// switcher's (`labels.chrome.switchWallet`, or `.wallets` on the Mac).
    public init(_ name: String, accessibilityName: String, layout: Layout = .phone,
                action: @escaping @MainActor () -> Void = {}) {
        self.name = name
        self.accessibilityName = accessibilityName
        self.layout = layout
        self.action = action
    }

    public var body: some View {
        Button(action: action) {
            switch layout {
            case .phone:
                HStack(spacing: 6) {
                    FobMark().frame(width: 26, height: 26)
                    Text(name)
                        .font(.headline)
                        .foregroundStyle(colors.ink)
                    Image(systemName: "chevron.down")
                        .font(.caption.weight(.heavy))
                        .foregroundStyle(colors.muted)
                }
                .lineLimit(1)
                // In a toolbar item the name was squeezed out entirely;
                // the chip keeps its natural width.
                .fixedSize()
                .frame(minHeight: 44)
            case .mac:
                HStack(spacing: 5) {
                    Text(name)
                        .font(.headline)
                    Image(systemName: "chevron.down")
                        .font(.caption2.weight(.heavy))
                }
                .foregroundStyle(colors.ink)
                .padding(.horizontal, 8)
                .frame(height: 28)
            }
        }
        .buttonStyle(.plain)
        .lineLimit(1)
        .contentShape(Rectangle())
        .accessibilityLabel(accessibilityName)
        .accessibilityValue(name)
        .accessibilityIdentifier("wallet-chip")
    }
}
