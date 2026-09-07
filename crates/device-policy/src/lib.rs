//! Pure, fail-closed policy for choosing destructive media targets.
//!
//! The crate consumes observations made by platform-specific code. It neither
//! enumerates devices nor performs raw I/O. A caller first obtains a
//! [`UsbTargetSelection`] with [`select_usb_target`], displays and confirms the
//! exact plan, then takes a fresh observation and calls
//! [`revalidate_usb_target`] immediately before mutation.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;

/// A platform-provided locator used to open a whole device.
///
/// Locators are intentionally treated as volatile. A locator is never an
/// identity and is safe to use only after revalidation against a stable ID.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DeviceLocator(String);

impl DeviceLocator {
    /// Creates a non-empty locator.
    pub fn new(value: impl Into<String>) -> Result<Self, InvalidIdentifier> {
        let value = value.into();
        validate_identifier(&value)?;
        Ok(Self(value))
    }

    /// Returns the platform locator.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DeviceLocator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A platform-derived identity expected to survive re-enumeration.
///
/// Platform adapters should construct this from the strongest identity they
/// can prove (for example, an immutable device instance identifier plus
/// hardware serial). They must not use a device path as the stable ID.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StableDeviceId(String);

impl StableDeviceId {
    /// Creates a non-empty stable identifier.
    pub fn new(value: impl Into<String>) -> Result<Self, InvalidIdentifier> {
        let value = value.into();
        validate_identifier(&value)?;
        Ok(Self(value))
    }

    /// Returns the identifier value.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for StableDeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Error returned when a locator or stable identifier is unusable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvalidIdentifier;

impl fmt::Display for InvalidIdentifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("identifiers must contain at least one non-whitespace character and no NUL")
    }
}

impl Error for InvalidIdentifier {}

fn validate_identifier(value: &str) -> Result<(), InvalidIdentifier> {
    if value.contains('\0') || value.trim().is_empty() {
        Err(InvalidIdentifier)
    } else {
        Ok(())
    }
}

/// Identity confidence reported for an observed device.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IdentityState {
    /// Exactly one stable identity was established.
    Stable(StableDeviceId),
    /// Evidence pointed to multiple possible identities.
    Ambiguous,
    /// No stable identity could be established.
    Missing,
}

/// Whether the operating system classifies the device as removable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Attachment {
    /// A removable device.
    Removable,
    /// An internal/fixed device.
    Internal,
    /// The attachment class cannot be proven.
    Ambiguous,
}

/// Write-access state reported by the platform adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WriteAccess {
    /// The whole device is writable.
    Writable,
    /// The device or media is read-only.
    ReadOnly,
    /// Write access cannot be proven.
    Ambiguous,
}

/// The kind of storage object represented by an observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceScope {
    /// A complete disk/device, suitable for an image write.
    WholeDevice,
    /// A partition, volume, or other child object.
    Child,
    /// The scope cannot be proven.
    Ambiguous,
}

/// The storage roots on which a device depends.
///
/// Roots let policy detect aliases, virtual devices, and child/parent overlap.
/// A normal physical USB disk generally has a one-element set containing its
/// own stable ID. Empty or conflicting topology is treated as ambiguous.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StorageTopology {
    roots: BTreeSet<StableDeviceId>,
    ambiguous: bool,
}

impl StorageTopology {
    /// Creates a topology whose complete set of physical roots is known.
    pub fn known(roots: impl IntoIterator<Item = StableDeviceId>) -> Self {
        Self {
            roots: roots.into_iter().collect(),
            ambiguous: false,
        }
    }

    /// Creates a topology that cannot be resolved safely.
    pub fn ambiguous() -> Self {
        Self {
            roots: BTreeSet::new(),
            ambiguous: true,
        }
    }

    /// Returns the known physical roots.
    pub fn roots(&self) -> &BTreeSet<StableDeviceId> {
        &self.roots
    }

    fn is_usable(&self) -> bool {
        !self.ambiguous && !self.roots.is_empty()
    }
}

/// A complete safety-relevant observation of one target candidate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceObservation {
    /// Volatile platform path/locator.
    pub locator: DeviceLocator,
    /// Stable identity evidence.
    pub identity: IdentityState,
    /// Capacity of the whole observed object in bytes.
    pub capacity_bytes: u64,
    /// Removable/internal classification.
    pub attachment: Attachment,
    /// Current write-access state.
    pub write_access: WriteAccess,
    /// Whole-device versus child classification.
    pub scope: DeviceScope,
    /// Physical storage dependency roots.
    pub topology: StorageTopology,
}

/// A known protected storage endpoint and all of its physical roots.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KnownStorage {
    /// Stable identity of the endpoint itself.
    pub identity: StableDeviceId,
    /// Physical roots backing the endpoint.
    pub topology: StorageTopology,
}

impl KnownStorage {
    fn is_usable(&self) -> bool {
        self.topology.is_usable()
    }
}

/// Whether a safety-critical storage endpoint was resolved.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProtectedStorage {
    /// The endpoint and its topology were resolved.
    Known(KnownStorage),
    /// The endpoint exists but could not be resolved safely.
    Unknown,
}

/// Location of the verified image being written.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ImageSource {
    /// The image does not reside on a local block device.
    NotOnLocalStorage,
    /// The image resides on this local storage endpoint.
    Local(ProtectedStorage),
}

/// Safety-critical host storage facts at one instant.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostStorageContext {
    /// Storage containing the running operating system.
    pub system: ProtectedStorage,
    /// Storage from which the image will be read.
    pub image_source: ImageSource,
}

/// One point-in-time inventory supplied by a platform adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceSnapshot {
    /// Host storage facts captured with this inventory.
    pub host: HostStorageContext,
    /// Observed target candidates. Duplicate locators or IDs are not trusted.
    pub devices: Vec<DeviceObservation>,
}

/// A refusal produced by the fail-closed USB policy.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum UsbPolicyDenial {
    /// The requested locator is absent from the fresh snapshot.
    DeviceNotFound,
    /// A locator appears more than once in one snapshot.
    DuplicateLocator,
    /// No unique stable device identity is available.
    AmbiguousIdentity,
    /// The same supposedly stable identity appears on multiple observations.
    DuplicateStableIdentity,
    /// The device or host topology is incomplete or contradictory.
    AmbiguousTopology,
    /// The running system disk could not be resolved.
    UnknownSystemDisk,
    /// The local source disk could not be resolved.
    UnknownSourceDisk,
    /// The selected object is or overlaps the running system disk.
    SystemDisk,
    /// The selected object is or overlaps the image source disk.
    SourceDisk,
    /// The device is internal/fixed.
    InternalDevice,
    /// Removability cannot be proven.
    AmbiguousAttachment,
    /// The device is read-only.
    ReadOnly,
    /// Write access cannot be proven.
    AmbiguousWriteAccess,
    /// The target is not proven to be a whole device.
    NotWholeDevice,
    /// The target is smaller than the image.
    Undersized {
        /// Observed target capacity.
        capacity_bytes: u64,
        /// Required image length.
        required_bytes: u64,
    },
    /// The required image length is invalid.
    EmptyImage,
    /// A safety-relevant fact changed after user selection.
    ChangedSinceSelection,
}

impl fmt::Display for UsbPolicyDenial {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DeviceNotFound => f.write_str("device is not present"),
            Self::DuplicateLocator => f.write_str("device locator is not unique"),
            Self::AmbiguousIdentity => f.write_str("device identity is not uniquely established"),
            Self::DuplicateStableIdentity => {
                f.write_str("stable device identity occurs more than once")
            }
            Self::AmbiguousTopology => f.write_str("storage topology is ambiguous"),
            Self::UnknownSystemDisk => f.write_str("running system disk is unknown"),
            Self::UnknownSourceDisk => f.write_str("local image source disk is unknown"),
            Self::SystemDisk => f.write_str("target is or overlaps the running system disk"),
            Self::SourceDisk => f.write_str("target is or overlaps the image source disk"),
            Self::InternalDevice => f.write_str("target is an internal device"),
            Self::AmbiguousAttachment => f.write_str("target removability is ambiguous"),
            Self::ReadOnly => f.write_str("target is read-only"),
            Self::AmbiguousWriteAccess => f.write_str("target write access is ambiguous"),
            Self::NotWholeDevice => f.write_str("target is not proven to be a whole device"),
            Self::Undersized {
                capacity_bytes,
                required_bytes,
            } => write!(
                f,
                "target has {capacity_bytes} bytes but the image requires {required_bytes} bytes"
            ),
            Self::EmptyImage => f.write_str("image length must be non-zero"),
            Self::ChangedSinceSelection => {
                f.write_str("target or protected storage changed after selection")
            }
        }
    }
}

impl Error for UsbPolicyDenial {}

/// Immutable evidence captured when the user selects a USB target.
///
/// Fields are deliberately private so callers cannot synthesize or alter a
/// successful selection without passing policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UsbTargetSelection {
    observation: DeviceObservation,
    host: HostStorageContext,
    image_size_bytes: u64,
}

impl UsbTargetSelection {
    /// Returns the exact locator shown and confirmed at selection time.
    pub fn locator(&self) -> &DeviceLocator {
        &self.observation.locator
    }

    /// Returns the selected stable identity.
    pub fn stable_id(&self) -> &StableDeviceId {
        match &self.observation.identity {
            IdentityState::Stable(identity) => identity,
            IdentityState::Ambiguous | IdentityState::Missing => {
                unreachable!("policy never creates a selection without a stable identity")
            }
        }
    }

    /// Returns the capacity displayed and confirmed at selection time.
    pub fn capacity_bytes(&self) -> u64 {
        self.observation.capacity_bytes
    }

    /// Returns the exact image length used for selection.
    pub fn image_size_bytes(&self) -> u64 {
        self.image_size_bytes
    }
}

/// Revalidated evidence that is safe to pass to the next typed operation.
///
/// This authorization is only a policy result. It does not lock the device;
/// callers must keep the fresh snapshot-to-open interval minimal and have the
/// privileged helper perform its own identity check after opening the handle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RevalidatedUsbTarget {
    observation: DeviceObservation,
    image_size_bytes: u64,
}

impl RevalidatedUsbTarget {
    /// Returns the freshly revalidated device locator.
    pub fn locator(&self) -> &DeviceLocator {
        &self.observation.locator
    }

    /// Returns the freshly revalidated stable identity.
    pub fn stable_id(&self) -> &StableDeviceId {
        match &self.observation.identity {
            IdentityState::Stable(identity) => identity,
            IdentityState::Ambiguous | IdentityState::Missing => {
                unreachable!("revalidation only succeeds for a stable identity")
            }
        }
    }

    /// Returns the authorized image length.
    pub fn image_size_bytes(&self) -> u64 {
        self.image_size_bytes
    }
}

/// Classifies a candidate and captures the evidence needed for revalidation.
pub fn select_usb_target(
    snapshot: &DeviceSnapshot,
    locator: &DeviceLocator,
    image_size_bytes: u64,
) -> Result<UsbTargetSelection, UsbPolicyDenial> {
    let observation = classify(snapshot, locator, image_size_bytes)?;
    Ok(UsbTargetSelection {
        observation: observation.clone(),
        host: snapshot.host.clone(),
        image_size_bytes,
    })
}

/// Re-identifies and reclassifies a selected target from a fresh snapshot.
///
/// Any change to the selected observation or protected host storage context is
/// a denial, including an otherwise safe-looking device reusing the old path.
pub fn revalidate_usb_target(
    selection: &UsbTargetSelection,
    fresh_snapshot: &DeviceSnapshot,
) -> Result<RevalidatedUsbTarget, UsbPolicyDenial> {
    let fresh = match classify(
        fresh_snapshot,
        selection.locator(),
        selection.image_size_bytes,
    ) {
        Ok(observation) => observation,
        Err(UsbPolicyDenial::DeviceNotFound) => return Err(UsbPolicyDenial::ChangedSinceSelection),
        Err(error) => return Err(error),
    };

    if fresh != &selection.observation || fresh_snapshot.host != selection.host {
        return Err(UsbPolicyDenial::ChangedSinceSelection);
    }

    Ok(RevalidatedUsbTarget {
        observation: fresh.clone(),
        image_size_bytes: selection.image_size_bytes,
    })
}

fn classify<'a>(
    snapshot: &'a DeviceSnapshot,
    locator: &DeviceLocator,
    image_size_bytes: u64,
) -> Result<&'a DeviceObservation, UsbPolicyDenial> {
    if image_size_bytes == 0 {
        return Err(UsbPolicyDenial::EmptyImage);
    }

    let locator_count = snapshot
        .devices
        .iter()
        .filter(|device| &device.locator == locator)
        .count();
    let candidate = match locator_count {
        0 => return Err(UsbPolicyDenial::DeviceNotFound),
        1 => snapshot
            .devices
            .iter()
            .find(|device| &device.locator == locator)
            .expect("count established one matching locator"),
        _ => return Err(UsbPolicyDenial::DuplicateLocator),
    };

    let identity = match &candidate.identity {
        IdentityState::Stable(identity) => identity,
        IdentityState::Ambiguous | IdentityState::Missing => {
            return Err(UsbPolicyDenial::AmbiguousIdentity)
        }
    };

    let id_counts = stable_id_counts(&snapshot.devices);
    if id_counts.get(identity).copied() != Some(1) {
        return Err(UsbPolicyDenial::DuplicateStableIdentity);
    }

    if !candidate.topology.is_usable() {
        return Err(UsbPolicyDenial::AmbiguousTopology);
    }
    let system = known_protected(&snapshot.host.system, UsbPolicyDenial::UnknownSystemDisk)?;
    if overlaps(candidate, system) {
        return Err(UsbPolicyDenial::SystemDisk);
    }
    if let ImageSource::Local(source) = &snapshot.host.image_source {
        let source = known_protected(source, UsbPolicyDenial::UnknownSourceDisk)?;
        if overlaps(candidate, source) {
            return Err(UsbPolicyDenial::SourceDisk);
        }
    }

    match candidate.attachment {
        Attachment::Removable => {}
        Attachment::Internal => return Err(UsbPolicyDenial::InternalDevice),
        Attachment::Ambiguous => return Err(UsbPolicyDenial::AmbiguousAttachment),
    }
    match candidate.write_access {
        WriteAccess::Writable => {}
        WriteAccess::ReadOnly => return Err(UsbPolicyDenial::ReadOnly),
        WriteAccess::Ambiguous => return Err(UsbPolicyDenial::AmbiguousWriteAccess),
    }
    if candidate.scope != DeviceScope::WholeDevice {
        return Err(UsbPolicyDenial::NotWholeDevice);
    }
    if candidate.capacity_bytes < image_size_bytes {
        return Err(UsbPolicyDenial::Undersized {
            capacity_bytes: candidate.capacity_bytes,
            required_bytes: image_size_bytes,
        });
    }

    Ok(candidate)
}

fn stable_id_counts(devices: &[DeviceObservation]) -> BTreeMap<&StableDeviceId, usize> {
    let mut counts = BTreeMap::new();
    for device in devices {
        if let IdentityState::Stable(identity) = &device.identity {
            *counts.entry(identity).or_insert(0) += 1;
        }
    }
    counts
}

fn known_protected(
    protected: &ProtectedStorage,
    unknown_error: UsbPolicyDenial,
) -> Result<&KnownStorage, UsbPolicyDenial> {
    match protected {
        ProtectedStorage::Known(storage) if storage.is_usable() => Ok(storage),
        ProtectedStorage::Known(_) => Err(UsbPolicyDenial::AmbiguousTopology),
        ProtectedStorage::Unknown => Err(unknown_error),
    }
}

fn overlaps(candidate: &DeviceObservation, protected: &KnownStorage) -> bool {
    let candidate_identity_matches = match &candidate.identity {
        IdentityState::Stable(identity) => identity == &protected.identity,
        IdentityState::Ambiguous | IdentityState::Missing => false,
    };
    candidate_identity_matches
        || candidate.topology.roots().contains(&protected.identity)
        || protected.topology.roots().iter().any(|root| {
            candidate.topology.roots().contains(root)
                || matches!(&candidate.identity, IdentityState::Stable(identity) if identity == root)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const IMAGE_SIZE: u64 = 4_000_000_000;

    fn id(value: &str) -> StableDeviceId {
        StableDeviceId::new(value).unwrap()
    }

    fn locator(value: &str) -> DeviceLocator {
        DeviceLocator::new(value).unwrap()
    }

    fn topology(value: &str) -> StorageTopology {
        StorageTopology::known([id(value)])
    }

    fn known(value: &str) -> ProtectedStorage {
        ProtectedStorage::Known(KnownStorage {
            identity: id(value),
            topology: topology(value),
        })
    }

    fn safe_device(path: &str, identity: &str, capacity_bytes: u64) -> DeviceObservation {
        DeviceObservation {
            locator: locator(path),
            identity: IdentityState::Stable(id(identity)),
            capacity_bytes,
            attachment: Attachment::Removable,
            write_access: WriteAccess::Writable,
            scope: DeviceScope::WholeDevice,
            topology: topology(identity),
        }
    }

    fn snapshot(device: DeviceObservation) -> DeviceSnapshot {
        DeviceSnapshot {
            host: HostStorageContext {
                system: known("system"),
                image_source: ImageSource::Local(known("source")),
            },
            devices: vec![device],
        }
    }

    #[test]
    fn safe_removable_whole_device_is_accepted_and_revalidated() {
        let snapshot = snapshot(safe_device("disk-7", "usb-7", IMAGE_SIZE + 1));
        let selection = select_usb_target(&snapshot, &locator("disk-7"), IMAGE_SIZE).unwrap();
        let target = revalidate_usb_target(&selection, &snapshot).unwrap();
        assert_eq!(target.stable_id(), &id("usb-7"));
    }

    #[test]
    fn refuses_every_required_unsafe_class() {
        let mut device = safe_device("disk-7", "usb-7", IMAGE_SIZE + 1);

        device.attachment = Attachment::Internal;
        assert_eq!(
            select_usb_target(&snapshot(device.clone()), &device.locator, IMAGE_SIZE),
            Err(UsbPolicyDenial::InternalDevice)
        );

        device.attachment = Attachment::Ambiguous;
        assert_eq!(
            select_usb_target(&snapshot(device.clone()), &device.locator, IMAGE_SIZE),
            Err(UsbPolicyDenial::AmbiguousAttachment)
        );

        device.attachment = Attachment::Removable;
        device.write_access = WriteAccess::ReadOnly;
        assert_eq!(
            select_usb_target(&snapshot(device.clone()), &device.locator, IMAGE_SIZE),
            Err(UsbPolicyDenial::ReadOnly)
        );

        device.write_access = WriteAccess::Writable;
        device.capacity_bytes = IMAGE_SIZE - 1;
        assert_eq!(
            select_usb_target(&snapshot(device.clone()), &device.locator, IMAGE_SIZE),
            Err(UsbPolicyDenial::Undersized {
                capacity_bytes: IMAGE_SIZE - 1,
                required_bytes: IMAGE_SIZE,
            })
        );
    }

    #[test]
    fn refuses_system_and_source_topology_overlap() {
        let system = safe_device("disk-0", "system", IMAGE_SIZE + 1);
        assert_eq!(
            select_usb_target(&snapshot(system.clone()), &system.locator, IMAGE_SIZE),
            Err(UsbPolicyDenial::SystemDisk)
        );

        let source = safe_device("disk-1", "source", IMAGE_SIZE + 1);
        assert_eq!(
            select_usb_target(&snapshot(source.clone()), &source.locator, IMAGE_SIZE),
            Err(UsbPolicyDenial::SourceDisk)
        );

        let mut alias = safe_device("disk-2", "virtual-usb", IMAGE_SIZE + 1);
        alias.topology = StorageTopology::known([id("system")]);
        assert_eq!(
            select_usb_target(&snapshot(alias.clone()), &alias.locator, IMAGE_SIZE),
            Err(UsbPolicyDenial::SystemDisk)
        );
    }

    #[test]
    fn unknown_protected_storage_fails_closed() {
        let device = safe_device("disk-7", "usb-7", IMAGE_SIZE + 1);
        let path = device.locator.clone();
        let mut current = snapshot(device);
        current.host.system = ProtectedStorage::Unknown;
        assert_eq!(
            select_usb_target(&current, &path, IMAGE_SIZE),
            Err(UsbPolicyDenial::UnknownSystemDisk)
        );

        current.host.system = known("system");
        current.host.image_source = ImageSource::Local(ProtectedStorage::Unknown);
        assert_eq!(
            select_usb_target(&current, &path, IMAGE_SIZE),
            Err(UsbPolicyDenial::UnknownSourceDisk)
        );
    }

    #[test]
    fn any_safety_relevant_change_is_refused() {
        let original = snapshot(safe_device("disk-7", "usb-7", IMAGE_SIZE + 1));
        let selection = select_usb_target(&original, &locator("disk-7"), IMAGE_SIZE).unwrap();

        let mut changed = original.clone();
        changed.devices[0].capacity_bytes += 1;
        assert_eq!(
            revalidate_usb_target(&selection, &changed),
            Err(UsbPolicyDenial::ChangedSinceSelection)
        );

        let mut changed_host = original.clone();
        changed_host.host.image_source = ImageSource::NotOnLocalStorage;
        assert_eq!(
            revalidate_usb_target(&selection, &changed_host),
            Err(UsbPolicyDenial::ChangedSinceSelection)
        );
    }

    proptest! {
        #[test]
        fn duplicate_stable_ids_always_fail_closed(
            suffix in "[A-Za-z0-9_-]{1,32}",
            first_capacity in (IMAGE_SIZE..u64::MAX),
            second_capacity in (IMAGE_SIZE..u64::MAX),
        ) {
            let shared = format!("shared-{suffix}");
            let first = safe_device("disk-a", &shared, first_capacity);
            let second = safe_device("disk-b", &shared, second_capacity);
            let mut current = snapshot(first);
            current.devices.push(second);

            prop_assert_eq!(
                select_usb_target(&current, &locator("disk-a"), IMAGE_SIZE),
                Err(UsbPolicyDenial::DuplicateStableIdentity)
            );
        }

        #[test]
        fn device_path_reuse_with_another_id_never_revalidates(
            old_suffix in "[A-Za-z0-9_-]{1,32}",
            new_suffix in "[A-Za-z0-9_-]{1,32}",
            capacity in (IMAGE_SIZE..u64::MAX),
        ) {
            prop_assume!(old_suffix != new_suffix);
            let old_id = format!("old-{old_suffix}");
            let new_id = format!("new-{new_suffix}");
            let selected_snapshot = snapshot(safe_device("reused-path", &old_id, capacity));
            let selection = select_usb_target(
                &selected_snapshot,
                &locator("reused-path"),
                IMAGE_SIZE,
            ).unwrap();
            let fresh_snapshot = snapshot(safe_device("reused-path", &new_id, capacity));

            prop_assert_eq!(
                revalidate_usb_target(&selection, &fresh_snapshot),
                Err(UsbPolicyDenial::ChangedSinceSelection)
            );
        }

        #[test]
        fn duplicate_locators_never_select(
            suffix_a in "[A-Za-z0-9_-]{1,32}",
            suffix_b in "[A-Za-z0-9_-]{1,32}",
        ) {
            let first = safe_device("same-path", &format!("a-{suffix_a}"), IMAGE_SIZE);
            let second = safe_device("same-path", &format!("b-{suffix_b}"), IMAGE_SIZE);
            let mut current = snapshot(first);
            current.devices.push(second);

            prop_assert_eq!(
                select_usb_target(&current, &locator("same-path"), IMAGE_SIZE),
                Err(UsbPolicyDenial::DuplicateLocator)
            );
        }

        #[test]
        fn undersized_devices_never_select(capacity in 0..IMAGE_SIZE) {
            let device = safe_device("disk-small", "usb-small", capacity);
            prop_assert_eq!(
                select_usb_target(&snapshot(device), &locator("disk-small"), IMAGE_SIZE),
                Err(UsbPolicyDenial::Undersized {
                    capacity_bytes: capacity,
                    required_bytes: IMAGE_SIZE,
                })
            );
        }
    }
}
