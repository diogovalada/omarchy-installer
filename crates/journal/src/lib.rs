//! Durable operation journal for destructive or restartable setup operations.
//!
//! Each entry is a bounded JSON line whose SHA-256 digest covers its content and
//! the preceding entry's digest. Filesystem journals also maintain two rotating
//! head anchors. This detects partial writes, internal edits, reordering, and a
//! log that has been shortened or extended without its committed anchor.
//!
//! The chain is an integrity mechanism, not authentication: an attacker able to
//! rewrite both the journal and its anchors can construct a new valid chain.
//! Never put credentials, paths containing user names, command output, or other
//! sensitive data in a journal. The public event model intentionally accepts
//! only UUIDs and short, restricted identifiers.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fmt,
    fs::OpenOptions,
    io::{self, Write},
    path::{Path, PathBuf},
    str::FromStr,
    time::{SystemTime, UNIX_EPOCH},
};
use thiserror::Error;
use uuid::Uuid;

const SCHEMA_VERSION: u32 = 1;
const MAX_ENTRY_BYTES: usize = 16 * 1024;
const MAX_JOURNAL_BYTES: usize = 64 * 1024 * 1024;
const MAX_IDENTIFIER_BYTES: usize = 96;

/// Unique identity of one execution. It cannot be changed after the first entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OperationId(Uuid);

/// Identity of the immutable plan authorized for an operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PlanId(Uuid);

macro_rules! uuid_id {
    ($type:ident) => {
        impl $type {
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }

            pub fn from_uuid(value: Uuid) -> Self {
                Self(value)
            }

            pub fn as_uuid(self) -> Uuid {
                self.0
            }
        }

        impl Default for $type {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $type {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }

        impl FromStr for $type {
            type Err = uuid::Error;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Uuid::parse_str(value).map(Self)
            }
        }
    };
}

uuid_id!(OperationId);
uuid_id!(PlanId);

/// A deliberately restricted, non-sensitive identifier used for steps/codes.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct JournalCode(String);

impl JournalCode {
    pub fn new(value: impl Into<String>) -> Result<Self, JournalError> {
        let value = value.into();
        validate_identifier(&value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for JournalCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for JournalCode {
    type Err = JournalError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

fn validate_identifier(value: &str) -> Result<(), JournalError> {
    let valid = !value.is_empty()
        && value.len() <= MAX_IDENTIFIER_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'));
    if valid {
        Ok(())
    } else {
        Err(JournalError::UnsafeIdentifier)
    }
}

/// Durable progress states. Transitions must occur in this order for each step.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckpointState {
    Prepared,
    Applied,
    Verified,
}

impl CheckpointState {
    fn ordinal(self) -> u8 {
        match self {
            Self::Prepared => 0,
            Self::Applied => 1,
            Self::Verified => 2,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum TerminalState {
    Succeeded,
    Failed { code: JournalCode },
    Cancelled { code: JournalCode },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum JournalEvent {
    Started,
    Checkpoint {
        step: JournalCode,
        state: CheckpointState,
    },
    Terminal {
        state: TerminalState,
    },
}

impl JournalEvent {
    fn validate(&self) -> Result<(), JournalError> {
        match self {
            Self::Started => Ok(()),
            Self::Checkpoint { step, .. } => validate_identifier(step.as_str()),
            Self::Terminal {
                state: TerminalState::Failed { code } | TerminalState::Cancelled { code },
            } => validate_identifier(code.as_str()),
            Self::Terminal {
                state: TerminalState::Succeeded,
            } => Ok(()),
        }
    }
}

/// One immutable record in the chain.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JournalEntry {
    pub schema_version: u32,
    pub sequence: u64,
    pub operation_id: OperationId,
    pub plan_id: PlanId,
    pub recorded_unix_ms: u64,
    pub previous_hash: Option<String>,
    pub event: JournalEvent,
    pub entry_hash: String,
}

#[derive(Serialize)]
struct HashMaterial<'a> {
    schema_version: u32,
    sequence: u64,
    operation_id: OperationId,
    plan_id: PlanId,
    recorded_unix_ms: u64,
    previous_hash: &'a Option<String>,
    event: &'a JournalEvent,
}

impl JournalEntry {
    fn calculate_hash(&self) -> Result<String, JournalError> {
        let material = HashMaterial {
            schema_version: self.schema_version,
            sequence: self.sequence,
            operation_id: self.operation_id,
            plan_id: self.plan_id,
            recorded_unix_ms: self.recorded_unix_ms,
            previous_hash: &self.previous_hash,
            event: &self.event,
        };
        let bytes = serde_json::to_vec(&material)?;
        Ok(hex_digest(Sha256::digest(bytes).as_slice()))
    }
}

fn hex_digest(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReplayState {
    Active,
    Terminal(TerminalState),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplayReport {
    pub operation_id: OperationId,
    pub plan_id: PlanId,
    pub state: ReplayState,
    pub checkpoints: BTreeMap<JournalCode, CheckpointState>,
    pub entries: Vec<JournalEntry>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeadAnchor {
    schema_version: u32,
    generation: u64,
    entry_count: u64,
    head_hash: String,
    anchor_hash: String,
}

impl HeadAnchor {
    fn new(generation: u64, entry_count: u64, head_hash: String) -> Self {
        let mut anchor = Self {
            schema_version: SCHEMA_VERSION,
            generation,
            entry_count,
            head_hash,
            anchor_hash: String::new(),
        };
        anchor.anchor_hash = anchor.calculate_hash();
        anchor
    }

    fn calculate_hash(&self) -> String {
        let mut digest = Sha256::new();
        digest.update(self.schema_version.to_le_bytes());
        digest.update(self.generation.to_le_bytes());
        digest.update(self.entry_count.to_le_bytes());
        digest.update(self.head_hash.as_bytes());
        hex_digest(&digest.finalize())
    }

    fn valid(&self) -> bool {
        self.schema_version == SCHEMA_VERSION
            && self.head_hash.len() == 64
            && self.anchor_hash == self.calculate_hash()
    }
}

/// Bytes and the last separately committed head observed by a storage backend.
pub struct StoredJournal {
    pub bytes: Vec<u8>,
    pub anchor: Option<HeadAnchor>,
}

/// Minimal backend contract. `commit` must not publish the anchor before data.
pub trait JournalStorage: Send {
    fn load(&mut self) -> io::Result<StoredJournal>;
    fn commit(&mut self, record: &[u8], anchor: &HeadAnchor) -> io::Result<()>;
}

/// Deterministic storage useful for simulations and tests.
#[derive(Default, Debug)]
pub struct MemoryStorage {
    bytes: Vec<u8>,
    anchor: Option<HeadAnchor>,
}

impl MemoryStorage {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn anchor(&self) -> Option<&HeadAnchor> {
        self.anchor.as_ref()
    }

    pub fn into_parts(self) -> (Vec<u8>, Option<HeadAnchor>) {
        (self.bytes, self.anchor)
    }

    pub fn from_parts(bytes: Vec<u8>, anchor: Option<HeadAnchor>) -> Self {
        Self { bytes, anchor }
    }
}

impl JournalStorage for MemoryStorage {
    fn load(&mut self) -> io::Result<StoredJournal> {
        Ok(StoredJournal {
            bytes: self.bytes.clone(),
            anchor: self.anchor.clone(),
        })
    }

    fn commit(&mut self, record: &[u8], anchor: &HeadAnchor) -> io::Result<()> {
        self.bytes.extend_from_slice(record);
        self.anchor = Some(anchor.clone());
        Ok(())
    }
}

/// Append-only file plus two rotating head anchors.
///
/// A complete line is flushed before the next anchor slot is replaced. The
/// other slot remains a fallback if the machine stops while writing an anchor.
/// Access from multiple processes must be serialized by the application.
pub struct FileStorage {
    path: PathBuf,
    anchor_paths: [PathBuf; 2],
    generation: Option<u64>,
}

impl FileStorage {
    pub fn new(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref().to_owned();
        Self {
            anchor_paths: [
                suffixed_path(&path, ".head.0"),
                suffixed_path(&path, ".head.1"),
            ],
            path,
            generation: None,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn read_anchor(path: &Path) -> io::Result<Option<HeadAnchor>> {
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        let anchor: HeadAnchor = match serde_json::from_slice(&bytes) {
            Ok(anchor) => anchor,
            Err(_) => return Ok(None),
        };
        Ok(anchor.valid().then_some(anchor))
    }
}

fn suffixed_path(path: &Path, suffix: &str) -> PathBuf {
    let mut name: OsString = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

impl JournalStorage for FileStorage {
    fn load(&mut self) -> io::Result<StoredJournal> {
        let bytes = match std::fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(error),
        };
        let anchors = [
            Self::read_anchor(&self.anchor_paths[0])?,
            Self::read_anchor(&self.anchor_paths[1])?,
        ];
        let anchor = anchors
            .into_iter()
            .flatten()
            .max_by_key(|item| item.generation);
        self.generation = anchor.as_ref().map(|item| item.generation);
        Ok(StoredJournal { bytes, anchor })
    }

    fn commit(&mut self, record: &[u8], anchor: &HeadAnchor) -> io::Result<()> {
        if let Some(parent) = self
            .path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent)?;
        }
        let mut log = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        log.write_all(record)?;
        log.sync_data()?;

        let slot = (anchor.generation % 2) as usize;
        let encoded = serde_json::to_vec(anchor).map_err(io::Error::other)?;
        let mut head = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&self.anchor_paths[slot])?;
        head.write_all(&encoded)?;
        head.sync_all()?;
        self.generation = Some(anchor.generation);
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum JournalError {
    #[error("journal storage failed: {0}")]
    Storage(#[from] io::Error),
    #[error("journal encoding is invalid: {0}")]
    Encoding(#[from] serde_json::Error),
    #[error("journal is empty")]
    Empty,
    #[error("journal exceeds its size limit")]
    JournalTooLarge,
    #[error("journal entry exceeds its size limit")]
    EntryTooLarge,
    #[error("journal ends with a partial entry")]
    TruncatedEntry,
    #[error("journal contains an empty entry")]
    EmptyEntry,
    #[error("unsupported journal schema at sequence {0}")]
    UnsupportedSchema(u64),
    #[error("invalid sequence at entry {0}")]
    InvalidSequence(usize),
    #[error("operation or plan identity changed at sequence {0}")]
    IdentityChanged(u64),
    #[error("hash chain is invalid at sequence {0}")]
    InvalidHash(u64),
    #[error("journal head anchor is missing or does not match the log")]
    AnchorMismatch,
    #[error("the first event is not started, or started appears more than once")]
    InvalidStart,
    #[error("an event appears after the terminal state")]
    EventAfterTerminal,
    #[error("journal is already terminal")]
    AlreadyTerminal,
    #[error("checkpoint transition is invalid for step {0}")]
    InvalidCheckpoint(String),
    #[error("identifier must be 1-96 ASCII letters, digits, '.', '_', ':', or '-'")]
    UnsafeIdentifier,
    #[error("system clock is before the Unix epoch")]
    Clock,
}

/// Validate and replay unanchored journal bytes.
///
/// This verifies line completeness, hashes, IDs and lifecycle rules. Prefer
/// [`Journal::open`] for stored journals because an anchor is required there to
/// detect removal/addition of complete trailing entries.
pub fn replay(bytes: &[u8]) -> Result<ReplayReport, JournalError> {
    replay_inner(bytes, None, false)
}

fn replay_inner(
    bytes: &[u8],
    anchor: Option<&HeadAnchor>,
    require_anchor: bool,
) -> Result<ReplayReport, JournalError> {
    if bytes.is_empty() {
        return Err(JournalError::Empty);
    }
    if bytes.len() > MAX_JOURNAL_BYTES {
        return Err(JournalError::JournalTooLarge);
    }
    if !bytes.ends_with(b"\n") {
        return Err(JournalError::TruncatedEntry);
    }

    let mut entries = Vec::new();
    let mut operation_id = None;
    let mut plan_id = None;
    let mut previous_hash: Option<String> = None;
    let mut checkpoints = BTreeMap::new();
    let mut state = ReplayState::Active;

    for (index, line_with_newline) in bytes.split_inclusive(|byte| *byte == b'\n').enumerate() {
        let line = &line_with_newline[..line_with_newline.len() - 1];
        if line.is_empty() {
            return Err(JournalError::EmptyEntry);
        }
        if line.len() > MAX_ENTRY_BYTES {
            return Err(JournalError::EntryTooLarge);
        }
        let entry: JournalEntry = serde_json::from_slice(line)?;
        entry.event.validate()?;
        if entry.schema_version != SCHEMA_VERSION {
            return Err(JournalError::UnsupportedSchema(entry.sequence));
        }
        if entry.sequence != index as u64 {
            return Err(JournalError::InvalidSequence(index));
        }
        if index == 0 {
            operation_id = Some(entry.operation_id);
            plan_id = Some(entry.plan_id);
            if entry.previous_hash.is_some() || entry.event != JournalEvent::Started {
                return Err(JournalError::InvalidStart);
            }
        } else {
            if Some(entry.operation_id) != operation_id || Some(entry.plan_id) != plan_id {
                return Err(JournalError::IdentityChanged(entry.sequence));
            }
            if matches!(entry.event, JournalEvent::Started) {
                return Err(JournalError::InvalidStart);
            }
            if matches!(state, ReplayState::Terminal(_)) {
                return Err(JournalError::EventAfterTerminal);
            }
        }
        if entry.previous_hash != previous_hash || entry.calculate_hash()? != entry.entry_hash {
            return Err(JournalError::InvalidHash(entry.sequence));
        }

        apply_event(&entry.event, &mut checkpoints, &mut state)?;
        previous_hash = Some(entry.entry_hash.clone());
        entries.push(entry);
    }

    if require_anchor {
        let Some(anchor) = anchor.filter(|item| item.valid()) else {
            return Err(JournalError::AnchorMismatch);
        };
        if anchor.entry_count != entries.len() as u64
            || Some(anchor.head_hash.as_str()) != previous_hash.as_deref()
        {
            return Err(JournalError::AnchorMismatch);
        }
    }

    Ok(ReplayReport {
        operation_id: operation_id.expect("nonempty journal"),
        plan_id: plan_id.expect("nonempty journal"),
        state,
        checkpoints,
        entries,
    })
}

fn apply_event(
    event: &JournalEvent,
    checkpoints: &mut BTreeMap<JournalCode, CheckpointState>,
    state: &mut ReplayState,
) -> Result<(), JournalError> {
    match event {
        JournalEvent::Started => {}
        JournalEvent::Checkpoint { step, state: next } => {
            let expected = checkpoints
                .get(step)
                .map(|prior| prior.ordinal() + 1)
                .unwrap_or(0);
            if next.ordinal() != expected {
                return Err(JournalError::InvalidCheckpoint(step.to_string()));
            }
            checkpoints.insert(step.clone(), *next);
        }
        JournalEvent::Terminal { state: terminal } => {
            *state = ReplayState::Terminal(terminal.clone());
        }
    }
    Ok(())
}

/// An opened, process-local journal. Callers must serialize access to a path.
pub struct Journal<S: JournalStorage> {
    storage: S,
    report: ReplayReport,
    anchor_generation: u64,
}

impl<S: JournalStorage> Journal<S> {
    pub fn create(
        mut storage: S,
        operation_id: OperationId,
        plan_id: PlanId,
    ) -> Result<Self, JournalError> {
        let stored = storage.load()?;
        if !stored.bytes.is_empty() || stored.anchor.is_some() {
            return Err(JournalError::InvalidStart);
        }
        let entry = make_entry(0, operation_id, plan_id, None, JournalEvent::Started)?;
        let record = encode_record(&entry)?;
        let anchor = HeadAnchor::new(0, 1, entry.entry_hash.clone());
        storage.commit(&record, &anchor)?;
        Ok(Self {
            storage,
            report: ReplayReport {
                operation_id,
                plan_id,
                state: ReplayState::Active,
                checkpoints: BTreeMap::new(),
                entries: vec![entry],
            },
            anchor_generation: 0,
        })
    }

    pub fn open(mut storage: S) -> Result<Self, JournalError> {
        let stored = storage.load()?;
        let anchor_generation = stored
            .anchor
            .as_ref()
            .map(|item| item.generation)
            .unwrap_or(0);
        let report = replay_inner(&stored.bytes, stored.anchor.as_ref(), true)?;
        Ok(Self {
            storage,
            report,
            anchor_generation,
        })
    }

    pub fn report(&self) -> &ReplayReport {
        &self.report
    }

    pub fn append_checkpoint(
        &mut self,
        step: JournalCode,
        state: CheckpointState,
    ) -> Result<&JournalEntry, JournalError> {
        self.append(JournalEvent::Checkpoint { step, state })
    }

    pub fn finish(&mut self, state: TerminalState) -> Result<&JournalEntry, JournalError> {
        self.append(JournalEvent::Terminal { state })
    }

    pub fn into_storage(self) -> S {
        self.storage
    }

    fn append(&mut self, event: JournalEvent) -> Result<&JournalEntry, JournalError> {
        if matches!(self.report.state, ReplayState::Terminal(_)) {
            return Err(JournalError::AlreadyTerminal);
        }
        event.validate()?;

        let mut checkpoints = self.report.checkpoints.clone();
        let mut replay_state = self.report.state.clone();
        apply_event(&event, &mut checkpoints, &mut replay_state)?;

        let previous_hash = self
            .report
            .entries
            .last()
            .map(|entry| entry.entry_hash.clone());
        let sequence = self.report.entries.len() as u64;
        let entry = make_entry(
            sequence,
            self.report.operation_id,
            self.report.plan_id,
            previous_hash,
            event,
        )?;
        let record = encode_record(&entry)?;
        let generation = self
            .anchor_generation
            .checked_add(1)
            .ok_or(JournalError::JournalTooLarge)?;
        let anchor = HeadAnchor::new(generation, sequence + 1, entry.entry_hash.clone());
        self.storage.commit(&record, &anchor)?;

        self.anchor_generation = generation;
        self.report.checkpoints = checkpoints;
        self.report.state = replay_state;
        self.report.entries.push(entry);
        Ok(self.report.entries.last().expect("entry just pushed"))
    }
}

fn make_entry(
    sequence: u64,
    operation_id: OperationId,
    plan_id: PlanId,
    previous_hash: Option<String>,
    event: JournalEvent,
) -> Result<JournalEntry, JournalError> {
    let recorded_unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| JournalError::Clock)?
        .as_millis()
        .try_into()
        .map_err(|_| JournalError::Clock)?;
    let mut entry = JournalEntry {
        schema_version: SCHEMA_VERSION,
        sequence,
        operation_id,
        plan_id,
        recorded_unix_ms,
        previous_hash,
        event,
        entry_hash: String::new(),
    };
    entry.entry_hash = entry.calculate_hash()?;
    Ok(entry)
}

fn encode_record(entry: &JournalEntry) -> Result<Vec<u8>, JournalError> {
    let mut record = serde_json::to_vec(entry)?;
    if record.len() > MAX_ENTRY_BYTES {
        return Err(JournalError::EntryTooLarge);
    }
    record.push(b'\n');
    Ok(record)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn ids() -> (OperationId, PlanId) {
        (
            OperationId::from_uuid(Uuid::from_u128(1)),
            PlanId::from_uuid(Uuid::from_u128(2)),
        )
    }

    fn completed_memory_journal() -> MemoryStorage {
        let (operation, plan) = ids();
        let mut journal = Journal::create(MemoryStorage::default(), operation, plan).unwrap();
        let step = JournalCode::new("download.image").unwrap();
        journal
            .append_checkpoint(step.clone(), CheckpointState::Prepared)
            .unwrap();
        journal
            .append_checkpoint(step.clone(), CheckpointState::Applied)
            .unwrap();
        journal
            .append_checkpoint(step, CheckpointState::Verified)
            .unwrap();
        journal.finish(TerminalState::Succeeded).unwrap();
        journal.into_storage()
    }

    #[test]
    fn creates_replays_and_reopens_complete_chain() {
        let storage = completed_memory_journal();
        let journal = Journal::open(storage).unwrap();
        assert_eq!(journal.report().entries.len(), 5);
        assert_eq!(
            journal.report().state,
            ReplayState::Terminal(TerminalState::Succeeded)
        );
        assert_eq!(
            journal.report().checkpoints[&JournalCode::new("download.image").unwrap()],
            CheckpointState::Verified
        );
    }

    #[test]
    fn rejects_invalid_checkpoint_transition_without_writing() {
        let (operation, plan) = ids();
        let mut journal = Journal::create(MemoryStorage::default(), operation, plan).unwrap();
        let error = journal
            .append_checkpoint(
                JournalCode::new("write.usb").unwrap(),
                CheckpointState::Applied,
            )
            .unwrap_err();
        assert!(matches!(error, JournalError::InvalidCheckpoint(_)));
        assert_eq!(journal.report().entries.len(), 1);
    }

    #[test]
    fn terminal_state_is_final() {
        let (operation, plan) = ids();
        let mut journal = Journal::create(MemoryStorage::default(), operation, plan).unwrap();
        journal
            .finish(TerminalState::Cancelled {
                code: JournalCode::new("user_cancelled").unwrap(),
            })
            .unwrap();
        assert!(matches!(
            journal.finish(TerminalState::Succeeded),
            Err(JournalError::AlreadyTerminal)
        ));
    }

    #[test]
    fn detects_partial_tail() {
        let storage = completed_memory_journal();
        let (mut bytes, _) = storage.into_parts();
        bytes.pop();
        assert!(matches!(replay(&bytes), Err(JournalError::TruncatedEntry)));
    }

    #[test]
    fn detects_content_tampering() {
        let storage = completed_memory_journal();
        let (mut bytes, _) = storage.into_parts();
        let position = bytes
            .windows("download.image".len())
            .position(|window| window == b"download.image")
            .unwrap();
        bytes[position] = b'X';
        assert!(matches!(replay(&bytes), Err(JournalError::InvalidHash(_))));
    }

    #[test]
    fn anchor_detects_removal_of_complete_tail() {
        let storage = completed_memory_journal();
        let (mut bytes, anchor) = storage.into_parts();
        let last_start = bytes[..bytes.len() - 1]
            .iter()
            .rposition(|byte| *byte == b'\n')
            .unwrap()
            + 1;
        bytes.truncate(last_start);
        assert!(matches!(
            Journal::open(MemoryStorage::from_parts(bytes, anchor)),
            Err(JournalError::AnchorMismatch)
        ));
    }

    #[test]
    fn anchor_detects_uncommitted_complete_tail() {
        let storage = completed_memory_journal();
        let (mut bytes, anchor) = storage.into_parts();
        let existing = replay(&bytes).unwrap();
        let extra = make_entry(
            existing.entries.len() as u64,
            existing.operation_id,
            existing.plan_id,
            existing.entries.last().map(|item| item.entry_hash.clone()),
            JournalEvent::Terminal {
                state: TerminalState::Succeeded,
            },
        )
        .unwrap();
        bytes.extend(encode_record(&extra).unwrap());
        assert!(matches!(
            Journal::open(MemoryStorage::from_parts(bytes, anchor)),
            Err(JournalError::EventAfterTerminal) | Err(JournalError::AnchorMismatch)
        ));
    }

    #[test]
    fn rejects_changed_identity_even_if_entry_is_rehashed() {
        let storage = completed_memory_journal();
        let (bytes, _) = storage.into_parts();
        let mut entries: Vec<JournalEntry> = bytes
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .map(|line| serde_json::from_slice(line).unwrap())
            .collect();
        entries[1].operation_id = OperationId::new();
        entries[1].entry_hash = entries[1].calculate_hash().unwrap();
        let mut rewritten = Vec::new();
        for entry in entries {
            rewritten.extend(encode_record(&entry).unwrap());
        }
        assert!(matches!(
            replay(&rewritten),
            Err(JournalError::IdentityChanged(1))
        ));
    }

    #[test]
    fn file_storage_survives_reopen_and_uses_rotating_anchors() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("operation.journal");
        let (operation, plan) = ids();
        let mut journal = Journal::create(FileStorage::new(&path), operation, plan).unwrap();
        journal
            .append_checkpoint(
                JournalCode::new("inspect.target").unwrap(),
                CheckpointState::Prepared,
            )
            .unwrap();
        drop(journal);

        let reopened = Journal::open(FileStorage::new(&path)).unwrap();
        assert_eq!(reopened.report().entries.len(), 2);
        assert!(fs::metadata(suffixed_path(&path, ".head.0")).is_ok());
        assert!(fs::metadata(suffixed_path(&path, ".head.1")).is_ok());
    }

    #[test]
    fn corrupt_latest_anchor_falls_back_and_detects_unanchored_log_tail() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("operation.journal");
        let (operation, plan) = ids();
        let mut journal = Journal::create(FileStorage::new(&path), operation, plan).unwrap();
        journal
            .append_checkpoint(
                JournalCode::new("inspect.target").unwrap(),
                CheckpointState::Prepared,
            )
            .unwrap();
        drop(journal);

        // Generation 1 is in slot 1. Slot 0 still anchors generation 0.
        fs::write(suffixed_path(&path, ".head.1"), b"partial").unwrap();
        assert!(matches!(
            Journal::open(FileStorage::new(&path)),
            Err(JournalError::AnchorMismatch)
        ));
    }

    #[test]
    fn rejects_identifiers_that_could_contain_sensitive_free_text() {
        assert!(JournalCode::new("contains spaces and a path C:\\Users\\name").is_err());
        assert!(JournalCode::new("").is_err());
        assert!(JournalCode::new("safe.error-code:1").is_ok());
    }
}
