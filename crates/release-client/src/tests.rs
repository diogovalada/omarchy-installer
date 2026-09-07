#![allow(clippy::unwrap_used, clippy::expect_used)]
// Fixture routes use the same case-sensitive URL grammar as production.
#![allow(clippy::case_sensitive_file_extension_comparisons)]
use super::*;
use pgp::composed::{ArmorOptions, DetachedSignature, KeyType, SecretKeyParamsBuilder};
use pgp::ser::Serialize as PgpSerialize;
use pgp::types::Password;
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::io::BufRead;
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex, atomic::AtomicU8};
use std::thread::{self, JoinHandle};
use tempfile::TempDir;

// Only test code can supply a local HTTP origin or test signing key. Keys are
// generated in memory; no test key is reachable through the production pin.
struct Fixture {
    transport: Transport,
    release: Release,
    armor: String,
    data: Vec<u8>,
    mode: Arc<AtomicU8>,
    requests: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

#[test]
fn single_pass_verification_enforces_signature_digest_length_and_cancellation() {
    let fixture = Fixture::new();
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("existing.iso");
    fs::write(&path, &fixture.data).unwrap();
    let cancel = AtomicBool::new(false);
    verify_signature(
        &path,
        &fixture.release,
        &fixture.armor,
        &cancel,
        &mut |_| {},
    )
    .unwrap();
    let mut incorrect = fixture.release.clone();
    incorrect.sha256 = "00".repeat(32);
    assert!(matches!(
        verify_signature(&path, &incorrect, &fixture.armor, &cancel, &mut |_| {}),
        Err(Error::Cache(
            omarchy_downloader::Error::DigestMismatch { .. }
        ))
    ));
    incorrect = fixture.release.clone();
    incorrect.length += 1;
    assert!(verify_signature(&path, &incorrect, &fixture.armor, &cancel, &mut |_| {}).is_err());
    let mut corrupted = fixture.data.clone();
    corrupted[123] ^= 1;
    fs::write(&path, corrupted).unwrap();
    assert!(
        verify_signature(
            &path,
            &fixture.release,
            &fixture.armor,
            &cancel,
            &mut |_| {}
        )
        .is_err()
    );
    cancel.store(true, Ordering::Relaxed);
    assert!(matches!(
        verify_signature(
            &path,
            &fixture.release,
            &fixture.armor,
            &cancel,
            &mut |_| {}
        ),
        Err(Error::Cancelled)
    ));
}

impl Fixture {
    fn new() -> Self {
        let data: Vec<u8> = (0..251_u8).cycle().take(1024 * 1024).collect();
        let secret = SecretKeyParamsBuilder::default()
            .key_type(KeyType::Ed25519Legacy)
            .can_certify(true)
            .can_sign(true)
            .primary_user_id("TEST ONLY <fixture@invalid.example>".into())
            .build()
            .unwrap()
            .generate(rand::thread_rng())
            .unwrap();
        let public = SignedPublicKey::from(secret.clone());
        let armor = public.to_armored_string(ArmorOptions::default()).unwrap();
        let signature = DetachedSignature::sign_binary_data(
            rand::thread_rng(),
            &secret.primary_key,
            &Password::empty(),
            HashAlgorithm::Sha256,
            data.as_slice(),
        )
        .unwrap()
        .to_bytes()
        .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}/", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let file_name = "omarchy-1.2.3.iso".to_owned();
        let release = Release {
            version: "1.2.3".into(),
            url: format!("{origin}{file_name}"),
            signature_url: format!("{origin}{file_name}.sig"),
            length: data.len() as u64,
            sha256: format!("{:x}", Sha256::digest(&data)),
            file_name,
            signer_fingerprint: public.fingerprint().to_string().to_uppercase(),
            signature,
        };
        let mode = Arc::new(AtomicU8::new(0));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (server_release, server_data, server_mode, server_requests, server_stop) = (
            release.clone(),
            data.clone(),
            mode.clone(),
            requests.clone(),
            stop.clone(),
        );
        let worker = thread::spawn(move || {
            while !server_stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let _ = serve(
                            stream,
                            &server_release,
                            &server_data,
                            server_mode.load(Ordering::Relaxed),
                            &server_requests,
                        );
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                    }
                    Err(_) => break,
                }
            }
        });
        let transport = Transport {
            client: Client::builder()
                .no_proxy()
                .pool_max_idle_per_host(0)
                .redirect(reqwest::redirect::Policy::none())
                .read_timeout(Duration::from_secs(2))
                .build()
                .unwrap(),
            home: origin.clone(),
            origin,
            fixture: true,
        };
        Self {
            transport,
            release,
            armor,
            data,
            mode,
            requests,
            stop,
            worker: Some(worker),
        }
    }

    fn download(
        &self,
        dir: &Path,
        cancel: &AtomicBool,
        mut callback: impl FnMut(DownloadProgress),
    ) -> Result<DownloadedImage> {
        runtime()?.block_on(download_with(
            &self.transport,
            &self.release,
            dir,
            cancel,
            &mut callback,
            &self.armor,
        ))
    }

    fn partial(&self, dir: &Path, length: usize) -> ResumeToken {
        let cache = VerifiedCache::new(dir).unwrap();
        let mut session = cache.begin(self.release.spec().unwrap()).unwrap();
        session.append(&self.data[..length]).unwrap();
        session.checkpoint().unwrap();
        save_pointer(
            &dir.join(format!("release-{}.resume", self.release.sha256)),
            session.token(),
        )
        .unwrap();
        session.token().clone()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            worker.join().unwrap();
        }
    }
}

fn serve(
    mut stream: TcpStream,
    release: &Release,
    data: &[u8],
    mode: u8,
    requests: &Mutex<Vec<String>>,
) -> io::Result<()> {
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request = String::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return Ok(());
        }
        request.push_str(&line);
        if line == "\r\n" {
            break;
        }
    }
    requests.lock().unwrap().push(request.clone());
    let first = request.lines().next().unwrap();
    let path = first.split_whitespace().nth(1).unwrap();
    let head = first.starts_with("HEAD ");
    let offset = request.lines().find_map(|line| {
        line.to_ascii_lowercase()
            .strip_prefix("range: bytes=")
            .and_then(|value| value.trim_end_matches('-').parse::<usize>().ok())
    });
    let mut status = "200 OK";
    let mut extra = String::new();
    let mut body = if path == "/" {
        if mode == 8 {
            vec![b'x'; MAX_HOME + 1]
        } else {
            format!("<a href=\"{}\">ISO</a>", release.url).into_bytes()
        }
    } else if path.ends_with(".sha256") {
        format!(
            "{}  {}\n",
            release.sha256,
            if mode == 6 {
                "../wrong.iso"
            } else {
                &release.file_name
            }
        )
        .into_bytes()
    } else if path.ends_with(".sig") {
        release.signature.clone()
    } else {
        data.to_vec()
    };
    if path.ends_with(".iso") && !head {
        if mode == 5 {
            status = "302 Found";
            extra = "Location: https://example.com/steal\r\n".into();
        }
        if mode == 4 {
            extra.push_str("Content-Encoding: gzip\r\n");
        }
        if let Some(offset) = offset
            && mode != 2
        {
            status = "206 Partial Content";
            write!(
                extra,
                "Content-Range: bytes {}-{}/{}\r\n",
                if mode == 1 { offset + 1 } else { offset },
                data.len() - 1,
                data.len()
            )
            .unwrap();
            body = data[offset..].to_vec();
        }
    }
    let length = if mode == 7 && head { 0 } else { body.len() };
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Length: {length}\r\n{extra}Connection: close\r\n\r\n"
    )?;
    if !head {
        if mode == 3 && path.ends_with(".iso") {
            body.truncate(body.len() / 2);
        }
        if mode == 9 && path.ends_with(".iso") {
            body[100] ^= 1;
        }
        for chunk in body.chunks(4096) {
            stream.write_all(chunk)?;
            if path.ends_with(".iso") {
                thread::sleep(Duration::from_millis(1));
            }
        }
    }
    Ok(())
}

#[test]
fn pinned_key_and_current_signature_packet_are_supported() {
    let (key, _) = SignedPublicKey::from_string(PINNED_KEY).unwrap();
    assert_eq!(
        key.fingerprint().to_string().to_uppercase(),
        SIGNER_FINGERPRINT
    );
    key.verify_bindings().unwrap();
}

#[test]
fn discovery_is_bounded_and_rejects_malformed_metadata() {
    let f = Fixture::new();
    let runtime = runtime().unwrap();
    let cancel = AtomicBool::new(false);
    let release = runtime
        .block_on(resolve_with(&f.transport, &cancel))
        .unwrap();
    assert_eq!(release.file_name, f.release.file_name);
    assert_eq!(release.length, f.data.len() as u64);
    for mode in [6, 7, 8] {
        f.mode.store(mode, Ordering::Relaxed);
        assert!(
            runtime
                .block_on(resolve_with(&f.transport, &cancel))
                .is_err(),
            "mode {mode}"
        );
    }
}

#[test]
fn official_urls_and_filenames_fail_closed() {
    let t = Transport::official().unwrap();
    for url in [
        "http://iso.omarchy.org/omarchy-1.2.3.iso",
        "https://evil.example/omarchy-1.2.3.iso",
        "https://user@iso.omarchy.org/omarchy-1.2.3.iso",
        "https://iso.omarchy.org:444/omarchy-1.2.3.iso",
        "https://iso.omarchy.org/omarchy-1.2.3.iso?key=wrong",
        "https://iso.omarchy.org/omarchy-1.2.3.iso#x",
    ] {
        assert!(t.validate_url(url).is_err(), "{url}");
    }
    for name in [
        "../omarchy-1.2.3.iso",
        "omarchy-1.2.3/evil.iso",
        "omarchy-01.2.3.iso",
        "omarchy-1.2.3-beta.iso",
    ] {
        assert!(version_from_name(name).is_err());
    }
    assert!(discover_file("<a href=\"https://iso.omarchy.org/omarchy-1.2.3.iso\"><a href=\"https://iso.omarchy.org/omarchy-2.3.4.iso\">", ISO_ORIGIN).is_err());
    assert!(
        checksum_for(
            format!("{}  ../x\n", "a".repeat(64)).as_bytes(),
            "omarchy-1.2.3.iso"
        )
        .is_err()
    );
}

#[test]
fn cancellation_persists_and_resumes_then_cache_revalidates_signature() {
    let f = Fixture::new();
    let dir = TempDir::new().unwrap();
    let cancel = AtomicBool::new(false);
    let failure = f.download(dir.path(), &cancel, |p| {
        if p.phase == DownloadPhase::Downloading && p.received_bytes > 0 {
            cancel.store(true, Ordering::Relaxed);
        }
    });
    assert!(matches!(failure, Err(Error::Cancelled)));
    cancel.store(false, Ordering::Relaxed);
    let image = f.download(dir.path(), &cancel, |_| {}).unwrap();
    assert!(image.signature_verified);
    assert!(!image.cache_hit);
    assert_eq!(fs::read(&image.path).unwrap(), f.data);
    assert!(
        f.requests
            .lock()
            .unwrap()
            .iter()
            .any(|r| r.to_lowercase().contains("range: bytes="))
    );
    let cached = f.download(dir.path(), &cancel, |_| {}).unwrap();
    assert!(cached.cache_hit && cached.signature_verified);
    let mut wrong = f.release.clone();
    let last = wrong.signature.len() - 1;
    wrong.signature[last] ^= 1;
    assert!(
        runtime()
            .unwrap()
            .block_on(download_with(
                &f.transport,
                &wrong,
                dir.path(),
                &cancel,
                &mut |_| {},
                &f.armor
            ))
            .is_err()
    );
    fs::write(image.path, b"corrupt cached image").unwrap();
    let repaired = f.download(dir.path(), &cancel, |_| {}).unwrap();
    assert!(!repaired.cache_hit && repaired.signature_verified);
}

#[test]
fn bad_range_preserves_partial_and_ignored_range_restarts() {
    let f = Fixture::new();
    let dir = TempDir::new().unwrap();
    let token = f.partial(dir.path(), 512);
    f.mode.store(1, Ordering::Relaxed);
    assert!(
        f.download(dir.path(), &AtomicBool::new(false), |_| {})
            .is_err()
    );
    assert_eq!(
        VerifiedCache::new(dir.path())
            .unwrap()
            .resume(&token)
            .unwrap()
            .state()
            .received_len,
        512
    );
    f.mode.store(2, Ordering::Relaxed);
    let image = f
        .download(dir.path(), &AtomicBool::new(false), |_| {})
        .unwrap();
    assert_eq!(fs::read(image.path).unwrap(), f.data);
}

#[test]
fn truncation_encoding_and_redirects_never_promote() {
    let f = Fixture::new();
    for mode in [3, 4, 5] {
        f.mode.store(mode, Ordering::Relaxed);
        let dir = TempDir::new().unwrap();
        assert!(
            f.download(dir.path(), &AtomicBool::new(false), |_| {})
                .is_err(),
            "mode {mode}"
        );
        assert_eq!(
            fs::read_dir(dir.path().join("artifacts")).unwrap().count(),
            0
        );
    }
}

#[test]
fn digest_mismatch_and_untrusted_signer_never_promote() {
    let f = Fixture::new();
    let cancel = AtomicBool::new(false);
    let dir = TempDir::new().unwrap();
    let mut wrong = f.release.clone();
    wrong.sha256 = "a".repeat(64);
    let failure = runtime().unwrap().block_on(download_with(
        &f.transport,
        &wrong,
        dir.path(),
        &cancel,
        &mut |_| {},
        &f.armor,
    ));
    assert!(matches!(
        failure,
        Err(Error::Cache(
            omarchy_downloader::Error::DigestMismatch { .. }
        ))
    ));
    assert_eq!(
        fs::read_dir(dir.path().join("artifacts")).unwrap().count(),
        0
    );
    assert_eq!(
        fs::read_dir(dir.path().join("partials")).unwrap().count(),
        0
    );
    let second = Fixture::new();
    let failure = runtime().unwrap().block_on(download_with(
        &f.transport,
        &f.release,
        dir.path(),
        &cancel,
        &mut |_| {},
        &second.armor,
    ));
    assert!(matches!(failure, Err(Error::Signature(_))));
    // A public test key must also be rejected by the production verifier.
    assert!(verify_signature(dir.path(), &f.release, PINNED_KEY, &cancel, &mut |_| {}).is_err());
}

#[test]
fn malformed_range_signature_and_checksum_are_rejected() {
    for range in [
        "bytes 4-9/11",
        "bytes 3-9/10",
        "bytes 4-10/10",
        "items 4-9/10",
        "bytes 4-9/*",
        "bytes +4-9/10",
        "bytes 4-9/18446744073709551616",
    ] {
        assert!(validate_range(range, 4, 10).is_err(), "{range}");
    }
    validate_range("bytes 4-9/10", 4, 10).unwrap();
    let f = Fixture::new();
    let mut doubled = f.release.signature.clone();
    doubled.extend_from_slice(&f.release.signature);
    assert!(parse_signature(&doubled).is_err());
    assert!(parse_signature(&[]).is_err());
    assert!(parse_signature(&vec![0; MAX_SIGNATURE + 1]).is_err());
    assert!(
        checksum_for(
            format!("{}  {}\nextra", f.release.sha256, f.release.file_name).as_bytes(),
            &f.release.file_name
        )
        .is_err()
    );
}

#[test]
fn cancel_before_start_creates_no_cache() {
    let f = Fixture::new();
    let dir = TempDir::new().unwrap();
    let cache = dir.path().join("absent");
    assert!(matches!(
        f.download(&cache, &AtomicBool::new(true), |_| {}),
        Err(Error::Cancelled)
    ));
    assert!(!cache.exists());
}

#[test]
fn corrupt_complete_response_is_discarded_and_retry_fetches_again() {
    let f = Fixture::new();
    let dir = TempDir::new().unwrap();
    let cancel = AtomicBool::new(false);
    f.mode.store(9, Ordering::Relaxed);
    let failure = f.download(dir.path(), &cancel, |_| {});
    assert!(matches!(
        failure,
        Err(Error::Cache(
            omarchy_downloader::Error::DigestMismatch { .. }
        ))
    ));
    assert_eq!(
        fs::read_dir(dir.path().join("partials")).unwrap().count(),
        0
    );
    f.mode.store(0, Ordering::Relaxed);
    let image = f.download(dir.path(), &cancel, |_| {}).unwrap();
    assert!(image.signature_verified);
    assert_eq!(fs::read(image.path).unwrap(), f.data);
    assert_eq!(f.requests.lock().unwrap().len(), 2);
}

#[test]
fn torn_pointer_and_malformed_state_recover_without_deleting_untrusted_files() {
    let fixture = Fixture::new();
    let dir = TempDir::new().unwrap();
    let outside = dir.path().join("outside");
    fs::write(&outside, b"keep me").unwrap();
    let cancel = AtomicBool::new(false);
    for (index, value) in [
        Vec::new(),
        b"../outside".to_vec(),
        vec![b'a'; 129],
        vec![0xff],
    ]
    .into_iter()
    .enumerate()
    {
        let cache_path = dir.path().join(format!("cache-{index}"));
        let _cache = VerifiedCache::new(&cache_path).unwrap();
        let pointer = cache_path.join(format!("release-{}.resume", fixture.release.sha256));
        fs::write(pointer, value).unwrap();
        let image = fixture.download(&cache_path, &cancel, |_| {}).unwrap();
        assert!(image.signature_verified);
        assert_eq!(fs::read(&outside).unwrap(), b"keep me");
    }
    let malformed_cache = dir.path().join("malformed");
    let token = fixture.partial(&malformed_cache, 512);
    let state = malformed_cache
        .join("partials")
        .join(format!("{}.json", token.as_str()));
    let partial = malformed_cache
        .join("partials")
        .join(format!("{}.part", token.as_str()));
    fs::write(&state, b"{torn json").unwrap();
    assert!(
        fixture
            .download(&malformed_cache, &cancel, |_| {})
            .unwrap()
            .signature_verified
    );
    assert_eq!(fs::read(state).unwrap(), b"{torn json");
    assert_eq!(fs::read(partial).unwrap(), fixture.data[..512]);
}

#[test]
fn cancellation_during_partial_and_cache_rehash_preserves_valid_state() {
    let fixture = Fixture::new();
    let dir = TempDir::new().unwrap();
    let token = fixture.partial(dir.path(), 512 * 1024);
    let cancel = AtomicBool::new(false);
    let cancel_hash = |p: DownloadProgress| {
        if p.phase == DownloadPhase::Preparing && p.received_bytes > 0 {
            cancel.store(true, Ordering::Relaxed);
        }
    };
    assert!(matches!(
        fixture.download(dir.path(), &cancel, cancel_hash),
        Err(Error::Cancelled)
    ));
    let pointer = dir
        .path()
        .join(format!("release-{}.resume", fixture.release.sha256));
    assert_eq!(fs::read_to_string(&pointer).unwrap(), token.as_str());
    cancel.store(false, Ordering::Relaxed);
    let image = fixture.download(dir.path(), &cancel, |_| {}).unwrap();
    assert!(matches!(
        fixture.download(dir.path(), &cancel, cancel_hash),
        Err(Error::Cancelled)
    ));
    assert!(image.path.exists());
    cancel.store(false, Ordering::Relaxed);
    assert!(
        fixture
            .download(dir.path(), &cancel, |_| {})
            .unwrap()
            .cache_hit
    );
}

#[test]
#[ignore = "explicit read-only official network smoke; no ISO body download"]
fn official_metadata_smoke() {
    let release = resolve_current().unwrap();
    assert_eq!(release.signer_fingerprint(), SIGNER_FINGERPRINT);
    assert!(release.length() > 1024 * 1024 * 1024);
    println!(
        "{} {} {}",
        release.version(),
        release.length(),
        release.sha256()
    );
}
