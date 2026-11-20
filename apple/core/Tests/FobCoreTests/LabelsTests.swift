// The words around the wallet's data come from crates/fob-words through
// fob-ffi (CORE-005), never from Swift: a sample of them, as the boards show.

import FobCore
import XCTest

final class LabelsTests: XCTestCase {
    @MainActor
    func testBoardWords() {
        let words = AppModel(arguments: []).labels
        XCTAssertEqual([words.home.receive, words.home.send, words.home.activity, words.home.coins],
                       ["Receive", "Send", "Activity", "Coins"])
        XCTAssertEqual([words.tabs.wallet, words.tabs.contacts, words.tabs.settings],
                       ["Wallet", "Contacts", "Settings"])
        XCTAssertEqual(words.receive.whoIsItFor, "Who is it for?")
        XCTAssertEqual(words.send.review, "Review")
        XCTAssertEqual(words.send.payWho, "Pay who?")
        XCTAssertEqual(words.confirm.usePassphrase, "Use passphrase")
        XCTAssertEqual(words.backup.writtenDown, "I've written them down")
        XCTAssertEqual([words.welcome.create, words.welcome.restore], ["Create a wallet", "Restore a wallet"])
        XCTAssertEqual(words.welcome.points.map(\.title), ["Ready in seconds", "Keeps your receipts", "Yours alone"])
        // The test runs on the Mac.
        XCTAssertEqual(words.confirm.sendWith, "Send with Touch ID")
        XCTAssertEqual(words.settings.thisDevice, "This Mac")
    }

    func testDeviceWords() {
        let phone = labels(device: "iPhone", biometry: .faceId)
        XCTAssertEqual(phone.confirm.payWith, "Pay with Face ID")
        XCTAssertEqual(phone.welcome.points.last?.line,
                       "Talks only to the node you pick. Receipts stay on this iPhone.")
        XCTAssertEqual(labelPay(name: "Corner Bakery"), "Pay Corner Bakery")
        XCTAssertEqual(labelReceiptItems(items: 2), "Receipt · 2 items")
    }

    func testChromeWords() {
        let chrome = labels(device: "iPhone", biometry: .faceId).chrome
        XCTAssertEqual([chrome.scanAPaymentCode, chrome.switchWallet, chrome.sections],
                       ["Scan a payment code", "Switch wallet", "Sections"])
        XCTAssertEqual([chrome.contacts, chrome.refresh, chrome.settings, chrome.wallets],
                       ["Contacts", "Refresh", "Settings", "Wallets"])
        XCTAssertEqual(labelSyncChip(label: "Synced"), "Synced, tap for details")
    }

    func testSyncChipLooks() {
        let chips = SyncChip.sceneStates.map(\.chip)
        XCTAssertEqual(chips.count, 3)
        guard chips.count == 3 else { return }
        XCTAssertEqual(chips[0], .synced(text: "Synced"))
        if case .catchingUp(let text, let progress) = chips[1] {
            XCTAssertEqual(text, "4 min left")
            XCTAssertTrue(progress > 0 && progress < 1, "progress \(progress)")
        } else {
            XCTFail("the second look isn't catching up: \(chips[1])")
        }
        XCTAssertEqual(chips[2], .offline(text: "Can't reach node"))
    }
}
