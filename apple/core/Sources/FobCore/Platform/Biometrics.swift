// Face ID / Touch ID / Optic ID before a payment goes (CORE-040), through
// LocalAuthentication. A scene never prompts: `-FobBiometrics
// pass|fail|unavailable` (pass unless told) says what the check answers,
// so the confirm screens can be screenshot in every outcome.

import Foundation
import LocalAuthentication
#if canImport(UIKit)
import UIKit
#endif

/// How this device checks it's you; none without biometrics (or with none
/// enrolled), when the passphrase stands in.
public enum BiometricKind: String, Sendable, CaseIterable {
    case faceID, touchID, opticID, none

    /// The core's name for it, for the words "Pay with Face ID"; none for none.
    public var biometry: Biometry? {
        switch self {
        case .faceID: .faceId
        case .touchID: .touchId
        case .opticID: .opticId
        case .none: nil
        }
    }

    /// The SF Symbol drawn on the button that pays.
    public var symbol: String {
        switch self {
        case .faceID: "faceid"
        case .touchID: "touchid"
        case .opticID: "opticid"
        case .none: "key.fill"
        }
    }
}

/// What a check answered.
public enum BiometricResult: String, Sendable, CaseIterable {
    /// It's you: the payment may go.
    case ok
    /// The person (or the system) called it off.
    case cancelled
    /// No biometrics here, none enrolled, or locked out.
    case unavailable
    /// Not recognised.
    case failed
}

public struct Biometrics: Sendable {
    public enum Mode: Sendable, Equatable {
        /// LocalAuthentication, for the real wallet.
        case system
        /// Scene mode: answers `result` without asking anyone.
        case stub(BiometricResult)
    }

    public let mode: Mode
    /// What the stub says the device has (the device's usual kind, or none
    /// when unavailable).
    private let stubKind: BiometricKind

    /// `mode`, with `kind` standing in for the device's when stubbed.
    public init(mode: Mode, kind: BiometricKind = .faceID) {
        self.mode = mode
        if case .stub(.unavailable) = mode {
            stubKind = .none
        } else {
            stubKind = kind
        }
    }

    /// From the launch arguments: in scene mode the stub `-FobBiometrics`
    /// names (pass unless told); otherwise the system's.
    @MainActor
    public init(arguments: [String], sceneMode: Bool) {
        guard sceneMode else {
            self.init(mode: .system)
            return
        }
        var result = BiometricResult.ok
        if let i = arguments.firstIndex(of: "-FobBiometrics"), i + 1 < arguments.count {
            switch arguments[i + 1] {
            case "fail": result = .failed
            case "unavailable": result = .unavailable
            default: result = .ok
            }
        }
        self.init(mode: .stub(result), kind: Self.deviceKind)
    }

    /// Never asks anyone: a scene's.
    public var isStub: Bool { mode != .system }

    /// How this device checks it's you, for the button's words and symbol.
    public var kind: BiometricKind {
        guard mode == .system else { return stubKind }
        let context = LAContext()
        var error: NSError?
        guard context.canEvaluatePolicy(.deviceOwnerAuthenticationWithBiometrics, error: &error) else {
            return .none
        }
        switch context.biometryType {
        case .faceID: return .faceID
        case .touchID: return .touchID
        case .opticID: return .opticID
        default: return .none
        }
    }

    /// Asks for Face ID / Touch ID with `reason` ("Pay Joel A$20.00").
    /// `passphrase` is the fallback hook: when biometrics aren't there, or
    /// the person picks "Use passphrase", it asks for the passphrase instead
    /// and says whether it was right. A stub answers at once and never
    /// calls it.
    public func confirm(
        reason: String,
        passphrase: (@Sendable () async -> Bool)? = nil
    ) async -> BiometricResult {
        if case .stub(let result) = mode { return result }
        let context = LAContext()
        let outcome: BiometricResult
        do {
            let ok = try await context.evaluatePolicy(
                .deviceOwnerAuthenticationWithBiometrics, localizedReason: reason)
            return ok ? .ok : .failed
        } catch let error as LAError {
            switch error.code {
            case .userCancel, .appCancel, .systemCancel:
                return .cancelled
            case .userFallback, .biometryNotAvailable, .biometryNotEnrolled, .biometryLockout, .passcodeNotSet:
                outcome = .unavailable
            default:
                return .failed
            }
        } catch {
            return .failed
        }
        guard let passphrase else { return outcome }
        return await passphrase() ? .ok : .failed
    }

    /// The device's usual kind until LocalAuthentication says: Face ID on
    /// iPhone, Touch ID on iPad and the Mac.
    @MainActor
    public static var deviceKind: BiometricKind {
        #if os(macOS)
        .touchID
        #else
        UIDevice.current.userInterfaceIdiom == .phone ? .faceID : .touchID
        #endif
    }
}
