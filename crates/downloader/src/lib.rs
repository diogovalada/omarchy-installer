//! Verified artifact ingestion and content-addressed caching.
//!
//! This crate deliberately does not perform network requests. A caller may use
//! any transport it wants and feed received bytes into [`DownloadSession`]. No
//! artifact becomes visible in the verified cache until its exact length and
//! SHA-256 digest have both been checked.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const STATE_VERSION: u32 = 1;
const COPY_BUFFER_SIZE: usize = 64 * 1024;
static TOKEN_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("invalid SHA-256 digest: expected exactly 64 hexadecimal characters")]
    InvalidDigest,

    #[error(
        "artifact exceeds its declared length of {expected} bytes (attempted {attempted} bytes)"
    )]
    Oversized { expected: u64, attempted: u64 },

    #[error("artifact is truncated: expected {expected} bytes, received {actual}")]
    Truncated { expected: u64, actual: u64 },

    #[error("artifact SHA-256 mismatch: expected {expected}, computed {actual}")]
    DigestMismatch {
        expected: Sha256Digest,
        actual: Sha256Digest,
    },

    #[error("resume state is invalid: {0}")]
    InvalidResumeState(String),

    #[error("verified-cache entry is invalid: {0}")]
    InvalidCacheEntry(String),

    #[error("resume state could not be encoded: {0}")]
    StateEncoding(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

/// A parsed, fixed-size SHA-256 digest.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Sha256Digest(#[serde(with = "digest_hex")] [u8; 32]);

impl Sha256Digest {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Display for Sha256Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl FromStr for Sha256Digest {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self> {
        if value.len() != 64 || !value.as_bytes().iter().all(u8::is_ascii_hexdigit) {
            return Err(Error::InvalidDigest);
        }

        let mut bytes = [0_u8; 32];
        for (index, chunk) in value.as_bytes().chunks_exact(2).enumerate() {
            let text = std::str::from_utf8(chunk).map_err(|_| Error::InvalidDigest)?;
            bytes[index] = u8::from_str_radix(text, 16).map_err(|_| Error::InvalidDigest)?;
        }
        Ok(Self(bytes))
    }
}

/// Immutable requirements for an artifact.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ArtifactSpec {
    pub expected_len: u64,
    pub expected_sha256: Sha256Digest,
}

impl ArtifactSpec {
    pub fn new(expected_len: u64, expected_sha256: Sha256Digest) -> Self {
        Self {
            expected_len,
            expected_sha256,
        }
    }

    pub fn from_hex(expected_len: u64, expected_sha256: &str) -> Result<Self> {
        Ok(Self::new(expected_len, expected_sha256.parse()?))
    }
}

/// An opaque identifier that lets a caller reopen an interrupted session.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ResumeToken(String);

impl ResumeToken {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for ResumeToken {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self> {
        let token = Self(value.to_owned());
        validate_token(&token)?;
        Ok(token)
    }
}

/// Persisted information about an interrupted transfer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResumeState {
    pub token: ResumeToken,
    pub spec: ArtifactSpec,
    pub received_len: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct StoredState {
    version: u32,
    token: String,
    spec: ArtifactSpec,
}

/// A content-addressed cache that only exposes fully verified artifacts.
#[derive(Clone, Debug)]
pub struct VerifiedCache {
    root: PathBuf,
}

impl VerifiedCache {
    pub fn new(root: impl Into<PathBuf>) -> Result<Self> {
        let cache = Self { root: root.into() };
        fs::create_dir_all(cache.partials_dir())?;
        fs::create_dir_all(cache.artifacts_dir())?;
        Ok(cache)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Start a transfer with a unique, exclusive partial destination.
    pub fn begin(&self, spec: ArtifactSpec) -> Result<DownloadSession> {
        for _ in 0..128 {
            let token = ResumeToken(unique_token());
            let partial_path = self.partial_path(&token);
            let file = match OpenOptions::new()
                .create_new(true)
                .read(true)
                .write(true)
                .open(&partial_path)
            {
                Ok(file) => file,
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.into()),
            };

            let stored = StoredState {
                version: STATE_VERSION,
                token: token.0.clone(),
                spec: spec.clone(),
            };
            if let Err(error) = write_new_state(&self.state_path(&token), &stored) {
                drop(file);
                let _ = fs::remove_file(&partial_path);
                if matches!(&error, Error::Io(io_error) if io_error.kind() == io::ErrorKind::AlreadyExists)
                {
                    continue;
                }
                return Err(error);
            }

            return Ok(DownloadSession {
                cache: self.clone(),
                token,
                spec,
                partial_path,
                file,
                hasher: Sha256::new(),
                received_len: 0,
            });
        }

        Err(Error::InvalidResumeState(
            "could not allocate a unique partial destination".into(),
        ))
    }

    /// Reopen a transfer. The actual partial file is authoritative; its length
    /// and hash state are reconstructed instead of trusting a progress counter.
    pub fn resume(&self, token: &ResumeToken) -> Result<DownloadSession> {
        self.resume_with_progress(token, |_| Ok(()))
    }

    /// Reopen a transfer while observing its rehash. The callback runs before
    /// reading and after each bounded block. Returning an I/O error cancels the
    /// operation without modifying the partial or persisted state.
    pub fn resume_with_progress(
        &self,
        token: &ResumeToken,
        progress: impl FnMut(u64) -> io::Result<()>,
    ) -> Result<DownloadSession> {
        validate_token(token)?;
        let state_path = self.state_path(token);
        let stored: StoredState = serde_json::from_reader(BufReader::new(File::open(&state_path)?))
            .map_err(|error| Error::InvalidResumeState(error.to_string()))?;
        if stored.version != STATE_VERSION || stored.token != token.0 {
            return Err(Error::InvalidResumeState(
                "state version or token does not match".into(),
            ));
        }

        let partial_path = self.partial_path(token);
        let partial_len = fs::metadata(&partial_path)?.len();
        if partial_len > stored.spec.expected_len {
            return Err(Error::Oversized {
                expected: stored.spec.expected_len,
                attempted: partial_len,
            });
        }
        let mut reader = BufReader::new(File::open(&partial_path)?);
        let mut hasher = Sha256::new();
        let received_len = hash_with_progress(&mut reader, &mut hasher, progress)?;
        if received_len > stored.spec.expected_len {
            return Err(Error::Oversized {
                expected: stored.spec.expected_len,
                attempted: received_len,
            });
        }

        let file = OpenOptions::new()
            .read(true)
            .append(true)
            .open(&partial_path)?;
        Ok(DownloadSession {
            cache: self.clone(),
            token: token.clone(),
            spec: stored.spec,
            partial_path,
            file,
            hasher,
            received_len,
        })
    }

    /// Return a cached path only after re-verifying its length and digest.
    pub fn lookup(&self, spec: &ArtifactSpec) -> Result<Option<PathBuf>> {
        self.lookup_with_progress(spec, |_| Ok(()))
    }

    /// Look up an artifact with a fallible callback during its bounded-block
    /// rehash. A callback error leaves the existing artifact unchanged.
    pub fn lookup_with_progress(
        &self,
        spec: &ArtifactSpec,
        progress: impl FnMut(u64) -> io::Result<()>,
    ) -> Result<Option<PathBuf>> {
        let path = self.artifact_path(spec.expected_sha256);
        if !path.try_exists()? {
            return Ok(None);
        }
        verify_path_with_progress(&path, spec, progress)
            .map_err(|error| Error::InvalidCacheEntry(error.to_string()))?;
        Ok(Some(path))
    }

    fn partials_dir(&self) -> PathBuf {
        self.root.join("partials")
    }

    fn artifacts_dir(&self) -> PathBuf {
        self.root.join("artifacts")
    }

    fn partial_path(&self, token: &ResumeToken) -> PathBuf {
        self.partials_dir().join(format!("{}.part", token.0))
    }

    fn state_path(&self, token: &ResumeToken) -> PathBuf {
        self.partials_dir().join(format!("{}.json", token.0))
    }

    fn artifact_path(&self, digest: Sha256Digest) -> PathBuf {
        self.artifacts_dir().join(digest.to_string())
    }
}

/// A partially received artifact. Dropping it preserves its resumable state.
pub struct DownloadSession {
    cache: VerifiedCache,
    token: ResumeToken,
    spec: ArtifactSpec,
    partial_path: PathBuf,
    file: File,
    hasher: Sha256,
    received_len: u64,
}

impl DownloadSession {
    pub fn token(&self) -> &ResumeToken {
        &self.token
    }

    pub fn partial_path(&self) -> &Path {
        &self.partial_path
    }

    pub fn state(&self) -> ResumeState {
        ResumeState {
            token: self.token.clone(),
            spec: self.spec.clone(),
            received_len: self.received_len,
        }
    }

    /// Append transport bytes, refusing a write that would exceed the declared
    /// size. Rejected bytes are never partially written.
    pub fn append(&mut self, bytes: &[u8]) -> Result<()> {
        let bytes_len = u64::try_from(bytes.len()).map_err(|_| Error::Oversized {
            expected: self.spec.expected_len,
            attempted: u64::MAX,
        })?;
        let attempted = self
            .received_len
            .checked_add(bytes_len)
            .ok_or(Error::Oversized {
                expected: self.spec.expected_len,
                attempted: u64::MAX,
            })?;
        if attempted > self.spec.expected_len {
            return Err(Error::Oversized {
                expected: self.spec.expected_len,
                attempted,
            });
        }

        self.file.write_all(bytes)?;
        self.hasher.update(bytes);
        self.received_len = attempted;
        Ok(())
    }

    /// Consume an arbitrary reader, suitable for a network client's response
    /// body while leaving transport policy outside this crate.
    pub fn copy_from(&mut self, mut reader: impl Read) -> Result<u64> {
        let starting_len = self.received_len;
        let mut buffer = vec![0_u8; COPY_BUFFER_SIZE];
        loop {
            let read = reader.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            self.append(&buffer[..read])?;
        }
        Ok(self.received_len - starting_len)
    }

    /// Ask the operating system to durably checkpoint received bytes.
    pub fn checkpoint(&mut self) -> Result<()> {
        self.file.sync_data()?;
        Ok(())
    }

    /// Check the accumulated exact length and digest without promoting the
    /// artifact. This lets callers authenticate signatures before promotion
    /// without rereading bytes solely to check their checksum.
    pub fn verify(&self) -> Result<()> {
        if self.received_len != self.spec.expected_len {
            return Err(Error::Truncated {
                expected: self.spec.expected_len,
                actual: self.received_len,
            });
        }

        let actual = Sha256Digest::from_bytes(self.hasher.clone().finalize().into());
        if actual != self.spec.expected_sha256 {
            return Err(Error::DigestMismatch {
                expected: self.spec.expected_sha256,
                actual,
            });
        }
        Ok(())
    }

    /// Verify and atomically make the artifact visible in the cache.
    pub fn finalize(self) -> Result<VerifiedArtifact> {
        self.verify()?;
        let actual = self.spec.expected_sha256;
        self.file.sync_all()?;
        // Windows does not allow deleting the partial while our handle is open.
        // No further writes are permitted once verification has completed.
        drop(self.file);
        let destination = self.cache.artifact_path(actual);
        let mut reused_existing = false;

        // hard_link is an atomic create-if-absent operation on the same volume.
        // It cannot replace an existing cache entry, unlike rename on Unix.
        match fs::hard_link(&self.partial_path, &destination) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                verify_path(&destination, &self.spec)
                    .map_err(|error| Error::InvalidCacheEntry(error.to_string()))?;
                reused_existing = true;
            }
            Err(error) => return Err(error.into()),
        }

        // Once the link exists, cleanup may safely be retried after a crash.
        fs::remove_file(&self.partial_path)?;
        let state_path = self.cache.state_path(&self.token);
        match fs::remove_file(state_path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }

        Ok(VerifiedArtifact {
            path: destination,
            length: self.spec.expected_len,
            sha256: actual,
            reused_existing,
        })
    }

    /// Explicitly discard an interrupted transfer and its resume state.
    pub fn discard(self) -> Result<()> {
        drop(self.file);
        match fs::remove_file(&self.partial_path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        match fs::remove_file(self.cache.state_path(&self.token)) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedArtifact {
    pub path: PathBuf,
    pub length: u64,
    pub sha256: Sha256Digest,
    pub reused_existing: bool,
}

fn verify_path(path: &Path, spec: &ArtifactSpec) -> Result<()> {
    verify_path_with_progress(path, spec, |_| Ok(()))
}

fn verify_path_with_progress(
    path: &Path,
    spec: &ArtifactSpec,
    progress: impl FnMut(u64) -> io::Result<()>,
) -> Result<()> {
    let metadata = fs::metadata(path)?;
    if metadata.len() != spec.expected_len {
        return Err(Error::Truncated {
            expected: spec.expected_len,
            actual: metadata.len(),
        });
    }
    let mut reader = BufReader::new(File::open(path)?);
    let mut hasher = Sha256::new();
    hash_with_progress(&mut reader, &mut hasher, progress)?;
    let actual = Sha256Digest::from_bytes(hasher.finalize().into());
    if actual != spec.expected_sha256 {
        return Err(Error::DigestMismatch {
            expected: spec.expected_sha256,
            actual,
        });
    }
    Ok(())
}

fn hash_with_progress(
    reader: &mut impl Read,
    hasher: &mut Sha256,
    mut progress: impl FnMut(u64) -> io::Result<()>,
) -> io::Result<u64> {
    let mut buffer = vec![0_u8; COPY_BUFFER_SIZE];
    let mut received = 0_u64;
    progress(received)?;
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        received += read as u64;
        progress(received)?;
    }
    Ok(received)
}

fn write_new_state(path: &Path, state: &StoredState) -> Result<()> {
    let mut file = OpenOptions::new().create_new(true).write(true).open(path)?;
    serde_json::to_writer(&mut file, state)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}

fn validate_token(token: &ResumeToken) -> Result<()> {
    if token.0.is_empty()
        || !token
            .0
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err(Error::InvalidResumeState("unsafe resume token".into()));
    }
    Ok(())
}

fn unique_token() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let sequence = TOKEN_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("{:x}-{:x}-{:x}", std::process::id(), timestamp, sequence)
}

mod digest_hex {
    use serde::{de::Error as _, Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(bytes: &[u8; 32], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut encoded = String::with_capacity(64);
        for byte in bytes {
            use std::fmt::Write as _;
            write!(&mut encoded, "{byte:02x}").expect("writing to a String cannot fail");
        }
        serializer.serialize_str(&encoded)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<[u8; 32], D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value
            .parse::<super::Sha256Digest>()
            .map(|digest| digest.0)
            .map_err(D::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::io::Cursor;
    use tempfile::TempDir;

    fn spec_for(bytes: &[u8]) -> ArtifactSpec {
        ArtifactSpec::new(
            bytes.len() as u64,
            Sha256Digest::from_bytes(Sha256::digest(bytes).into()),
        )
    }

    fn cache() -> (TempDir, VerifiedCache) {
        let directory = TempDir::new().unwrap();
        let cache = VerifiedCache::new(directory.path()).unwrap();
        (directory, cache)
    }

    #[test]
    fn valid_artifact_is_invisible_until_atomic_promotion() {
        let (_directory, cache) = cache();
        let bytes = b"verified omarchy image";
        let spec = spec_for(bytes);
        let mut session = cache.begin(spec.clone()).unwrap();
        let partial = session.partial_path().to_owned();
        let state = cache.state_path(session.token());

        session.copy_from(Cursor::new(bytes)).unwrap();
        assert_eq!(cache.lookup(&spec).unwrap(), None);
        assert!(partial.exists());
        assert!(state.exists());

        let artifact = session.finalize().unwrap();
        assert_eq!(fs::read(&artifact.path).unwrap(), bytes);
        assert!(!artifact.reused_existing);
        assert!(!partial.exists());
        assert!(!state.exists());
        assert_eq!(cache.lookup(&spec).unwrap(), Some(artifact.path));
    }

    #[test]
    fn corrupt_artifact_is_rejected_and_not_promoted() {
        let (_directory, cache) = cache();
        let expected = b"expected";
        let spec = spec_for(expected);
        let mut session = cache.begin(spec.clone()).unwrap();
        session.append(b"corrupt!").unwrap();

        assert!(matches!(
            session.finalize(),
            Err(Error::DigestMismatch { .. })
        ));
        assert_eq!(cache.lookup(&spec).unwrap(), None);
    }

    #[test]
    fn truncated_artifact_is_rejected_and_not_promoted() {
        let (_directory, cache) = cache();
        let complete = b"complete artifact";
        let spec = spec_for(complete);
        let mut session = cache.begin(spec.clone()).unwrap();
        session.append(b"complete").unwrap();

        assert!(matches!(
            session.finalize(),
            Err(Error::Truncated { expected, actual })
                if expected == complete.len() as u64 && actual == 8
        ));
        assert_eq!(cache.lookup(&spec).unwrap(), None);
    }

    #[test]
    fn oversized_append_is_rejected_without_partial_write() {
        let (_directory, cache) = cache();
        let spec = spec_for(b"tiny");
        let mut session = cache.begin(spec).unwrap();

        assert!(matches!(
            session.append(b"too large"),
            Err(Error::Oversized {
                expected: 4,
                attempted: 9
            })
        ));
        assert_eq!(session.state().received_len, 0);
        assert_eq!(fs::metadata(session.partial_path()).unwrap().len(), 0);
    }

    #[test]
    fn interrupted_transfer_resumes_from_durable_partial_state() {
        let (_directory, cache) = cache();
        let bytes = b"first half-second half";
        let spec = spec_for(bytes);
        let token = {
            let mut session = cache.begin(spec.clone()).unwrap();
            session.append(b"first half-").unwrap();
            session.checkpoint().unwrap();
            session.token().clone()
        };

        let mut resumed = cache.resume(&token).unwrap();
        assert_eq!(resumed.state().received_len, 11);
        assert_eq!(resumed.state().spec, spec);
        resumed.append(b"second half").unwrap();
        let artifact = resumed.finalize().unwrap();
        assert_eq!(fs::read(artifact.path).unwrap(), bytes);
    }

    #[test]
    fn sessions_always_get_unique_partial_destinations() {
        let (_directory, cache) = cache();
        let spec = spec_for(b"same artifact");
        let first = cache.begin(spec.clone()).unwrap();
        let second = cache.begin(spec).unwrap();
        assert_ne!(first.token(), second.token());
        assert_ne!(first.partial_path(), second.partial_path());
    }

    #[test]
    fn existing_verified_entry_wins_without_being_replaced() {
        let (_directory, cache) = cache();
        let bytes = b"identical artifact";
        let spec = spec_for(bytes);

        let mut first = cache.begin(spec.clone()).unwrap();
        first.append(bytes).unwrap();
        let original = first.finalize().unwrap();

        let mut second = cache.begin(spec).unwrap();
        second.append(bytes).unwrap();
        let reused = second.finalize().unwrap();

        assert!(reused.reused_existing);
        assert_eq!(reused.path, original.path);
        assert_eq!(fs::read(reused.path).unwrap(), bytes);
    }

    #[test]
    fn invalid_digest_text_is_rejected() {
        assert!(matches!(
            "abcd".parse::<Sha256Digest>(),
            Err(Error::InvalidDigest)
        ));
        assert!(matches!(
            "gggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggg"
                .parse::<Sha256Digest>(),
            Err(Error::InvalidDigest)
        ));
    }

    #[test]
    fn resume_token_can_be_safely_reconstructed_after_restart() {
        let (_directory, cache) = cache();
        let session = cache.begin(spec_for(b"restartable")).unwrap();
        let encoded = session.token().as_str().to_owned();
        let reconstructed: ResumeToken = encoded.parse().unwrap();
        assert_eq!(&reconstructed, session.token());
        assert!("../outside".parse::<ResumeToken>().is_err());
    }

    #[test]
    fn preview_verification_does_not_promote() {
        let (_directory, cache) = cache();
        let bytes = b"verify then authenticate";
        let spec = spec_for(bytes);
        let mut session = cache.begin(spec.clone()).unwrap();
        assert!(matches!(session.verify(), Err(Error::Truncated { .. })));
        session.append(bytes).unwrap();
        session.verify().unwrap();
        assert!(session.partial_path().exists());
        assert_eq!(cache.lookup(&spec).unwrap(), None);
        session.finalize().unwrap();
    }

    #[test]
    fn interrupted_rehash_preserves_partial_and_cached_artifact() {
        let (_directory, cache) = cache();
        let bytes = vec![9_u8; COPY_BUFFER_SIZE * 4];
        let spec = spec_for(&bytes);
        let mut session = cache.begin(spec.clone()).unwrap();
        session.append(&bytes).unwrap();
        let token = session.token().clone();
        let path = session.partial_path().to_owned();
        drop(session);
        let cancelled = cache.resume_with_progress(&token, |received| {
            if received > 0 {
                Err(io::Error::other("cancelled"))
            } else {
                Ok(())
            }
        });
        assert!(matches!(cancelled, Err(Error::Io(_))));
        assert_eq!(fs::read(&path).unwrap(), bytes);
        let resumed = cache.resume(&token).unwrap();
        let artifact = resumed.finalize().unwrap();
        assert!(cache
            .lookup_with_progress(&spec, |received| {
                if received > 0 {
                    Err(io::Error::other("cancelled"))
                } else {
                    Ok(())
                }
            })
            .is_err());
        assert_eq!(fs::read(artifact.path).unwrap(), bytes);
        assert!(cache.lookup(&spec).unwrap().is_some());
    }
}
