//! Retain the prepared-filesystem implementation without exposing it as a fallback.
use crate::setup_protocol::{Destination, StagedAction};

pub const STAGED_ISO_BLOCKER: &str = "Installation without USB is waiting for a verified official Omarchy ISO containing same-disk installer support (upstream PR #187). Use a USB installer for now. Prepared-filesystem deployment is retained for future work, but is not enabled.";

pub const STAGED_ISO_TESTING: bool = cfg!(feature = "staged-iso-testing");

pub fn check_destination(destination: &Destination) -> Result<(), String> {
    match destination {
        Destination::StagedIso {
            action: StagedAction::Stage | StagedAction::Inspect | StagedAction::Arm,
            ..
        } if !STAGED_ISO_TESTING => Err(STAGED_ISO_BLOCKER.into()),
        Destination::InspectDirect
        | Destination::DirectX86 { .. }
        | Destination::PrepareRuntime => Err(STAGED_ISO_BLOCKER.into()),
        Destination::PrepareBitLocker {
            reminder_only: false,
        } => Err(
            "BitLocker protection is suspended only during confirmed installer preparation.".into(),
        ),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::setup_protocol::{BootMenu, DirectTarget};

    #[test]
    fn retained_builder_cannot_be_launched_through_desktop_protocol() {
        for destination in [
            Destination::InspectDirect,
            Destination::PrepareRuntime,
            Destination::DirectX86 {
                disk_number: 0,
                disk_unique_id: "fixture".into(),
                target: DirectTarget::Free {
                    start_offset_bytes: 1048576,
                },
                allocation_bytes: 50 * 1024 * 1024 * 1024,
                boot_menu: BootMenu::default(),
            },
        ] {
            assert!(check_destination(&destination).is_err());
        }
    }

    #[test]
    fn encryption_preparation_does_not_enable_direct_installation() {
        assert!(check_destination(&Destination::PrepareBitLocker {
            reminder_only: false
        })
        .is_err());
        assert!(check_destination(&Destination::PrepareBitLocker {
            reminder_only: true
        })
        .is_ok());
        assert!(check_destination(&Destination::InspectDirect).is_err());
    }

    #[test]
    fn release_gate_blocks_new_staging_but_not_owned_cleanup() {
        for action in [
            StagedAction::Stage,
            StagedAction::Inspect,
            StagedAction::Arm,
        ] {
            assert_eq!(
                check_destination(&Destination::StagedIso {
                    action,
                    selection: None,
                    operation_id: None
                })
                .is_ok(),
                STAGED_ISO_TESTING
            );
        }
        for action in [StagedAction::Status, StagedAction::Cleanup] {
            assert!(check_destination(&Destination::StagedIso {
                action,
                selection: None,
                operation_id: None
            })
            .is_ok());
        }
    }
}
