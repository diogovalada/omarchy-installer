import Foundation
import OmarchyAppleInstallerTrustCore
import OmarchyInstallerUXCore

/// Adds render-only layout inventory and the pinned model admission gate.
/// The unmodified upstream environment retains every actual trust/approval object.
final class BridgeEnvironment: InstallerEnvironment, @unchecked Sendable {
  private let live = LiveInstallerEnvironment()
  private let lock = NSLock()
  private var retainedHost: AppleSiliconHostInspection?
  private var retainedInventory: ValidatedEngineInventory?
  private var bridgeBlock: String?

  var inspection: (AppleSiliconHostInspection?, ValidatedEngineInventory?, String?) {
    lock.withLock { (retainedHost, retainedInventory, bridgeBlock) }
  }
  var installationBlocked: Bool {
    lock.withLock { bridgeBlock != nil } || live.installationBlocked
  }
  var engineSupported: Bool { live.engineSupported }
  var hasApprovedPlan: Bool { live.hasApprovedPlan }
  var helperStatus: HelperDisplay { live.helperStatus }

  func inspect() async throws -> HostDisplay {
    lock.withLock {
      retainedHost = nil
      retainedInventory = nil
      bridgeBlock = nil
    }
    let host = try await Task.detached {
      try AppleSiliconHostInspector().inspect()
    }.value
    lock.withLock { retainedHost = host }
    guard BridgeIdentity.catalogModels.contains(host.identity.deviceIdentifier) else {
      let reason = "The pinned release catalog does not enable this Apple model"
      lock.withLock { bridgeBlock = reason }
      throw BridgeFailure("unsupported_model", reason)
    }
    let engine = try await EngineInspectionRunner().inspect().validated
    guard engine.deviceIdentifier == host.identity.deviceIdentifier else {
      throw InstallerAppError.hostChanged
    }
    lock.withLock { retainedInventory = engine.inventory }
    return try await live.inspect()
  }

  func preparePlan(
    selection: InstallTargetSelection,
    omarchyBytes: UInt64?,
    progress: @escaping @Sendable (AssetProgressUpdate) -> Void
  ) async throws -> PlanPreparationDisplay {
    guard !installationBlocked else {
      throw BridgeFailure("unsupported_selection", "Inspect an eligible Mac before preparing a plan")
    }
    // The upstream environment validates the retained selection against a fresh
    // signed inspection and retains all executable plan and approval objects.
    return try await live.preparePlan(selection: selection, omarchyBytes: omarchyBytes, progress: progress)
  }
  func approve() throws { try live.approve() }
  func discardApproval() { live.discardApproval() }
  func refreshHelperStatus() -> HelperDisplay { live.refreshHelperStatus() }
  func execute(
    operation: InstallOperationKind,
    authorization: MachineOwnerAuthorization,
    journal: @escaping @Sendable (Data) -> Void
  ) async throws -> CompletionDisplay {
    guard !installationBlocked else { throw BridgeFailure("blocked", "Installation is blocked") }
    return try await live.execute(operation: operation, authorization: authorization, journal: journal)
  }
  // Recovery instructions are returned to Tauri. No shutdown is exposed over IPC.
}
