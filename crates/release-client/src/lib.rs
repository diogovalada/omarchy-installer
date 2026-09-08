//! Read-only official release discovery and authenticated artifact downloads.
//!
//! Call the synchronous public API from a blocking worker, never an async runtime
//! thread. The release object is constructed only by official discovery. No
//! external key, URL, or serialized release can enter the production API.

use fs2::FileExt;
use omarchy_downloader::{ArtifactSpec, DownloadSession, ResumeToken, VerifiedCache};
use pgp::composed::{Deserializable, SignedPublicKey};
use pgp::crypto::hash::HashAlgorithm;
use pgp::packet::{Packet, PacketParser, Signature, SignatureType, SubpacketData};
use pgp::types::KeyDetails;
use reqwest::{Client, Response, StatusCode, Url, header};
use serde::Serialize;
use std::fs::{self, File, OpenOptions};
use std::future::Future;
use std::io::{self, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const SIGNER_FINGERPRINT: &str = "40DFB630FF42BCFFB047046CF0134EE680CAC571";
const PINNED_KEY: &str = include_str!("../keys/omarchy.asc");
const HOME_URL: &str = "https://omarchy.org/";
const ISO_ORIGIN: &str = "https://iso.omarchy.org/";
const MAX_HOME: usize = 1024 * 1024;
const MAX_SIGNATURE: usize = 16 * 1024;
const MAX_CHECKSUM: usize = 1024;
const MAX_IMAGE: u64 = 64 * 1024 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("download cancelled; received bytes are preserved for resume")]
    Cancelled,
    #[error("another download is using this release cache")]
    Busy,
    #[error("invalid official release metadata: {0}")]
    Metadata(String),
    #[error("upstream signature verification failed: {0}")]
    Signature(String),
    #[error("network request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("artifact verification failed: {0}")]
    Cache(#[from] omarchy_downloader::Error),
    #[error("file operation failed: {0}")]
    Io(#[from] io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Metadata discovered from the official homepage and its ISO sidecars.
/// SHA-256 is not an authentication result. Only a completed download is verified.
#[derive(Clone, Debug, Serialize)]
pub struct Release {
    version: String,
    file_name: String,
    url: String,
    length: u64,
    sha256: String,
    signature_url: String,
    signer_fingerprint: String,
    #[serde(skip)]
    signature: Vec<u8>,
}

impl Release {
    #[must_use]
    pub fn version(&self) -> &str {
        &self.version
    }
    #[must_use]
    pub fn file_name(&self) -> &str {
        &self.file_name
    }
    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }
    #[must_use]
    pub fn length(&self) -> u64 {
        self.length
    }
    #[must_use]
    pub fn sha256(&self) -> &str {
        &self.sha256
    }
    #[must_use]
    pub fn signature_url(&self) -> &str {
        &self.signature_url
    }
    #[must_use]
    pub fn signer_fingerprint(&self) -> &str {
        &self.signer_fingerprint
    }
    /// Detached signature bytes for a separate native consumer to authenticate
    /// the same ISO offline against this crate's embedded upstream key.
    #[must_use]
    pub fn signature(&self) -> &[u8] {
        &self.signature
    }
    fn spec(&self) -> Result<ArtifactSpec> {
        Ok(ArtifactSpec::from_hex(self.length, &self.sha256)?)
    }
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DownloadPhase {
    Preparing,
    Downloading,
    VerifyingSignature,
    Complete,
}

#[derive(Clone, Debug, Serialize)]
pub struct DownloadProgress {
    pub phase: DownloadPhase,
    pub received_bytes: u64,
    pub total_bytes: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct DownloadedImage {
    pub path: PathBuf,
    pub version: String,
    pub file_name: String,
    pub length: u64,
    pub sha256: String,
    pub signer_fingerprint: String,
    pub signature_verified: bool,
    pub cache_hit: bool,
}

/// Discover the current ISO. Reads at most 1 MiB of HTML, 1 KiB of checksum,
/// 16 KiB of signature, and response headers for the image itself.
///
/// # Errors
/// Returns an error for network failures or unsupported, ambiguous, oversized,
/// or malformed official metadata. This does not authenticate ISO bytes.
pub fn resolve_current() -> Result<Release> {
    runtime()?.block_on(resolve_with(
        &Transport::official()?,
        &AtomicBool::new(false),
    ))
}

/// Download or resume in `cache_dir`. Returns only after exact length, SHA-256,
/// and the detached signature against the embedded upstream key all verify.
/// Cancellation and transport errors preserve partial bytes across app restart.
///
/// # Errors
/// Returns [`Error::Cancelled`] on cancellation, [`Error::Busy`] when another
/// process holds this artifact's lock, or a network, file, metadata, checksum,
/// or signature error. Complete corrupt partials are discarded before retry.
pub fn download(
    release: &Release,
    cache_dir: impl AsRef<Path>,
    cancel: &AtomicBool,
    mut progress: impl FnMut(DownloadProgress),
) -> Result<DownloadedImage> {
    let transport = Transport::official()?;
    validate_release(release, &transport)?;
    runtime()?.block_on(download_with(
        &transport,
        release,
        cache_dir.as_ref(),
        cancel,
        &mut progress,
        PINNED_KEY,
    ))
}

/// Verify a caller-selected existing file against newly discovered official
/// metadata without changing that file or downloading the ISO again.
///
/// # Errors
/// Returns an error on cancellation, file access failure, or a mismatch in
/// length, checksum, signing key, or detached signature.
pub fn verify_existing(
    release: &Release,
    path: impl AsRef<Path>,
    cancel: &AtomicBool,
    mut progress: impl FnMut(DownloadProgress),
) -> Result<DownloadedImage> {
    validate_release(release, &Transport::official()?)?;
    let path = path.as_ref();
    emit(&mut progress, DownloadPhase::Preparing, 0, release.length);
    // Check the advertised digest while the signature verifier reads this file,
    // using the same open handle and one sequential pass through the ISO.
    verify_signature(path, release, PINNED_KEY, cancel, &mut progress)?;
    check_cancel(cancel)?;
    emit(
        &mut progress,
        DownloadPhase::Complete,
        release.length,
        release.length,
    );
    Ok(result(release, path.to_path_buf(), true))
}

/// Authenticate a local ISO at a privilege boundary without network access.
/// The metadata is a size/digest constraint, not authority: the detached
/// signature must match the embedded upstream signing key. This deliberately
/// does not construct a public Release or claim release freshness/version.
///
/// # Errors
/// Rejects malformed constraints, changed data, cancellation, or an invalid
/// detached signature. The caller must keep the authenticated file immutable
/// until its consumer finishes (for example in a protected operation directory).
pub fn authenticate_iso_offline(
    path: impl AsRef<Path>,
    length: u64,
    sha256: &str,
    signature: &[u8],
    cancel: &AtomicBool,
    mut progress: impl FnMut(DownloadProgress),
) -> Result<()> {
    if length == 0
        || length > MAX_IMAGE
        || sha256.len() != 64
        || !sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
        || signature.is_empty()
        || signature.len() > MAX_SIGNATURE
    {
        return Err(metadata("invalid offline authentication constraints"));
    }
    let constraints = Release {
        version: String::new(),
        file_name: String::new(),
        url: String::new(),
        length,
        sha256: sha256.to_ascii_lowercase(),
        signature_url: String::new(),
        signer_fingerprint: SIGNER_FINGERPRINT.to_owned(),
        signature: signature.to_vec(),
    };
    let path = path.as_ref();
    emit(&mut progress, DownloadPhase::Preparing, 0, length);
    verify_signature(path, &constraints, PINNED_KEY, cancel, &mut progress)?;
    check_cancel(cancel)?;
    emit(&mut progress, DownloadPhase::Complete, length, length);
    Ok(())
}

fn runtime() -> Result<tokio::runtime::Runtime> {
    Ok(tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?)
}

#[derive(Debug)]
struct Transport {
    client: Client,
    home: String,
    origin: String,
    #[cfg(test)]
    fixture: bool,
}

impl Transport {
    fn official() -> Result<Self> {
        Ok(Self {
            client: Client::builder()
                .https_only(true)
                .redirect(reqwest::redirect::Policy::none())
                .connect_timeout(Duration::from_secs(15))
                .read_timeout(Duration::from_secs(15))
                .no_gzip()
                .no_brotli()
                .no_deflate()
                .no_zstd()
                .user_agent("Omarchy-Installer/0.1 (verified release discovery)")
                .build()?,
            home: HOME_URL.into(),
            origin: ISO_ORIGIN.into(),
            #[cfg(test)]
            fixture: false,
        })
    }

    fn validate_url(&self, value: &str) -> Result<()> {
        let url = Url::parse(value).map_err(|_| metadata("invalid URL"))?;
        let expected = Url::parse(&self.origin).map_err(|_| metadata("invalid origin"))?;
        #[cfg(test)]
        let allowed_scheme = if self.fixture { "http" } else { "https" };
        #[cfg(not(test))]
        let allowed_scheme = "https";
        if url.scheme() != allowed_scheme
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || url.port() != expected.port()
            || (url.host_str() != expected.host_str() && value != self.home)
            || url.as_str() != value
        {
            return Err(metadata("URL is outside pinned official origins"));
        }
        Ok(())
    }

    async fn get(&self, url: &str, range: Option<u64>, cancel: &AtomicBool) -> Result<Response> {
        self.validate_url(url)?;
        let mut request = self
            .client
            .get(url)
            .header(header::ACCEPT_ENCODING, "identity");
        if let Some(offset) = range {
            request = request.header(header::RANGE, format!("bytes={offset}-"));
        }
        let response = cancellable(request.send(), cancel).await??;
        validate_encoding(response.headers())?;
        // Redirects are never followed, including redirects within an official host.
        if response.status().is_redirection() {
            return Err(metadata("redirects are forbidden"));
        }
        Ok(response)
    }
}

async fn resolve_with(transport: &Transport, cancel: &AtomicBool) -> Result<Release> {
    let html = bounded(transport, &transport.home, MAX_HOME, cancel).await?;
    let file_name = discover_file(
        std::str::from_utf8(&html).map_err(|_| metadata("homepage is not UTF-8"))?,
        &transport.origin,
    )?;
    let version = version_from_name(&file_name)?.to_owned();
    let url = format!("{}{file_name}", transport.origin);
    let sha_url = format!("{url}.sha256");
    let checksum = bounded(transport, &sha_url, MAX_CHECKSUM, cancel).await?;
    let sha256 = checksum_for(&checksum, &file_name)?;
    let signature_url = format!("{url}.sig");
    let signature = bounded(transport, &signature_url, MAX_SIGNATURE, cancel).await?;
    let _ = parse_signature(&signature)?;
    transport.validate_url(&url)?;
    let head = cancellable(
        transport
            .client
            .head(&url)
            .header(header::ACCEPT_ENCODING, "identity")
            .timeout(Duration::from_secs(30))
            .send(),
        cancel,
    )
    .await??;
    if head.status() != StatusCode::OK {
        return Err(metadata("ISO HEAD did not return 200"));
    }
    validate_encoding(head.headers())?;
    let length = required_length(head.headers())?;
    if length == 0 || length > MAX_IMAGE {
        return Err(metadata("ISO length is outside accepted bounds"));
    }
    Ok(Release {
        version,
        file_name,
        url,
        length,
        sha256,
        signature_url,
        signer_fingerprint: SIGNER_FINGERPRINT.into(),
        signature,
    })
}

async fn bounded(
    transport: &Transport,
    url: &str,
    limit: usize,
    cancel: &AtomicBool,
) -> Result<Vec<u8>> {
    // A total metadata timeout also bounds a malicious slow trickle.
    let operation = async {
        let mut response = transport.get(url, None, cancel).await?;
        if response.status() != StatusCode::OK {
            return Err(metadata("metadata GET did not return 200"));
        }
        if response.headers().contains_key(header::CONTENT_RANGE) {
            return Err(metadata("unexpected metadata Content-Range"));
        }
        let declared = optional_length(response.headers())?;
        if declared.is_some_and(|length| length > limit as u64) {
            return Err(metadata("metadata exceeds size limit"));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = cancellable(response.chunk(), cancel).await?? {
            if bytes.len().saturating_add(chunk.len()) > limit {
                return Err(metadata("metadata exceeds size limit"));
            }
            bytes.extend_from_slice(&chunk);
        }
        if declared.is_some_and(|length| length != bytes.len() as u64) {
            return Err(metadata("metadata length mismatch"));
        }
        Ok(bytes)
    };
    tokio::time::timeout(Duration::from_secs(30), operation)
        .await
        .map_err(|_| metadata("metadata request timed out"))?
}

// These are case-sensitive URL paths, not case-insensitive Windows extensions.
#[allow(clippy::case_sensitive_file_extension_comparisons)]
fn discover_file(html: &str, origin: &str) -> Result<String> {
    let mut candidate = None;
    for quote in ['"', '\''] {
        let prefix = format!("href={quote}{origin}");
        for tail in html.split(&prefix).skip(1) {
            let name = tail
                .split(quote)
                .next()
                .ok_or_else(|| metadata("unterminated ISO link"))?;
            if !name.ends_with(".iso") {
                continue;
            }
            version_from_name(name)?;
            if candidate.as_deref().is_some_and(|old| old != name) {
                return Err(metadata("ambiguous official ISO links"));
            }
            candidate = Some(name.to_owned());
        }
    }
    candidate.ok_or_else(|| metadata("official homepage has no supported ISO link"))
}

fn version_from_name(name: &str) -> Result<&str> {
    let version = name
        .strip_prefix("omarchy-")
        .and_then(|s| s.strip_suffix(".iso"))
        .ok_or_else(|| metadata("invalid ISO filename"))?;
    if version.len() > 32
        || version.split('.').count() != 3
        || version.split('.').any(|part| {
            part.is_empty()
                || part.len() > 8
                || !part.bytes().all(|b| b.is_ascii_digit())
                || (part.len() > 1 && part.starts_with('0'))
        })
    {
        return Err(metadata("unsupported or unsafe ISO version"));
    }
    Ok(version)
}

fn checksum_for(bytes: &[u8], file_name: &str) -> Result<String> {
    let text = std::str::from_utf8(bytes).map_err(|_| metadata("checksum is not UTF-8"))?;
    let line = text.strip_suffix('\n').unwrap_or(text);
    let line = line.strip_suffix('\r').unwrap_or(line);
    let (digest, name) = line
        .split_once("  ")
        .or_else(|| line.split_once(" *"))
        .ok_or_else(|| metadata("invalid checksum record"))?;
    if name != file_name {
        return Err(metadata("checksum filename mismatch"));
    }
    Ok(digest
        .parse::<omarchy_downloader::Sha256Digest>()?
        .to_string())
}

fn validate_release(release: &Release, transport: &Transport) -> Result<()> {
    if release.version != version_from_name(&release.file_name)?
        || release.url != format!("{}{}", transport.origin, release.file_name)
        || release.signature_url != format!("{}.sig", release.url)
        || release.signer_fingerprint != SIGNER_FINGERPRINT
        || release.length == 0
        || release.length > MAX_IMAGE
    {
        return Err(metadata("release metadata is inconsistent"));
    }
    transport.validate_url(&release.url)?;
    let _ = release.spec()?;
    let _ = parse_signature(&release.signature)?;
    Ok(())
}

fn validate_encoding(headers: &header::HeaderMap) -> Result<()> {
    // Even identity is rejected if explicitly encoded: expected upstream bodies
    // have no Content-Encoding at all. Decompression is disabled on the client.
    if headers.contains_key(header::CONTENT_ENCODING) {
        return Err(metadata("encoded responses are forbidden"));
    }
    if headers.contains_key(header::TRANSFER_ENCODING)
        && headers.contains_key(header::CONTENT_LENGTH)
    {
        return Err(metadata("ambiguous transfer framing"));
    }
    Ok(())
}

fn optional_length(headers: &header::HeaderMap) -> Result<Option<u64>> {
    let mut values = headers.get_all(header::CONTENT_LENGTH).iter();
    let Some(value) = values.next() else {
        return Ok(None);
    };
    if values.next().is_some() {
        return Err(metadata("duplicate Content-Length"));
    }
    let text = value
        .to_str()
        .map_err(|_| metadata("non-ASCII Content-Length"))?;
    Ok(Some(decimal(text)?))
}

fn required_length(headers: &header::HeaderMap) -> Result<u64> {
    optional_length(headers)?.ok_or_else(|| metadata("missing Content-Length"))
}

fn decimal(value: &str) -> Result<u64> {
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
        return Err(metadata("malformed length or range number"));
    }
    value
        .parse()
        .map_err(|_| metadata("length or range overflow"))
}

async fn cancellable<T>(future: impl Future<Output = T>, cancel: &AtomicBool) -> Result<T> {
    tokio::pin!(future);
    loop {
        check_cancel(cancel)?;
        tokio::select! {
            output = &mut future => { check_cancel(cancel)?; return Ok(output); }
            () = tokio::time::sleep(Duration::from_millis(100)) => {}
        }
    }
}

fn check_cancel(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err(Error::Cancelled)
    } else {
        Ok(())
    }
}

fn metadata(message: &str) -> Error {
    Error::Metadata(message.into())
}
// Owned error values arrive directly through Result::map_err.
#[allow(clippy::needless_pass_by_value)]
fn signature_error(message: impl ToString) -> Error {
    Error::Signature(message.to_string())
}
fn emit(
    callback: &mut impl FnMut(DownloadProgress),
    phase: DownloadPhase,
    received_bytes: u64,
    total_bytes: u64,
) {
    callback(DownloadProgress {
        phase,
        received_bytes,
        total_bytes,
    });
}

fn result(release: &Release, path: PathBuf, cache_hit: bool) -> DownloadedImage {
    DownloadedImage {
        path,
        version: release.version.clone(),
        file_name: release.file_name.clone(),
        length: release.length,
        sha256: release.sha256.clone(),
        signer_fingerprint: release.signer_fingerprint.clone(),
        signature_verified: true,
        cache_hit,
    }
}

async fn download_with(
    transport: &Transport,
    release: &Release,
    cache_dir: &Path,
    cancel: &AtomicBool,
    progress: &mut impl FnMut(DownloadProgress),
    key: &str,
) -> Result<DownloadedImage> {
    check_cancel(cancel)?;
    let cache = VerifiedCache::new(cache_dir)?;
    let lock_path = cache_dir.join(format!("release-{}.lock", release.sha256));
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(lock_path)?;
    match FileExt::try_lock_exclusive(&lock) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::WouldBlock => return Err(Error::Busy),
        Err(e) => return Err(e.into()),
    }
    // The OS lock releases on drop, including cancellation or a process crash.
    let spec = release.spec()?;
    emit(progress, DownloadPhase::Preparing, 0, release.length);
    let cached =
        match cache.lookup_with_progress(&spec, hash_progress(cancel, progress, release.length)) {
            Ok(path) => path,
            Err(omarchy_downloader::Error::InvalidCacheEntry(_)) => {
                // This directory is an internal disposable cache. Repair only the
                // exact digest-named artifact while its cross-process lock is held.
                check_cancel(cancel)?;
                fs::remove_file(cache.root().join("artifacts").join(&release.sha256))?;
                None
            }
            Err(error) => return Err(error.into()),
        };
    if let Some(path) = cached {
        verify_signature(&path, release, key, cancel, progress)?;
        check_cancel(cancel)?;
        emit(
            progress,
            DownloadPhase::Complete,
            release.length,
            release.length,
        );
        return Ok(result(release, path, true));
    }
    let pointer = cache_dir.join(format!("release-{}.resume", release.sha256));
    let mut session = resume_or_begin(&cache, &pointer, &spec, cancel, progress)?;
    check_cancel(cancel)?;
    let offset = session.state().received_len;
    emit(progress, DownloadPhase::Downloading, offset, release.length);
    if offset < release.length {
        let response = transport
            .get(&release.url, (offset > 0).then_some(offset), cancel)
            .await?;
        let restart = validate_image_response(&response, offset, release.length)?;
        if restart {
            session.discard()?;
            session = cache.begin(spec.clone())?;
            save_pointer(&pointer, session.token())?;
            emit(progress, DownloadPhase::Downloading, 0, release.length);
        }
        transfer(response, &mut session, release, cancel, progress).await?;
    }
    session.checkpoint()?;
    check_cancel(cancel)?;
    // Verify the maintained streaming hash before reading for authentication.
    // Bad complete bytes must be discarded so Retry starts a new transfer.
    if let Err(error) = session.verify() {
        if matches!(error, omarchy_downloader::Error::DigestMismatch { .. }) {
            session.discard()?;
            remove_pointer(&pointer)?;
        }
        return Err(error.into());
    }
    // Authenticate before promotion: a SHA-only result never enters this API's
    // success path. Signature failures preserve the partial for a future retry.
    // DownloadSession retains its write handle until promotion; allow that
    // existing handle while authenticating its checkpointed bytes.
    verify_signature_with_sharing(session.partial_path(), release, key, cancel, progress, true)?;
    check_cancel(cancel)?;
    let finalized = session.finalize()?;
    remove_pointer(&pointer)?;
    check_cancel(cancel)?;
    emit(
        progress,
        DownloadPhase::Complete,
        release.length,
        release.length,
    );
    Ok(result(release, finalized.path, finalized.reused_existing))
}

fn resume_or_begin(
    cache: &VerifiedCache,
    pointer: &Path,
    spec: &ArtifactSpec,
    cancel: &AtomicBool,
    progress: &mut impl FnMut(DownloadProgress),
) -> Result<DownloadSession> {
    if pointer.try_exists()? {
        let resume = (|| {
            let mut text = String::new();
            File::open(pointer)?.take(129).read_to_string(&mut text)?;
            if text.len() > 128 {
                return Err(metadata("oversized resume token"));
            }
            let token: ResumeToken = text.parse()?;
            let state_path = cache
                .root()
                .join("partials")
                .join(format!("{}.json", token.as_str()));
            if fs::metadata(&state_path)?.len() > 4096 {
                return Err(metadata("oversized resume state"));
            }
            let session = cache
                .resume_with_progress(&token, hash_progress(cancel, progress, spec.expected_len))?;
            if &session.state().spec != spec {
                return Err(metadata("resume artifact does not match release"));
            }
            Ok(session)
        })();
        // Do not interpret a cancelled hash as damaged state and delete its
        // pointer. A valid partial always remains resumable after cancellation.
        check_cancel(cancel)?;
        match resume {
            Ok(session) => return Ok(session),
            Err(Error::Io(error))
                if !matches!(
                    error.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::InvalidData
                ) =>
            {
                return Err(error.into());
            }
            Err(Error::Cache(omarchy_downloader::Error::Io(error)))
                if error.kind() != io::ErrorKind::NotFound =>
            {
                return Err(error.into());
            }
            Err(_) => {}
        }
        // Torn, stale or malformed private state is recoverable. Remove only
        // the digest-scoped pointer, never files derived from untrusted tokens.
        // The orphan partial/state remain available for inspection or cleanup.
        remove_pointer(pointer)?;
    }
    let session = cache.begin(spec.clone())?;
    save_pointer(pointer, session.token())?;
    Ok(session)
}

fn hash_progress<'a>(
    cancel: &'a AtomicBool,
    progress: &'a mut impl FnMut(DownloadProgress),
    total: u64,
) -> impl FnMut(u64) -> io::Result<()> + 'a {
    let mut last_progress = std::time::Instant::now();
    let mut first_block = true;
    move |received| {
        if cancel.load(Ordering::Relaxed) {
            return Err(io::Error::other("cancelled"));
        }
        if received == 0
            || first_block
            || received == total
            || last_progress.elapsed() >= Duration::from_millis(200)
        {
            emit(progress, DownloadPhase::Preparing, received, total);
            last_progress = std::time::Instant::now();
            if received > 0 {
                first_block = false;
            }
        }
        // A progress callback may itself request cancellation.
        if cancel.load(Ordering::Relaxed) {
            return Err(io::Error::other("cancelled"));
        }
        Ok(())
    }
}

fn save_pointer(path: &Path, token: &ResumeToken) -> Result<()> {
    // Replaced only while the per-artifact OS lock is held. A torn pointer fails
    // closed; the independent unique partial is never reinterpreted as trusted.
    let mut file = File::create(path)?;
    file.write_all(token.as_str().as_bytes())?;
    file.sync_all()?;
    Ok(())
}

fn remove_pointer(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

fn validate_image_response(response: &Response, offset: u64, total: u64) -> Result<bool> {
    match response.status() {
        StatusCode::OK => {
            if response.headers().contains_key(header::CONTENT_RANGE) {
                return Err(metadata("200 response has Content-Range"));
            }
            if required_length(response.headers())? != total {
                return Err(metadata("ISO response length changed"));
            }
            // The server ignored Range. Restart from this validated full body.
            Ok(offset > 0)
        }
        StatusCode::PARTIAL_CONTENT if offset > 0 => {
            let headers = response.headers();
            let mut ranges = headers.get_all(header::CONTENT_RANGE).iter();
            let value = ranges
                .next()
                .ok_or_else(|| metadata("missing Content-Range"))?;
            if ranges.next().is_some() {
                return Err(metadata("duplicate Content-Range"));
            }
            let value = value
                .to_str()
                .map_err(|_| metadata("non-ASCII Content-Range"))?;
            validate_range(value, offset, total)?;
            if required_length(headers)? != total - offset {
                return Err(metadata("range Content-Length mismatch"));
            }
            Ok(false)
        }
        _ => Err(metadata("ISO request returned an unexpected status")),
    }
}

fn validate_range(value: &str, offset: u64, total: u64) -> Result<()> {
    let value = value
        .strip_prefix("bytes ")
        .ok_or_else(|| metadata("invalid Content-Range unit"))?;
    let (span, size) = value
        .split_once('/')
        .ok_or_else(|| metadata("invalid Content-Range"))?;
    let (first, last) = span
        .split_once('-')
        .ok_or_else(|| metadata("invalid Content-Range span"))?;
    if decimal(first)? != offset || decimal(last)? != total - 1 || decimal(size)? != total {
        return Err(metadata(
            "Content-Range does not match requested offset and image length",
        ));
    }
    Ok(())
}

async fn transfer(
    mut response: Response,
    session: &mut DownloadSession,
    release: &Release,
    cancel: &AtomicBool,
    progress: &mut impl FnMut(DownloadProgress),
) -> Result<()> {
    let mut last_progress = std::time::Instant::now();
    let mut first_chunk = true;
    let mut checkpoint_at = session.state().received_len;
    loop {
        let chunk = match cancellable(response.chunk(), cancel).await {
            Ok(Ok(chunk)) => chunk,
            Ok(Err(error)) => {
                session.checkpoint()?;
                return Err(error.into());
            }
            Err(error) => {
                session.checkpoint()?;
                return Err(error);
            }
        };
        let Some(chunk) = chunk else {
            break;
        };
        session.append(&chunk)?;
        let received = session.state().received_len;
        if first_chunk
            || last_progress.elapsed() >= Duration::from_millis(200)
            || received == release.length
        {
            emit(
                progress,
                DownloadPhase::Downloading,
                received,
                release.length,
            );
            last_progress = std::time::Instant::now();
            first_chunk = false;
        }
        if received - checkpoint_at >= 16 * 1024 * 1024 {
            session.checkpoint()?;
            checkpoint_at = received;
        }
    }
    session.checkpoint()?;
    if session.state().received_len != release.length {
        return Err(omarchy_downloader::Error::Truncated {
            expected: release.length,
            actual: session.state().received_len,
        }
        .into());
    }
    Ok(())
}

fn parse_signature(bytes: &[u8]) -> Result<Signature> {
    if bytes.is_empty() || bytes.len() > MAX_SIGNATURE {
        return Err(signature_error("signature size is invalid"));
    }
    // PacketParser is used directly: composition helpers may skip packets they
    // do not understand. This protocol permits exactly one binary signature.
    let mut packets = PacketParser::new(bytes);
    let Some(Ok(Packet::Signature(signature))) = packets.next() else {
        return Err(signature_error(
            "expected one binary OpenPGP signature packet",
        ));
    };
    if packets.next().is_some() {
        return Err(signature_error("trailing or multiple signature packets"));
    }
    if signature.typ() != Some(SignatureType::Binary)
        || !matches!(
            signature.hash_alg(),
            Some(HashAlgorithm::Sha256 | HashAlgorithm::Sha384 | HashAlgorithm::Sha512)
        )
    {
        return Err(signature_error(
            "only binary signatures with SHA-256/384/512 are accepted",
        ));
    }
    let config = signature
        .config()
        .ok_or_else(|| signature_error("unsupported signature version"))?;
    for packet in config
        .hashed_subpackets
        .iter()
        .chain(&config.unhashed_subpackets)
    {
        if packet.is_critical
            && !matches!(
                packet.data,
                SubpacketData::SignatureCreationTime(_)
                    | SubpacketData::SignatureExpirationTime(_)
                    | SubpacketData::IssuerKeyId(_)
                    | SubpacketData::IssuerFingerprint(_)
            )
        {
            return Err(signature_error("unsupported critical signature subpacket"));
        }
    }
    let created = signature
        .created()
        .ok_or_else(|| signature_error("signature lacks authenticated creation time"))?
        .as_secs();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(signature_error)?
        .as_secs();
    if u64::from(created) > now + 300 {
        return Err(signature_error("signature is from the future"));
    }
    if let Some(expires) = signature.signature_expiration_time()
        && expires.as_secs() > 0
        && u64::from(created) + u64::from(expires.as_secs()) <= now
    {
        return Err(signature_error("signature has expired"));
    }
    Ok(signature)
}

fn verify_signature(
    path: &Path,
    release: &Release,
    armor: &str,
    cancel: &AtomicBool,
    progress: &mut impl FnMut(DownloadProgress),
) -> Result<()> {
    verify_signature_with_sharing(path, release, armor, cancel, progress, false)
}

fn verify_signature_with_sharing(
    path: &Path,
    release: &Release,
    armor: &str,
    cancel: &AtomicBool,
    progress: &mut impl FnMut(DownloadProgress),
    download_session_open: bool,
) -> Result<()> {
    use sha2::{Digest, Sha256};
    #[cfg(not(windows))]
    let _ = download_session_open;
    check_cancel(cancel)?;
    let (key, _) = SignedPublicKey::from_string(armor).map_err(signature_error)?;
    if key.fingerprint().to_string().to_uppercase() != release.signer_fingerprint {
        return Err(signature_error("embedded public key fingerprint mismatch"));
    }
    key.verify_bindings().map_err(signature_error)?;
    let signature = parse_signature(&release.signature)?;
    // Trust is directly pinned to the signing primary key. Encryption subkeys,
    // imported user keys, and network-fetched replacement keys cannot verify.
    if signature
        .issuer_fingerprint()
        .iter()
        .any(|fp| fp.to_string().to_uppercase() != release.signer_fingerprint)
    {
        return Err(signature_error("signature declares a different signer"));
    }
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(signature_error("ISO must be a regular file"));
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // Prevent writes/replacement while both integrity checks consume it.
        options.share_mode(if download_session_open { 3 } else { 1 }); // READ, plus WRITE for our download session
    }
    let file = options.open(path)?;
    if file.metadata()?.len() != release.length {
        return Err(signature_error("ISO length differs from release"));
    }
    emit(
        progress,
        DownloadPhase::VerifyingSignature,
        0,
        release.length,
    );
    let mut reader = CancelReader {
        inner: BufReader::with_capacity(1024 * 1024, file),
        sha256: Sha256::new(),
        cancel,
        read: 0,
        total: release.length,
        progress,
        last_progress: std::time::Instant::now(),
    };
    let checked = signature
        .verify(&key.primary_key, &mut reader)
        .map_err(signature_error);
    check_cancel(cancel)?;
    checked?;
    if reader.read != release.length {
        return Err(signature_error(
            "signature verifier did not consume exact file length",
        ));
    }
    let actual = omarchy_downloader::Sha256Digest::from_bytes(reader.sha256.finalize().into());
    let expected = release.spec()?.expected_sha256;
    if actual != expected {
        return Err(omarchy_downloader::Error::DigestMismatch { expected, actual }.into());
    }
    Ok(())
}

struct CancelReader<'a, F: FnMut(DownloadProgress)> {
    inner: BufReader<File>,
    sha256: sha2::Sha256,
    cancel: &'a AtomicBool,
    read: u64,
    total: u64,
    progress: &'a mut F,
    last_progress: std::time::Instant,
}

impl<F: FnMut(DownloadProgress)> Read for CancelReader<'_, F> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        use sha2::Digest;
        if self.cancel.load(Ordering::Relaxed) {
            return Err(io::Error::other("cancelled"));
        }
        let read = self.inner.read(bytes)?;
        self.read += read as u64;
        if self.read > self.total {
            return Err(io::Error::other("ISO grew during verification"));
        }
        self.sha256.update(&bytes[..read]);
        if self.last_progress.elapsed() >= Duration::from_millis(200) || read == 0 {
            emit(
                self.progress,
                DownloadPhase::VerifyingSignature,
                self.read,
                self.total,
            );
            self.last_progress = std::time::Instant::now();
        }
        Ok(read)
    }
}

#[cfg(test)]
mod tests;
