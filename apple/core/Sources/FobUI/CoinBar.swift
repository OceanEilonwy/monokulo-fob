// Money on its way (CORE-027, CORE-028): the confirmation ring a coin not
// ready yet fills (PhoneCoins' not-ready row, Activity's "not ready · 3 of
// 10"), the coins bar (ready coins in ink, a coin not ready in faint green
// filled as far as it's come) and Home's settling card over it (Main,
// PhoneHomeLight). The widths and every word are the core's.

import FobCore
import SwiftUI

// MARK: - Confirmation ring

/// A ring filling `done` of `needed` in green over the track, from the top
/// clockwise: 30pt as PhoneCoins draws it, or small beside a line of text.
public struct ConfirmationRing: View {
    /// How big it's drawn.
    public enum Size: Sendable {
        /// 30pt, a 3.5pt stroke: Coins' not-ready row.
        case regular
        /// 14pt, a 2.5pt stroke: in a line of text.
        case small

        var side: CGFloat { self == .regular ? 30 : 14 }
        var stroke: CGFloat { self == .regular ? 3.5 : 2.5 }
    }

    let confirmations: Confirmations
    let size: Size

    @Environment(\.fobColors) private var colors

    public init(_ confirmations: Confirmations, size: Size = .regular) {
        self.confirmations = confirmations
        self.size = size
    }

    public var body: some View {
        ZStack {
            Circle()
                .stroke(colors.track, lineWidth: size.stroke)
            Circle()
                // Never less than a sliver, so a coin just in reads as started.
                .trim(from: 0, to: max(0.02, confirmations.progress))
                .stroke(colors.green, style: StrokeStyle(lineWidth: size.stroke, lineCap: .round))
                .rotationEffect(.degrees(-90))
        }
        .padding(size.stroke / 2)
        .frame(width: size.side, height: size.side)
        .accessibilityElement()
        .accessibilityLabel(labelConfirmations(done: UInt64(max(0, confirmations.done)),
                                               needed: UInt64(max(0, confirmations.needed))))
        .accessibilityIdentifier("confirmation-ring")
    }
}

// MARK: - Coins bar

/// The coins bar: one segment a coin, 2pt apart, as wide as its share
/// (the core's `coinBarWidths`), 6pt tall. Ready coins in ink; a coin not
/// ready in faint green on the page's paper, solid green as far as it's
/// come. Hidden from VoiceOver: the card or the screen around it says it.
public struct CoinBar: View {
    let segments: [CoinSegment]

    @Environment(\.fobColors) private var colors

    public init(_ segments: [CoinSegment]) {
        self.segments = segments
    }

    public var body: some View {
        GeometryReader { proxy in
            let widths = coinBarWidths(segments: segments, width: Float(proxy.size.width))
            HStack(spacing: CGFloat(coinBarGap())) {
                ForEach(Array(segments.enumerated()), id: \.offset) { index, segment in
                    piece(segment)
                        .frame(width: index < widths.count ? CGFloat(widths[index]) : 0)
                }
            }
        }
        .frame(height: 6)
        .accessibilityHidden(true)
    }

    @ViewBuilder
    private func piece(_ segment: CoinSegment) -> some View {
        if let progress = segment.progress {
            ZStack(alignment: .leading) {
                Rectangle().fill(colors.paper)
                Rectangle().fill(colors.green.opacity(0.28))
                GeometryReader { proxy in
                    Rectangle()
                        .fill(colors.green)
                        .frame(width: proxy.size.width * CGFloat(min(1, max(0, progress))))
                }
            }
        } else {
            Rectangle().fill(colors.ink)
        }
    }
}

// MARK: - Settling card

/// Home's card while some money isn't ready: "0.3870 ready" in ink, then
/// "↓ 0.0312 not yet" and the clock with "09:32" in green, the coins bar
/// under them. Read by VoiceOver as one: "0.3870 XMR ready to spend, 0.0312
/// XMR not yet, all ready about 09:32". Show it only while
/// `balance.notReady` is set.
public struct SettlingCard: View {
    let balance: HomeBalance
    let coinBar: [CoinSegment]
    let labels: HomeLabels

    @Environment(\.fobColors) private var colors

    /// `coinBar` is the wallet's (`model.coinBar`); `labels` give "ready"
    /// and "not yet" (`model.labels.home`).
    public init(balance: HomeBalance, coinBar: [CoinSegment], labels: HomeLabels) {
        self.balance = balance
        self.coinBar = coinBar
        self.labels = labels
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 10) {
                Text(verbatim: "\(balance.ready) \(labels.ready)")
                    .foregroundStyle(colors.ink)
                    .frame(maxWidth: .infinity, alignment: .leading)
                if let notReady = balance.notReady {
                    HStack(spacing: 2) {
                        Image(systemName: "arrow.down")
                            .font(.footnote.weight(.bold))
                        Text(verbatim: "\(notReady) \(labels.notYet)")
                    }
                    .foregroundStyle(colors.green)
                }
                if let allReadyAt = balance.allReadyAt {
                    HStack(spacing: 4) {
                        Image(systemName: "clock")
                            .font(.subheadline)
                        Text(verbatim: allReadyAt)
                            .fontWeight(.regular)
                    }
                    .foregroundStyle(colors.green)
                }
            }
            .font(.subheadline.weight(.semibold))
            .monospacedDigit()
            .lineLimit(1)
            CoinBar(coinBar)
        }
        .padding(.horizontal, 16)
        .padding(.top, 12)
        .padding(.bottom, 14)
        .background(colors.card, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 18, style: .continuous).strokeBorder(colors.line, lineWidth: 1))
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(balance.notReadyDescription ?? "")
        .accessibilityIdentifier("settling-card")
    }
}
