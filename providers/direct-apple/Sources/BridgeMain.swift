import Darwin
import Foundation
import OmarchyAppleInstallerTrustCore
import OmarchyInstallerUXCore

@main
struct BridgeMain {
  @MainActor
  static func main() async {
    guard CommandLine.arguments == [CommandLine.arguments[0], "--stdio"] else {
      try? FileHandle.standardError.write(contentsOf: Data("Usage: omarchy-apple-bridge --stdio\n".utf8))
      exit(EX_USAGE)
    }
    umask(0o077)
    signal(SIGPIPE, SIG_IGN)
    do {
      let lease = try InstallerAppInstanceLease.acquire(
        at: InstallerAppInstanceLease.defaultLockFileURL()
      )
      await BridgeServer().run()
      lease.release()
    } catch {
      // Never interpolate request content, credentials or arbitrary process errors.
      try? FileHandle.standardError.write(contentsOf: Data("The native installer bridge could not acquire its instance lease.\n".utf8))
      exit(EX_TEMPFAIL)
    }
  }
}

@MainActor
final class BridgeServer {
  private var environment = BridgeEnvironment()
  private lazy var session = InstallerSession(environment: environment)
  private let admission = BridgeAdmission()
  private let sessionID = UUID().uuidString
  private var usedIDs = Set<String>()
  private var activeCommand: String?
  private var activeTask: Task<Void, Never>?
  private var approvedDigest: String?
  private var admissionFailure: BridgeFailure?
  private var disconnected = false
  private var hasInspected = false
  private var lastState: Data?
  private struct ReplacementChoice {
    let id: String
    let identifier: String
    let install: ExistingInstallDisplay
  }
  private var replacements = [ReplacementChoice]()
  private var alongsideID = UUID().uuidString
  private var selectedReplacement: ReplacementChoice?
  private var pendingAllocation: UInt64?

  private func resetStorage() {
    replacements = []
    alongsideID = UUID().uuidString
    selectedReplacement = nil
    pendingAllocation = nil
  }

  func run() async {
    refreshAdmission()
    publishState()
    let reporter = Task { @MainActor [weak self] in
      while !Task.isCancelled {
        try? await Task.sleep(for: .milliseconds(250))
        self?.publishState()
      }
    }
    await Task.detached { [self] in
      await readBridgeInput { data in await self.receive(data) }
    }.value
    disconnected = true
    // EOF does not cancel helper execution or imply rollback. Let a submitted
    // operation settle through the upstream coordinator and its durable journal.
    await activeTask?.value
    reporter.cancel()
  }

  private func refreshAdmission() {
    do {
      try admission.validate()
      admissionFailure = nil
    } catch let failure as BridgeFailure {
      admissionFailure = failure
    } catch {
      admissionFailure = BridgeFailure("admission_failed", "The desktop identity could not be authenticated")
    }
  }

  func receive(_ data: Data) -> Bool {
    var requestID: String?
    do {
      let request = try BridgeRequest.decode(data)
      requestID = request.id
      guard usedIDs.count < 4096, usedIDs.insert(request.id).inserted else {
        throw BridgeFailure("replayed_request", "Request identifiers must be unique within a bounded session")
      }
      switch request.command {
      case "state": result(request.id, snapshot())
      case "probe": result(request.id, probe())
      case "cancel":
        guard activeCommand == nil, !session.hasExecutionStarted else {
          throw BridgeFailure("cancel_unavailable", "The native backend cannot cancel an active operation; keep following its journal")
        }
        environment.discardApproval()
        environment = BridgeEnvironment()
        session = InstallerSession(environment: environment)
        approvedDigest = nil
        hasInspected = false
        resetStorage()
        result(request.id, snapshot())
      default:
        guard activeCommand == nil else {
          throw BridgeFailure("busy", "Another native operation is still running")
        }
        try dispatch(request)
      }
      publishState()
    } catch let failure as BridgeFailure {
      error(requestID, failure)
    } catch {
      error(requestID, BridgeFailure("invalid_request", "The request could not be decoded or admitted"))
    }
    return true
  }

  private func dispatch(_ request: BridgeRequest) throws {
    switch request.command {
    case "inspect":
      try requireArchitecture()
      guard !session.hasExecutionStarted else {
        throw BridgeFailure("execution_latched", "An executed plan cannot be reset in this session")
      }
      approvedDigest = nil
      resetStorage()
      hasInspected = true
      launch(request) { await self.session.inspect() }
    case "prepare_plan":
      guard !session.hasExecutionStarted else {
        throw BridgeFailure("execution_latched", "An executed plan cannot be prepared again")
      }
      let allocation = try allocationBytes(request)
      guard selectedReplacement == nil else {
        throw BridgeFailure("fixed_allocation", "Replacement uses the entire existing installation; discard preparation to choose different storage")
      }
      let fromWelcome: Bool
      switch session.phase {
      case .welcome: fromWelcome = true
      case .planPrepared, .planReview: fromWelcome = false
      default: throw BridgeFailure("invalid_phase", "Inspect this Mac before preparing a plan")
      }
      guard fromWelcome || allocation != nil else {
        throw BridgeFailure("allocation_required", "Supply the new allocation when revising an alongside plan")
      }
      approvedDigest = nil
      pendingAllocation = allocation
      launch(request) {
        if fromWelcome {
          await self.session.continueToPlan()
          if let allocation, case .planPrepared = self.session.phase {
            await self.session.replan(omarchyBytes: allocation)
          }
        } else if let allocation {
          await self.session.replan(omarchyBytes: allocation)
        }
        // An allocation change is always a new review at the IPC boundary.
        self.session.setAcknowledged(false)
      }
    case "choose_storage":
      guard !session.hasExecutionStarted, case .existingInstallChoice(let options, _) = session.phase,
        let choiceID = request.params?.choiceId else {
        throw BridgeFailure("invalid_phase", "Choose storage from the current native inspection")
      }
      let allocation = try allocationBytes(request)
      if choiceID == alongsideID {
        guard request.params?.confirmation == nil else {
          throw BridgeFailure("invalid_request", "An alongside selection does not accept deletion confirmation")
        }
        selectedReplacement = nil
        approvedDigest = nil
        let requested = allocation
        replacements = []
        launch(request) {
          await self.session.chooseInstallAlongsideExistingInstall()
          if let requested, case .planPrepared = self.session.phase {
            await self.session.replan(omarchyBytes: requested)
          }
          self.session.setAcknowledged(false)
        }
      } else {
        guard allocation == nil,
          let choice = replacements.first(where: { $0.id == choiceID }),
          options.contains(choice.install),
          request.params?.confirmation?.trimmingCharacters(in: .whitespacesAndNewlines).lowercased() == choice.identifier.lowercased()
        else { throw BridgeFailure("invalid_selection", "Confirm the identifier of a current replacement choice; its allocation is fixed") }
        selectedReplacement = choice
        approvedDigest = nil
        pendingAllocation = nil
        replacements = []
        launch(request) {
          await self.session.chooseReplaceExistingInstall(choice.install)
          self.session.setAcknowledged(false)
        }
      }
    case "review_plan":
      guard case .planPrepared = session.phase else {
        throw BridgeFailure("invalid_phase", "No prepared plan is waiting for review")
      }
      session.continueToPlanReview()
      result(request.id, snapshot())
    case "acknowledge":
      guard case .planReview = session.phase, let value = request.params?.value else {
        throw BridgeFailure("invalid_phase", "A visible plan review and acknowledgement value are required")
      }
      session.setAcknowledged(value)
      result(request.id, snapshot())
    case "approve":
      guard case .planReview(let plan, true) = session.phase,
        request.params?.bindingDigest == plan.bindingDigest
      else { throw BridgeFailure("plan_mismatch", "Acknowledge the current plan and confirm its binding digest") }
      session.approve()
      if case .awaitingInstall = session.phase { approvedDigest = plan.bindingDigest }
      result(request.id, snapshot())
    case "refresh_helper":
      session.refreshHelperStatus()
      refreshAdmission()
      result(request.id, snapshot())
    case "execute", "retry_recovery":
      try requireArchitecture()
      refreshAdmission()
      if let failure = admissionFailure { throw failure }
      guard let digest = approvedDigest, request.params?.bindingDigest == digest else {
        throw BridgeFailure("plan_mismatch", "The request does not identify the retained approved plan")
      }
      if request.command == "execute" {
        guard session.canStartInstallation else {
          throw BridgeFailure("installation_blocked", "The approved plan, helper and host must all be ready")
        }
      } else {
        guard session.canRetryRecoveryAuthorization else {
          throw BridgeFailure("retry_unavailable", "No trusted Recovery authorization retry is available")
        }
      }
      guard let username = request.params?.username, let password = request.params?.password else {
        throw BridgeFailure("credentials_required", "Machine-owner credentials are required")
      }
      let authorization: MachineOwnerAuthorization
      do { authorization = try MachineOwnerAuthorization(username: username, password: Data(password.utf8)) }
      catch { throw BridgeFailure("invalid_credentials", "The machine-owner credential format is invalid") }
      if request.command == "execute" { session.presentInstallCredentials() }
      else { session.presentRecoveryRetryCredentials() }
      // Capture only authorization; never retain the request or password string
      // in the operation closure, session snapshot, event or error.
      launch(request.id, command: request.command) { await self.session.submit(authorization) }
    default: throw BridgeFailure("unknown_command", "Unknown native command")
    }
  }

  private func launch(_ request: BridgeRequest, action: @escaping @MainActor () async -> Void) {
    launch(request.id, command: request.command, action: action)
  }
  private func launch(_ id: String, command: String, action: @escaping @MainActor () async -> Void) {
    activeCommand = command
    activeTask = Task { @MainActor in
      await action()
      activeCommand = nil
      result(id, snapshot())
      publishState()
      activeTask = nil
    }
  }

  private func allocationBytes(_ request: BridgeRequest) throws -> UInt64? {
    guard let raw = request.params?.allocationBytes else { return nil }
    guard !raw.isEmpty, raw.utf8.allSatisfy({ (48...57).contains($0) }),
      let value = UInt64(raw), value > 0 else {
      throw BridgeFailure("invalid_allocation", "Allocation must be a positive UInt64 decimal string")
    }
    return value
  }
  private func requireArchitecture() throws {
    #if !arch(arm64)
      throw BridgeFailure("unsupported_architecture", "Native Apple installation requires an arm64 macOS process; Intel and Rosetta are not supported")
    #endif
  }

  private func probe() -> [String: Any] {
    refreshAdmission()
    var blockers = [[String: String]]()
    #if arch(arm64)
      let architecture = "arm64"
    #else
      let architecture = "x86_64"
      blockers.append(["code": "unsupported_architecture", "message": "Requires Apple Silicon and a native arm64 process"])
    #endif
    if let failure = admissionFailure { blockers.append(["code": failure.code, "message": failure.message]) }
    do { _ = try ValidationEngineArtifactLocator().locate() }
    catch { blockers.append(["code": "engine_unavailable", "message": "The pinned .7 engine is not present or did not verify"] ) }
    do { _ = try InstallerReleaseConfigurationLocator().loadFromMainBundle() }
    catch { blockers.append(["code": "release_resources_unavailable", "message": "The pinned release resources are not packaged"] ) }
    if !environment.helperStatus.isEnabled {
      blockers.append(["code": "helper_not_installed", "message": "The signed native helper has not been installed"])
    }
    if let expires = ISO8601DateFormatter().date(from: "2026-11-30T02:26:29Z"), Date() >= expires {
      blockers.append(["code": "catalog_expired", "message": "The pinned release catalog has expired; a reviewed release update is required"])
    }
    return [
      "provider": "direct-apple", "protocolVersion": 1,
      "sourceCommit": BridgeIdentity.sourceCommit, "engineVersion": BridgeIdentity.engineVersion,
      "os": "macos", "architecture": architecture, "minimumMacOS": "15.0",
      "catalogModels": BridgeIdentity.catalogModels.sorted(),
      "catalogExpiresAt": "2026-11-30T02:26:29Z",
      "packagingReady": blockers.isEmpty, "blockers": blockers,
      "requiresHostInspection": true, "activeCancellationSupported": false,
      "releaseQualification": "The bridge has not been built or hardware-qualified on macOS",
    ]
  }

  private func snapshot() -> [String: Any] {
    var value: [String: Any] = [
      "sessionId": sessionID, "phase": hasInspected ? "inspecting" : "idle",
      "busy": activeCommand != nil, "activeCommand": activeCommand as Any? ?? NSNull(),
      "canExecute": session.canStartInstallation && admissionFailure == nil,
      "canRetryRecovery": session.canRetryRecoveryAuthorization && admissionFailure == nil,
      "cancelAvailable": activeCommand == nil && !session.hasExecutionStarted,
      "hasExecutionStarted": session.hasExecutionStarted,
      "helper": ["enabled": environment.helperStatus.isEnabled, "summary": environment.helperStatus.summary],
    ]
    if let failure = admissionFailure { value["admissionBlock"] = ["code": failure.code, "message": failure.message] }
    let (host, inventory, block) = environment.inspection
    if let block { value["blockingReason"] = block }
    if let host {
      value["host"] = [
        "model": host.identity.model, "chip": host.identity.chip,
        "deviceIdentifier": host.identity.deviceIdentifier, "macOSVersion": host.macOSVersion,
        "powerSource": host.powerSource.rawValue, "fileVaultEnabled": host.fileVaultEnabled,
        "containerIdentifier": host.storage.containerIdentifier,
        "physicalStoreIdentifier": host.storage.physicalStoreIdentifier,
        "containerSizeBytes": String(host.storage.containerSizeBytes),
        "containerFreeBytes": String(host.storage.containerFreeBytes),
        "shrinkableBytes": String(host.storage.shrinkableBytes),
      ]
    }
    if let inventory {
      value["layout"] = [
        "displayOnly": true, "layoutDigest": inventory.layoutDigest,
        "systemStoreIdentifier": inventory.systemStoreIdentifier,
        "candidates": inventory.candidates.map { candidate in
          ["kind": candidate.kind, "sourceIdentifier": candidate.sourceIdentifier,
           "offsetBytes": String(candidate.offsetBytes), "lengthBytes": String(candidate.lengthBytes),
           "minimumInstallBytes": String(candidate.minimumInstallBytes),
           "minimumContainerBytes": String(candidate.minimumContainerBytes)]
        },
      ]
    }
    switch session.phase {
    case .inspecting: break
    case .unsupported(let failure): value["phase"] = "unsupported"; value["failure"] = failureJSON(failure)
    case .welcome(let host):
      value["phase"] = "welcome"; value["hostSummary"] = host.chipAndSpace
      value["storageChoices"] = [alongsideChoiceJSON()]
    case .existingInstallChoice(let options, _):
      value["phase"] = "existing_install_choice"
      if replacements.map({ $0.install }) != options {
        replacements = options.enumerated().map { index, install in
          ReplacementChoice(id: UUID().uuidString, identifier: "Installation \(index + 1)", install: install)
        }
      }
      value["storageChoices"] = [alongsideChoiceJSON()] + replacements.map { choice in
        ["id": choice.id, "label": "Replace \(choice.identifier)",
         "detail": choice.install.sourceIdentifier, "sizeDescription": choice.install.sizeDescription,
         "eligible": true, "reasons": [],
         "deletion": ["scope": "installation", "identifier": choice.identifier,
           "partitionLabel": choice.install.sourceIdentifier, "sizeDescription": choice.install.sizeDescription]] as [String: Any]
      }
    case .preparingPlan(let progress): value["phase"] = "preparing_plan"; value["progress"] = preparationJSON(progress)
    case .planPrepared(let plan, let progress):
      value["phase"] = "plan_prepared"; value["plan"] = planJSON(plan); value["progress"] = preparationJSON(progress)
    case .planReview(let plan, let acknowledged):
      value["phase"] = "plan_review"; value["plan"] = planJSON(plan); value["acknowledged"] = acknowledged
    case .awaitingInstall(let plan, _, _): value["phase"] = "awaiting_install"; value["plan"] = planJSON(plan)
    case .installing(let progress):
      value["phase"] = "installing"
      value["progress"] = [
        "phaseTitle": progress.phaseTitle, "stageIndex": progress.stageIndex,
        "stageFractions": progress.stageFractions, "stageLabels": progress.stageLabels,
        "completedCheckpoints": progress.completedCheckpoints, "degraded": progress.degraded,
        "startedAt": ISO8601DateFormatter().string(from: progress.startedAt),
        "feed": progress.feed.suffix(100).map { ["id": $0.id, "kind": $0.kind.rawValue, "text": $0.text] as [String: Any] },
      ]
    case .awaitingRecovery(let handoff): value["phase"] = "awaiting_recovery"; value["handoff"] = handoffJSON(handoff)
    case .done(let completion):
      value["phase"] = "done"
      value["completion"] = [
        "headline": completion.headline, "subheadline": completion.subheadline,
        "nextAction": completion.nextAction.rawValue,
        "verified": factsJSON(completion.verified),
      ]
      if let handoff = completion.handoff { value["handoff"] = handoffJSON(handoff) }
    case .failed(let failure): value["phase"] = "failed"; value["failure"] = failureJSON(failure)
    }
    if let context = session.credentialSheet.context {
      value["credentials"] = [
        "verifying": context.isVerifying, "rejected": context.error == .credentialsRejected,
        "operation": context.kind.rawValue,
      ]
    }
    return value
  }

  private func alongsideChoiceJSON() -> [String: Any] {
    var allocation: [String: Any] = ["mode": "automatic"]
    if let pendingAllocation, pendingAllocation <= 9_007_199_254_740_991 {
      allocation["recommendedBytes"] = pendingAllocation
    }
    return ["id": alongsideID, "label": "Install alongside macOS",
      "detail": "The native installer selects suitable free space or a supported macOS resize. Detected existing installations can be reviewed before continuing.",
      "eligible": true, "reasons": [], "allocation": allocation]
  }
  private func planJSON(_ plan: PlanDisplay) -> [String: Any] {
    var value: [String: Any] = ["headline": plan.headline, "subheadline": plan.subheadline,
     "diskTotalBytes": String(plan.diskTotalBytes), "omarchyBytes": String(plan.omarchyBytes),
     "macOSBytes": String(plan.macOSBytes), "bindingDigest": plan.bindingDigest,
     "planDigest": plan.planDigest, "facts": factsJSON(plan.facts),
     "artifacts": plan.artifacts.map { ["role": $0.role, "fileName": $0.fileName, "expectedBytes": String($0.expectedBytes)] }]
    if let choice = selectedReplacement {
      value["deletion"] = ["identifier": choice.identifier,
        "sourceIdentifier": choice.install.sourceIdentifier, "sizeBytes": String(plan.omarchyBytes)]
    }
    return value
  }
  private func factsJSON(_ facts: [PlanFactRow]) -> [[String: Any]] {
    facts.map { ["label": $0.label, "value": $0.value, "isMonospaced": $0.isMonospaced] }
  }
  private func preparationJSON(_ progress: AssetProgressUpdate) -> [String: Any] {
    ["stage": progress.stage.rawValue, "rows": progress.rows.map {
      ["role": $0.role, "fileName": $0.fileName, "bytesCompleted": String($0.bytesCompleted),
       "totalBytes": String($0.totalBytes), "phase": $0.phase.rawValue]
    }]
  }
  private func failureJSON(_ failure: FailureDisplay) -> [String: Any] {
    ["headline": failure.headline, "detail": failure.plainDetail,
     "technicalDetail": failure.technicalDetail as Any? ?? NSNull(),
     "remedy": failure.remedy as Any? ?? NSNull(), "retryRecoveryAvailable": failure.retryRecoveryAvailable]
  }
  private func handoffJSON(_ handoff: HandoffDisplay) -> [String: Any] {
    ["headline": handoff.headline, "subheadline": handoff.subheadline,
     "explainer": handoff.explainer, "hint": handoff.hint,
     "steps": handoff.steps.map { ["number": $0.number, "title": $0.title, "detail": $0.detail] as [String: Any] }]
  }
  private func publishState() {
    guard let data = try? JSONSerialization.data(withJSONObject: snapshot(), options: [.sortedKeys]), data != lastState else { return }
    lastState = data
    send(["version": 1, "type": "state", "data": snapshot()])
  }
  private func result(_ id: String, _ data: [String: Any]) {
    send(["version": 1, "id": id, "type": "result", "data": data])
  }
  private func error(_ id: String?, _ failure: BridgeFailure) {
    send(["version": 1, "id": id as Any? ?? NSNull(), "type": "error",
          "error": ["code": failure.code, "message": failure.message]])
  }
  private func send(_ object: [String: Any]) {
    guard !disconnected, var data = try? JSONSerialization.data(withJSONObject: object, options: [.sortedKeys]) else { return }
    data.append(10)
    do { try FileHandle.standardOutput.write(contentsOf: data) }
    catch { disconnected = true }
  }
}
