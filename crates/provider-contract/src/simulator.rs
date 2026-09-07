use std::collections::BTreeMap;

use crate::{
    Architecture, AuthorizationProof, CancelDisposition, CancelRequest, CancelResult,
    CapabilitiesRequest, CapabilitiesResponse, CapabilityDescriptor, CapabilityId,
    DiagnosticBundle, DiagnosticCode, DiagnosticEntry, DiagnosticsRequest, ExecuteRequest,
    FindingCode, FindingSeverity, FirmwareKind, HostFacts, HostOs, ImmutablePlan, InstallMode,
    OperationIntent, OperationOutcome, OperationReceipt, OperationSnapshot, PlanAction, PlanId,
    PlanRequest, PlanResponse, PlanStep, PreflightFinding, PreflightRequest, PreflightResponse,
    ProbeRequest, ProbeResponse, ProgressEvent, ProgressSink, ProgressStage, ProviderDescriptor,
    ProviderError, ProviderErrorCode, ProviderId, ProviderLifecycle, RecoverRequest,
    RecoveryDisposition, RecoveryResult, ResumeRequest, ResumeResponse, RetryAdvice, Sha256Digest,
    SupportLevel, UnsignedPlan,
};

#[derive(Clone, Debug)]
pub struct SimulatorScenario {
    pub host: HostFacts,
    pub capabilities: Vec<CapabilityDescriptor>,
    pub preflight_findings: Vec<PreflightFinding>,
    pub interrupt_at: Option<ProgressStage>,
}

impl Default for SimulatorScenario {
    fn default() -> Self {
        Self {
            host: HostFacts {
                os: HostOs::Windows,
                architecture: Architecture::X86_64,
                os_version: "11-simulated".into(),
                device_model: Some("Omarchy Simulator".into()),
                firmware: FirmwareKind::Uefi,
                memory_bytes: 16 * 1024 * 1024 * 1024,
                secure_boot_enabled: Some(false),
                virtualization_available: Some(true),
                on_ac_power: Some(true),
                snapshot_digest: Sha256Digest::of_bytes(b"simulated-host-v1"),
            },
            capabilities: vec![CapabilityDescriptor {
                capability_id: CapabilityId::new("simulator.download").expect("static ID is valid"),
                operation_kind: crate::OperationKind::DownloadArtifact,
                support_level: SupportLevel::SimulationOnly,
                requires_elevation: false,
                reason_code: None,
            }],
            preflight_findings: vec![],
            interrupt_at: None,
        }
    }
}

#[derive(Clone, Debug)]
struct SimulatedOperation {
    snapshot: OperationSnapshot,
    events: Vec<ProgressEvent>,
    receipt: Option<OperationReceipt>,
}

#[derive(Clone, Debug)]
pub struct SimulatedProvider {
    scenario: SimulatorScenario,
    operations: BTreeMap<crate::OperationId, SimulatedOperation>,
}

impl SimulatedProvider {
    pub fn new(scenario: SimulatorScenario) -> Self {
        Self {
            scenario,
            operations: BTreeMap::new(),
        }
    }

    fn operation_not_found() -> ProviderError {
        ProviderError::new(
            ProviderErrorCode::OperationNotFound,
            "the simulator does not own this operation",
            RetryAdvice::Never,
        )
    }

    fn event(
        operation_id: &crate::OperationId,
        sequence: u64,
        now: u64,
        stage: ProgressStage,
    ) -> ProgressEvent {
        ProgressEvent {
            operation_id: operation_id.clone(),
            sequence,
            occurred_at_unix_seconds: now + sequence,
            stage,
            completed_units: sequence,
            total_units: Some(4),
            message_code: DiagnosticCode::new(
                format!("simulator.progress.{stage:?}").to_lowercase(),
            )
            .expect("generated diagnostic ID is portable"),
            cancellable: matches!(
                stage,
                ProgressStage::Queued | ProgressStage::Revalidating | ProgressStage::Preparing
            ),
        }
    }
}

impl Default for SimulatedProvider {
    fn default() -> Self {
        Self::new(SimulatorScenario::default())
    }
}

impl ProviderLifecycle for SimulatedProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            provider_id: ProviderId::new("omarchy.simulator").expect("static ID is valid"),
            display_name: "Omarchy Setup Simulator".into(),
            provider_version: env!("CARGO_PKG_VERSION").into(),
            protocol_versions: vec![crate::PROVIDER_PROTOCOL_V1],
        }
    }

    fn probe(&self, _request: ProbeRequest) -> Result<ProbeResponse, ProviderError> {
        Ok(ProbeResponse {
            host: self.scenario.host.clone(),
        })
    }

    fn capabilities(
        &self,
        request: CapabilitiesRequest,
    ) -> Result<CapabilitiesResponse, ProviderError> {
        if request.host.snapshot_digest != self.scenario.host.snapshot_digest {
            return Err(ProviderError::new(
                ProviderErrorCode::StaleHostFacts,
                "host facts do not match the current simulated snapshot",
                RetryAdvice::AfterReprobe,
            ));
        }
        Ok(CapabilitiesResponse {
            capabilities: self.scenario.capabilities.clone(),
        })
    }

    fn preflight(&self, request: PreflightRequest) -> Result<PreflightResponse, ProviderError> {
        if request.operation.host_snapshot_digest != self.scenario.host.snapshot_digest {
            return Err(ProviderError::new(
                ProviderErrorCode::StaleHostFacts,
                "operation was created from stale host facts",
                RetryAdvice::AfterReprobe,
            ));
        }

        let mut findings = self.scenario.preflight_findings.clone();
        let capability = self
            .scenario
            .capabilities
            .iter()
            .find(|capability| capability.capability_id == request.operation.capability_id);
        if capability.is_none_or(|capability| {
            capability.operation_kind != request.operation.intent.kind()
                || capability.support_level == SupportLevel::Unavailable
        }) {
            findings.push(PreflightFinding {
                code: FindingCode::new("capability.unavailable").expect("static ID is valid"),
                severity: FindingSeverity::Blocking,
                summary: "The simulated provider does not advertise this operation".into(),
                remediation: None,
            });
        }
        if let OperationIntent::CreateUsb { target, .. } = &request.operation.intent
            && (!target.removable || target.internal || target.system_disk)
        {
            findings.push(PreflightFinding {
                code: FindingCode::new("target.not_safe_removable").expect("static ID is valid"),
                severity: FindingSeverity::Blocking,
                summary: "USB targets must be removable, external, and not a system disk".into(),
                remediation: Some("Select an unambiguous disposable USB drive".into()),
            });
        }
        let may_plan = !findings
            .iter()
            .any(|finding| finding.severity == FindingSeverity::Blocking);
        Ok(PreflightResponse { findings, may_plan })
    }

    fn plan(&self, request: PlanRequest) -> Result<PlanResponse, ProviderError> {
        let preflight = self.preflight(PreflightRequest {
            operation: request.operation.clone(),
        })?;
        if !preflight.may_plan {
            return Err(ProviderError::new(
                ProviderErrorCode::PreflightBlocked,
                "blocking preflight findings prevent planning",
                RetryAdvice::AfterUserAction,
            ));
        }

        let action = match &request.operation.intent {
            OperationIntent::TryVirtualMachine {
                persistence,
                memory_bytes,
                storage_bytes,
                ..
            } => PlanAction::CreateVirtualMachine {
                persistence: *persistence,
                memory_bytes: *memory_bytes,
                storage_bytes: *storage_bytes,
            },
            OperationIntent::DownloadArtifact { artifact } => PlanAction::ResolveVerifiedArtifact {
                artifact: artifact.clone(),
            },
            OperationIntent::CreateUsb {
                artifact,
                target,
                verify_after_write,
            } => PlanAction::WriteRemovableMedia {
                artifact: artifact.clone(),
                target: target.clone(),
                verify_after_write: *verify_after_write,
            },
            OperationIntent::DirectInstall {
                artifact,
                target,
                mode,
            } => PlanAction::PrepareDirectInstall {
                artifact: artifact.clone(),
                target: target.clone(),
                mode: *mode,
            },
            OperationIntent::ManageOwnedResources { resource_id } => {
                PlanAction::RemoveOwnedResource {
                    resource_id: resource_id.clone(),
                }
            }
        };
        let elevated = matches!(
            action,
            PlanAction::WriteRemovableMedia { .. }
                | PlanAction::PrepareDirectInstall { .. }
                | PlanAction::RemoveOwnedResource { .. }
        );
        let destructive_effect = match action {
            PlanAction::WriteRemovableMedia { .. } => Some(
                "All existing data on the selected removable device will be overwritten".into(),
            ),
            PlanAction::PrepareDirectInstall {
                mode: InstallMode::Replace,
                ..
            } => Some("The selected installation target will be replaced".into()),
            _ => None,
        };
        let contents = UnsignedPlan {
            plan_id: PlanId::new(format!("plan.{}", request.operation.operation_id))
                .expect("operation ID yields a valid plan ID"),
            operation_id: request.operation.operation_id,
            provider_id: self.descriptor().provider_id,
            provider_version: self.descriptor().provider_version,
            host_snapshot_digest: request.operation.host_snapshot_digest,
            policy: request.policy,
            created_at_unix_seconds: request.requested_at_unix_seconds,
            expires_at_unix_seconds: request.requested_at_unix_seconds + 900,
            steps: vec![PlanStep {
                sequence: 1,
                action,
                requires_elevation: elevated,
                destructive_effect,
            }],
        };
        let plan = ImmutablePlan::seal(contents).map_err(|error| {
            ProviderError::new(
                ProviderErrorCode::Internal,
                format!("could not seal simulated plan: {error}"),
                RetryAdvice::Never,
            )
        })?;
        Ok(PlanResponse { plan })
    }

    fn execute(
        &mut self,
        request: ExecuteRequest,
        progress: &mut dyn ProgressSink,
    ) -> Result<OperationReceipt, ProviderError> {
        request.plan.validate_digest().map_err(|_| {
            ProviderError::new(
                ProviderErrorCode::InvalidPlan,
                "plan digest is invalid",
                RetryAdvice::Never,
            )
        })?;
        let AuthorizationProof { plan_digest, .. } = &request.authorization;
        if *plan_digest != request.plan.plan_digest {
            return Err(ProviderError::new(
                ProviderErrorCode::AuthorizationMismatch,
                "authorization is not bound to this plan",
                RetryAdvice::Never,
            ));
        }
        if request.requested_at_unix_seconds > request.plan.contents.expires_at_unix_seconds {
            return Err(ProviderError::new(
                ProviderErrorCode::InvalidPlan,
                "plan has expired",
                RetryAdvice::AfterReprobe,
            ));
        }
        if request.plan.contents.host_snapshot_digest != self.scenario.host.snapshot_digest {
            return Err(ProviderError::new(
                ProviderErrorCode::StaleHostFacts,
                "host changed after the plan was created",
                RetryAdvice::AfterReprobe,
            ));
        }

        let operation_id = request.plan.contents.operation_id.clone();
        if self.operations.contains_key(&operation_id) {
            return Err(ProviderError::new(
                ProviderErrorCode::InvalidTransition,
                "operation has already been executed",
                RetryAdvice::Never,
            ));
        }
        let stages = [
            ProgressStage::Queued,
            ProgressStage::Revalidating,
            ProgressStage::Preparing,
            ProgressStage::Complete,
        ];
        let mut events = Vec::new();
        for (index, stage) in stages.into_iter().enumerate() {
            let event = Self::event(
                &operation_id,
                index as u64 + 1,
                request.requested_at_unix_seconds,
                stage,
            );
            progress.emit(event.clone())?;
            events.push(event);
            if self.scenario.interrupt_at == Some(stage) {
                let interrupt_sequence = events.len() as u64 + 1;
                let interrupted = Self::event(
                    &operation_id,
                    interrupt_sequence,
                    request.requested_at_unix_seconds,
                    ProgressStage::Interrupted,
                );
                progress.emit(interrupted.clone())?;
                events.push(interrupted);
                let receipt = OperationReceipt {
                    operation_id: operation_id.clone(),
                    plan_digest: request.plan.plan_digest.clone(),
                    outcome: OperationOutcome::Interrupted,
                    completed_at_unix_seconds: request.requested_at_unix_seconds
                        + interrupt_sequence,
                    final_sequence: interrupt_sequence,
                    result_codes: vec![
                        DiagnosticCode::new("simulator.interrupted").expect("static ID is valid"),
                    ],
                };
                self.operations.insert(
                    operation_id.clone(),
                    SimulatedOperation {
                        snapshot: OperationSnapshot {
                            operation_id,
                            plan_digest: request.plan.plan_digest,
                            stage: ProgressStage::Interrupted,
                            last_sequence: interrupt_sequence,
                            terminal: false,
                        },
                        events,
                        receipt: Some(receipt.clone()),
                    },
                );
                return Ok(receipt);
            }
        }
        let receipt = OperationReceipt {
            operation_id: operation_id.clone(),
            plan_digest: request.plan.plan_digest.clone(),
            outcome: OperationOutcome::Succeeded,
            completed_at_unix_seconds: request.requested_at_unix_seconds + 4,
            final_sequence: 4,
            result_codes: vec![
                DiagnosticCode::new("simulator.succeeded").expect("static ID is valid"),
            ],
        };
        self.operations.insert(
            operation_id.clone(),
            SimulatedOperation {
                snapshot: OperationSnapshot {
                    operation_id,
                    plan_digest: request.plan.plan_digest,
                    stage: ProgressStage::Complete,
                    last_sequence: 4,
                    terminal: true,
                },
                events,
                receipt: Some(receipt.clone()),
            },
        );
        Ok(receipt)
    }

    fn resume(&self, request: ResumeRequest) -> Result<ResumeResponse, ProviderError> {
        let operation = self
            .operations
            .get(&request.operation_id)
            .ok_or_else(Self::operation_not_found)?;
        Ok(ResumeResponse {
            operation: operation.snapshot.clone(),
            events: operation
                .events
                .iter()
                .filter(|event| {
                    request
                        .after_sequence
                        .is_none_or(|after| event.sequence > after)
                })
                .cloned()
                .collect(),
            receipt: operation.receipt.clone(),
        })
    }

    fn recover(&mut self, request: RecoverRequest) -> Result<RecoveryResult, ProviderError> {
        let operation = self
            .operations
            .get(&request.operation_id)
            .ok_or_else(Self::operation_not_found)?;
        let disposition = if operation.snapshot.terminal {
            RecoveryDisposition::NothingToRecover
        } else {
            RecoveryDisposition::RestartFromBeginning
        };
        Ok(RecoveryResult {
            operation: operation.snapshot.clone(),
            disposition,
            explanation_code: DiagnosticCode::new(if operation.snapshot.terminal {
                "recovery.not_required"
            } else {
                "recovery.restart_required"
            })
            .expect("static ID is valid"),
        })
    }

    fn cancel(&mut self, request: CancelRequest) -> Result<CancelResult, ProviderError> {
        let operation = self
            .operations
            .get_mut(&request.operation_id)
            .ok_or_else(Self::operation_not_found)?;
        let disposition = if operation.snapshot.terminal {
            CancelDisposition::AlreadyTerminal
        } else if operation.snapshot.stage == ProgressStage::Interrupted {
            operation.snapshot.stage = ProgressStage::Cancelled;
            operation.snapshot.terminal = true;
            CancelDisposition::Accepted
        } else {
            CancelDisposition::UnsafeAtCurrentStage
        };
        Ok(CancelResult {
            operation: operation.snapshot.clone(),
            disposition,
        })
    }

    fn diagnostics(&self, request: DiagnosticsRequest) -> Result<DiagnosticBundle, ProviderError> {
        let operation = self
            .operations
            .get(&request.operation_id)
            .ok_or_else(Self::operation_not_found)?;
        Ok(DiagnosticBundle {
            operation_id: request.operation_id,
            generated_at_unix_seconds: request.generated_at_unix_seconds,
            entries: vec![
                DiagnosticEntry {
                    code: DiagnosticCode::new("provider.id").expect("static ID is valid"),
                    value: self.descriptor().provider_id.to_string(),
                },
                DiagnosticEntry {
                    code: DiagnosticCode::new("operation.stage").expect("static ID is valid"),
                    value: format!("{:?}", operation.snapshot.stage).to_lowercase(),
                },
            ],
            redaction_version: 1,
            contains_sensitive_data: false,
        })
    }
}
