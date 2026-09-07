use omarchy_setup_domain::{
    Architecture, ArtifactRef, CapabilityGrant, CapabilityKind, CapabilityRequest, ElevationKind,
    Fact, HostBinding, HostFacts, HostOs, ImmutablePlan, OperationEvent, OperationId,
    OperationIntent, OperationMachine, OperationState, PlanDigest, PlanError, PlanRequest,
    PolicyDecision, PolicyViolation, ReceiptDigest, SafetyPolicy, SupportLevel, TargetIdentity,
    TransitionError, evaluate_capability, plan,
};

const SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn facts() -> HostFacts {
    HostFacts {
        os: Fact::Known(HostOs::Windows),
        architecture: Fact::Known(Architecture::X86_64),
        elevation: Fact::Known(ElevationKind::WindowsUac),
        virtualization_available: Fact::Known(true),
    }
}

fn policy(kind: CapabilityKind) -> SafetyPolicy {
    SafetyPolicy {
        revision: 7,
        grants: vec![CapabilityGrant {
            kind,
            os: HostOs::Windows,
            architecture: Architecture::X86_64,
            provider_id: None,
            support: SupportLevel::Stable,
        }],
        disabled: vec![],
        allow_experimental: false,
    }
}

fn artifact() -> ArtifactRef {
    ArtifactRef {
        artifact_id: "omarchy-x86_64-1".into(),
        length_bytes: 1024,
        sha256: SHA.into(),
    }
}

fn host() -> HostBinding {
    HostBinding {
        os: HostOs::Windows,
        architecture: Architecture::X86_64,
        fingerprint_sha256: SHA.into(),
    }
}

fn usb_target() -> TargetIdentity {
    TargetIdentity {
        stable_id: "usb:serial-42".into(),
        display_name: "Test USB".into(),
        capacity_bytes: 4096,
        removable: true,
        internal: false,
        system_disk: false,
        identity_ambiguous: false,
    }
}

fn usb_plan() -> ImmutablePlan {
    plan(
        &facts(),
        &policy(CapabilityKind::CreateUsb),
        PlanRequest {
            operation_id: OperationId("operation-1".into()),
            host: host(),
            intent: OperationIntent::CreateUsb {
                artifact: artifact(),
                target: usb_target(),
            },
        },
    )
    .unwrap()
}

#[test]
fn missing_grant_denies_by_default() {
    let decision = evaluate_capability(
        &facts(),
        &SafetyPolicy {
            revision: 1,
            grants: vec![],
            disabled: vec![],
            allow_experimental: false,
        },
        CapabilityRequest {
            kind: CapabilityKind::Download,
            provider_id: None,
        },
    );
    assert_eq!(
        decision,
        PolicyDecision::Denied(PolicyViolation::MissingGrant)
    );
}

#[test]
fn unknown_safety_fact_denies_before_policy_matching() {
    let mut unknown = facts();
    unknown.elevation = Fact::Unknown;
    let decision = evaluate_capability(
        &unknown,
        &policy(CapabilityKind::CreateUsb),
        CapabilityRequest {
            kind: CapabilityKind::CreateUsb,
            provider_id: None,
        },
    );
    assert_eq!(
        decision,
        PolicyDecision::Denied(PolicyViolation::UnknownElevation)
    );
}

#[test]
fn emergency_disable_overrides_an_exact_grant() {
    let mut emergency = policy(CapabilityKind::CreateUsb);
    emergency.disabled.push(CapabilityKind::CreateUsb);
    let decision = evaluate_capability(
        &facts(),
        &emergency,
        CapabilityRequest {
            kind: CapabilityKind::CreateUsb,
            provider_id: None,
        },
    );
    assert_eq!(
        decision,
        PolicyDecision::Denied(PolicyViolation::EmergencyDisabled)
    );
}

#[test]
fn experimental_grant_requires_build_opt_in() {
    let mut gated = policy(CapabilityKind::Download);
    gated.grants[0].support = SupportLevel::Experimental;
    assert_eq!(
        evaluate_capability(
            &facts(),
            &gated,
            CapabilityRequest {
                kind: CapabilityKind::Download,
                provider_id: None,
            },
        ),
        PolicyDecision::Denied(PolicyViolation::ExperimentalDisabled)
    );
}

#[test]
fn duplicate_grants_fail_closed() {
    let mut duplicate = policy(CapabilityKind::Download);
    duplicate.grants.push(duplicate.grants[0].clone());
    assert_eq!(
        evaluate_capability(
            &facts(),
            &duplicate,
            CapabilityRequest {
                kind: CapabilityKind::Download,
                provider_id: None,
            },
        ),
        PolicyDecision::Denied(PolicyViolation::AmbiguousGrant)
    );
}

#[test]
fn usb_plan_derives_the_complete_safe_sequence() {
    let plan = usb_plan();
    assert!(plan.authorization_required());
    assert!(plan.verification_required());
    assert_eq!(plan.steps().len(), 9);
    assert!(matches!(
        plan.steps().last(),
        Some(omarchy_setup_domain::PlanStep::EjectDevice { .. })
    ));
}

#[test]
fn system_disk_is_never_planned_as_usb() {
    let mut target = usb_target();
    target.system_disk = true;
    let result = plan(
        &facts(),
        &policy(CapabilityKind::CreateUsb),
        PlanRequest {
            operation_id: OperationId("operation-2".into()),
            host: host(),
            intent: OperationIntent::CreateUsb {
                artifact: artifact(),
                target,
            },
        },
    );
    assert!(matches!(result, Err(PlanError::UnsafeTarget(_))));
}

#[test]
fn every_unsafe_target_flag_is_rejected() {
    let variants = [
        {
            let mut t = usb_target();
            t.identity_ambiguous = true;
            t
        },
        {
            let mut t = usb_target();
            t.removable = false;
            t
        },
        {
            let mut t = usb_target();
            t.internal = true;
            t
        },
        {
            let mut t = usb_target();
            t.capacity_bytes = 100;
            t
        },
    ];
    for (index, target) in variants.into_iter().enumerate() {
        let result = plan(
            &facts(),
            &policy(CapabilityKind::CreateUsb),
            PlanRequest {
                operation_id: OperationId(format!("operation-{index}")),
                host: host(),
                intent: OperationIntent::CreateUsb {
                    artifact: artifact(),
                    target,
                },
            },
        );
        assert!(matches!(result, Err(PlanError::UnsafeTarget(_))));
    }
}

#[test]
fn canonical_serialization_and_digest_are_deterministic() {
    let first = usb_plan();
    let second = usb_plan();
    assert_eq!(
        first.canonical_bytes().unwrap(),
        second.canonical_bytes().unwrap()
    );
    assert_eq!(first.digest().unwrap(), second.digest().unwrap());
    assert_eq!(first.digest().unwrap().0.len(), 64);
}

#[test]
fn canonical_payload_has_fixed_schema_prefix() {
    let bytes = usb_plan().canonical_bytes().unwrap();
    let json = String::from_utf8(bytes).unwrap();
    assert!(json.starts_with(
        "{\"schema_version\":1,\"operation_id\":\"operation-1\",\"policy_revision\":7,"
    ));
}

#[test]
fn changed_target_changes_digest() {
    let first = usb_plan().digest().unwrap();
    let mut target = usb_target();
    target.capacity_bytes += 1;
    let second = plan(
        &facts(),
        &policy(CapabilityKind::CreateUsb),
        PlanRequest {
            operation_id: OperationId("operation-1".into()),
            host: host(),
            intent: OperationIntent::CreateUsb {
                artifact: artifact(),
                target,
            },
        },
    )
    .unwrap()
    .digest()
    .unwrap();
    assert_ne!(first, second);
}

#[test]
fn privileged_state_machine_requires_confirmation_and_matching_digest() {
    let plan = usb_plan();
    let mut machine = OperationMachine::new(&plan).unwrap();
    machine.apply(OperationEvent::PlanConfirmed).unwrap();
    assert_eq!(machine.state(), &OperationState::AwaitingAuthorization);

    let mismatch = machine.apply(OperationEvent::AuthorizationGranted {
        plan_digest: PlanDigest(
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
        ),
    });
    assert_eq!(mismatch, Err(TransitionError::PlanDigestMismatch));
    assert_eq!(machine.state(), &OperationState::AwaitingAuthorization);

    machine
        .apply(OperationEvent::AuthorizationGranted {
            plan_digest: machine.plan_digest().clone(),
        })
        .unwrap();
    machine
        .apply(OperationEvent::ExecutionStarted {
            plan_digest: machine.plan_digest().clone(),
        })
        .unwrap();
    machine.apply(OperationEvent::VerificationStarted).unwrap();
    machine
        .apply(OperationEvent::Completed {
            receipt_digest: ReceiptDigest(SHA.into()),
        })
        .unwrap();
    assert!(matches!(machine.state(), OperationState::Succeeded { .. }));
}

#[test]
fn verification_cannot_be_skipped() {
    let plan = usb_plan();
    let mut machine = OperationMachine::new(&plan).unwrap();
    machine.apply(OperationEvent::PlanConfirmed).unwrap();
    machine
        .apply(OperationEvent::AuthorizationGranted {
            plan_digest: machine.plan_digest().clone(),
        })
        .unwrap();
    machine
        .apply(OperationEvent::ExecutionStarted {
            plan_digest: machine.plan_digest().clone(),
        })
        .unwrap();
    assert_eq!(
        machine.apply(OperationEvent::Completed {
            receipt_digest: ReceiptDigest(SHA.into()),
        }),
        Err(TransitionError::IllegalTransition)
    );
}

#[test]
fn terminal_state_rejects_later_events() {
    let plan = usb_plan();
    let mut machine = OperationMachine::new(&plan).unwrap();
    machine.apply(OperationEvent::PlanConfirmed).unwrap();
    machine.apply(OperationEvent::AuthorizationDenied).unwrap();
    assert_eq!(machine.state(), &OperationState::Cancelled);
    assert_eq!(
        machine.apply(OperationEvent::PlanConfirmed),
        Err(TransitionError::IllegalTransition)
    );
}
