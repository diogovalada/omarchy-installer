import Darwin
import Foundation
import Security
import OmarchyAppleInstallerTrustCore

enum BridgeIdentity {
  static let sourceCommit = "00daf3ebef9e4fbe89fb9b32bd184e65aa75d357"
  static let bundleIdentifier = "community.omarchy.setup.apple-bridge"
  static let engineVersion = "v0.9.0-omarchy.7"
  static let catalogModels: Set<String> = ["apple,j314s"]
}

/// The helper authenticates this companion. This additional boundary authenticates
/// the Tauri process controlling it, so the companion is not an identity relay.
final class BridgeAdmission {
  private struct Descriptor: Decodable {
    let schemaVersion: Int
    let sourceCommit: String
    let parentCodeSigningRequirement: String
  }
  private let originalParent = getppid()
  private var originalParentCode: SecCode?

  func validate() throws {
    guard geteuid() != 0 else {
      throw BridgeFailure("root_process_refused", "Launch the bridge as the signed desktop app's user")
    }
    guard Bundle.main.bundleIdentifier == BridgeIdentity.bundleIdentifier,
      let resource = Bundle.main.resourceURL?.appendingPathComponent("bridge-admission.json"),
      let bytes = try? Data(contentsOf: resource), bytes.count <= 16_384,
      let descriptor = try? JSONDecoder().decode(Descriptor.self, from: bytes),
      descriptor.schemaVersion == 1,
      descriptor.sourceCommit == BridgeIdentity.sourceCommit,
      !descriptor.parentCodeSigningRequirement.isEmpty,
      descriptor.parentCodeSigningRequirement.utf8.count <= 4096
    else {
      throw BridgeFailure("signed_bundle_required", "The native companion's sealed admission resources are missing")
    }
    var ownCode: SecCode?
    guard SecCodeCopySelf(SecCSFlags(), &ownCode) == errSecSuccess,
      let ownCode,
      SecCodeCheckValidity(ownCode, SecCSFlags(), nil) == errSecSuccess
    else { throw BridgeFailure("invalid_companion_signature", "The native companion's signature is invalid") }
    guard getppid() == originalParent, originalParent > 1 else {
      throw BridgeFailure("parent_changed", "The desktop process is no longer connected")
    }
    var requirement: SecRequirement?
    guard SecRequirementCreateWithString(
      descriptor.parentCodeSigningRequirement as CFString, SecCSFlags(), &requirement
    ) == errSecSuccess, let requirement else {
      throw BridgeFailure("invalid_parent_requirement", "The parent identity requirement is invalid")
    }
    if originalParentCode == nil {
      let attributes = [kSecGuestAttributePid as String: NSNumber(value: originalParent)] as CFDictionary
      guard SecCodeCopyGuestWithAttributes(nil, attributes, SecCSFlags(), &originalParentCode)
        == errSecSuccess else {
        throw BridgeFailure("parent_identity_unavailable", "Cannot authenticate the desktop process")
      }
    }
    guard let code = originalParentCode,
      SecCodeCheckValidity(code, SecCSFlags(), requirement) == errSecSuccess
    else {
      throw BridgeFailure("signed_parent_required", "The desktop process does not match the sealed signing requirement")
    }
  }
}
