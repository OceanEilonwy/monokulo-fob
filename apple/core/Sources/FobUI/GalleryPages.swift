// The component pages of FobUI's gallery: gallery-pills (CORE-021),
// gallery-chips (CORE-022) and gallery-chrome (CORE-030). Each draws its
// component as the boards do, with fob-words' words; the small headings are
// the gallery's own.

import FobCore
import SwiftUI

/// A heading over one part of a gallery page.
private struct GallerySection<Content: View>: View {
    let title: String
    @ViewBuilder let content: Content

    @Environment(\.fobColors) private var colors

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(verbatim: title)
                .fobFont(.sectionLabel)
                .foregroundStyle(colors.muted)
            content
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

// MARK: - gallery-pills

/// Home's Receive / Send pair on the phone and on the Mac, each style and
/// size, then a pressed and a disabled one of each style.
struct PillsGallery: View {
    let words: Labels

    var body: some View {
        GallerySection(title: "pair, phone") {
            ReceiveSendButtons(labels: words.home, layout: .phone, receive: {}, send: {})
        }
        GallerySection(title: "pair, mac") {
            ReceiveSendButtons(labels: words.home, layout: .mac, receive: {}, send: {})
                .frame(maxWidth: .infinity)
        }
        GallerySection(title: "review, paste") {
            PillButton(words.send.review, size: .large) {}
                .accessibilityIdentifier("pill-review")
            HStack {
                PillButton(words.send.paste, style: .tinted, size: .small) {}
                    .fixedSize()
                    .accessibilityIdentifier("pill-paste")
                Spacer()
            }
        }
        GallerySection(title: "pressed") {
            HStack(spacing: 8) {
                ForEach(PillStyle.allCases, id: \.self) { style in
                    PillBody(style: style, size: .compact, pressed: true) {
                        Label(words.home.send, systemImage: ReceiveSendButtons.sendSymbol)
                    }
                }
            }
        }
        GallerySection(title: "disabled") {
            HStack(spacing: 8) {
                ForEach(PillStyle.allCases, id: \.self) { style in
                    PillButton(words.home.send, systemImage: ReceiveSendButtons.sendSymbol, style: style,
                               size: .compact) {}
                        .disabled(true)
                }
            }
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("pills-disabled")
        }
    }
}

// MARK: - gallery-chips

/// The sync chip in each look (from fob-scenes), the Stagenet chip and the
/// wallet switcher, at the phone's size and the Mac's, then Main's top bar.
struct ChipsGallery: View {
    let words: Labels

    private var home: SceneWallet? { scene(name: "home") }
    private var walletName: String { home?.walletName ?? "" }
    private let states = SyncChip.sceneStates

    var body: some View {
        GallerySection(title: "main") {
            HStack(spacing: 8) {
                WalletChip(walletName, accessibilityName: words.chrome.switchWallet)
                Spacer(minLength: 0)
                if let first = states.first {
                    SyncChipView(first.chip, label: first.label)
                }
                FobIconButton(.scan, labels: words.chrome) {}
            }
            StagenetChip(words.home.stagenet)
        }
        GallerySection(title: "sync") {
            VStack(alignment: .leading, spacing: 4) {
                ForEach(Array(states.enumerated()), id: \.offset) { _, state in
                    SyncChipView(state.chip, label: state.label)
                }
            }
        }
        GallerySection(title: "mac") {
            // MacHome's pair over the balance, then the other two looks.
            HStack(spacing: 6) {
                StagenetChip(words.home.stagenet, size: .small)
                ForEach(Array(states.enumerated()), id: \.offset) { index, state in
                    if index == 0 {
                        SyncChipView(state.chip, label: state.label, size: .small)
                    }
                }
            }
            HStack(spacing: 6) {
                ForEach(Array(states.enumerated()), id: \.offset) { index, state in
                    if index > 0 {
                        SyncChipView(state.chip, label: state.label, size: .small)
                    }
                }
            }
            WalletChip(walletName, accessibilityName: words.chrome.wallets, layout: .mac)
        }
    }
}

// MARK: - gallery-chrome

/// A whole tab bar (Wallet, Contacts, Settings) round a navigation stack:
/// Wallet's toolbar (the wallet switcher, the sync chip, Scan), the Mac's
/// title-bar icons, and Coins pushed on it with "‹ Wallet".
struct ChromeGallery: View {
    let words: Labels

    @State private var tab: AppTab = .wallet

    var body: some View {
        TabView(selection: $tab) {
            ForEach(AppTab.allCases, id: \.self) { item in
                Tab(item.title(words.tabs), systemImage: item.symbol, value: item) {
                    switch item {
                    case .wallet:
                        NavigationStack {
                            ChromeWalletPage(words: words)
                        }
                    case .contacts, .settings:
                        ChromeEmptyPage(title: item.title(words.tabs))
                    }
                }
            }
        }
        .fobTabBar()
    }
}

private struct ChromeWalletPage: View {
    let words: Labels

    @Environment(\.fobColors) private var colors

    private var home: SceneWallet? { scene(name: "home") }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 24) {
                GallerySection(title: "mac title bar") {
                    HStack(spacing: 6) {
                        FobIconButton(.contacts, labels: words.chrome, look: .bar) {}
                        Spacer(minLength: 0)
                        WalletChip(home?.walletName ?? "", accessibilityName: words.chrome.wallets, layout: .mac)
                        Spacer(minLength: 0)
                        FobIconButton(.refresh, labels: words.chrome, look: .bar) {}
                        FobIconButton(.settings, labels: words.chrome, look: .bar) {}
                    }
                    .padding(.horizontal, 12)
                    .frame(height: 52)
                    .background(colors.card, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
                    .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous).strokeBorder(colors.line, lineWidth: 1))
                }
                GallerySection(title: "pushed") {
                    NavigationLink(value: Route.coins) {
                        HStack {
                            Text(words.coins.title)
                                .font(.body)
                                .foregroundStyle(colors.ink)
                            Spacer()
                            Image(systemName: "chevron.right")
                                .font(.footnote.weight(.semibold))
                                .foregroundStyle(colors.outline)
                        }
                        .padding(.horizontal, 16)
                        .frame(minHeight: 50)
                        .background(colors.card, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
                        .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous).strokeBorder(colors.line, lineWidth: 1))
                    }
                    .buttonStyle(.plain)
                    .accessibilityIdentifier("open-coins")
                }
            }
            .padding(16)
        }
        .background(colors.paper.ignoresSafeArea())
        .fobBars()
        .toolbar {
            ToolbarItem(placement: .navigation) {
                WalletChip(home?.walletName ?? "", accessibilityName: words.chrome.switchWallet)
            }
            .sharedBackgroundVisibility(.hidden)
            ToolbarItem(placement: .primaryAction) {
                HStack(spacing: 8) {
                    if let state = SyncChip.sceneStates.first {
                        SyncChipView(state.chip, label: state.label)
                    }
                    FobIconButton(.scan, labels: words.chrome) {}
                }
            }
            .sharedBackgroundVisibility(.hidden)
        }
        .navigationDestination(for: Route.self) { _ in
            ChromeCoinsPage(words: words)
        }
    }
}

/// Coins as PhoneCoins opens: "‹ Wallet", then the large title.
private struct ChromeCoinsPage: View {
    let words: Labels

    @Environment(\.fobColors) private var colors

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text(words.coins.title)
                .fobFont(.largeTitle)
                .foregroundStyle(colors.ink)
                .padding(.horizontal, 20)
                .padding(.top, 2)
                .accessibilityIdentifier("coins-title")
            Spacer()
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(colors.paper.ignoresSafeArea())
        .fobBars()
        .fobBackButton(words.tabs.wallet)
    }
}

private struct ChromeEmptyPage: View {
    let title: String

    @Environment(\.fobColors) private var colors

    var body: some View {
        Text(title)
            .fobFont(.largeTitle)
            .foregroundStyle(colors.ink)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            .padding(20)
            .background(colors.paper.ignoresSafeArea())
            .fobBars()
    }
}
