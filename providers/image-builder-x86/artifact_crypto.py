"""Ciphertext-only export of bounded image streams; no plaintext/key files.

Wire contract: docs/evidence/artifact-encryption-contract.md.
Implementation provenance: gpt-6-astra. Runtime crypto qualification is deferred.
This module has no command-line entry point and does not read keys from the
environment, argv, a key file, or a logger. The caller supplies an operation key
received over its private stdin and remains responsible for that key's lifetime.
"""

from __future__ import annotations

import hashlib
import hmac
import os
from pathlib import Path
import selectors
import stat
import subprocess
import threading
from typing import Callable, Iterable
import uuid


FORMAT = "openssl-aes-256-cbc-pbkdf2-sha256"
ITERATIONS = 100_000
MAC_DOMAIN = b"omarchy-artifact-mac-v1"
MAX_PLAINTEXT_BYTES = 64 * 1024**3
CHUNK_BYTES = 1024 * 1024
_MAGIC = b"Salted__"
_OPENSSL = "/usr/bin/openssl"


class ArtifactCryptoError(RuntimeError):
    """Export failed; no completed encrypted-artifact receipt is available."""


def _wipe(value: bytearray) -> None:
    value[:] = b"\0" * len(value)


def _password(key: bytearray) -> bytearray:
    # Avoid creating an immutable Base64 password string. Standard padded Base64
    # is exactly 44 ASCII bytes for the required 32-byte operation key.
    alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
    result = bytearray()
    for offset in range(0, len(key), 3):
        count = min(3, len(key) - offset)
        word = key[offset] << 16
        if count > 1:
            word |= key[offset + 1] << 8
        if count > 2:
            word |= key[offset + 2]
        result.extend((alphabet[(word >> 18) & 63], alphabet[(word >> 12) & 63],
                       alphabet[(word >> 6) & 63] if count > 1 else ord("="),
                       alphabet[word & 63] if count > 2 else ord("=")))
    return result


def _checked_destination(output_path: str | Path, role: str) -> Path:
    destination = Path(output_path)
    if not destination.is_absolute() or role not in {"esp", "root"} or destination.name != role + ".img.enc":
        raise ArtifactCryptoError("Expected an absolute role-specific encrypted output path")
    if ".." in destination.parts:
        raise ArtifactCryptoError("Parent traversal is not allowed in encrypted output paths")
    parent = destination.parent
    while True:
        metadata = parent.lstat()
        if not stat.S_ISDIR(metadata.st_mode) or stat.S_ISLNK(metadata.st_mode):
            raise ArtifactCryptoError("Encrypted output parent must be an ordinary existing directory")
        if parent == parent.parent:
            break
        parent = parent.parent
    return destination


def _drain_stderr(pipe) -> None:
    # OpenSSL diagnostics are never copied into an artifact manifest or log.
    # Draining to EOF avoids a subprocess pipe deadlock even on repeated errors.
    try:
        while pipe.read(8192):
            pass
    finally:
        pipe.close()


def _stop(process: subprocess.Popen | None) -> None:
    if process is None:
        return
    if process.stdin is not None:
        process.stdin.close()
    if process.poll() is None:
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()
    else:
        process.wait()


def encrypt_image(
    output_path: str | Path,
    key: bytes | bytearray,
    chunks: Iterable[bytes | bytearray | memoryview],
    total_size: int,
    operation_id: str,
    role: str,
    check_cancel: Callable[[], None],
    on_progress: Callable[[int], None] | None = None,
) -> dict:
    """Encrypt exactly total_size bytes from <=1 MiB chunks into a new file.

    check_cancel must raise to cancel; it is called before creation, between
    chunks, during pipe backpressure, process finalization and ciphertext hashing.
    on_progress receives plaintext bytes supplied to encryption. Observer errors
    abort the export. Neither callback receives a key or plaintext buffer.

    The installed, image-pinned OpenSSL 3.0 runtime generates its own eight-byte
    random salt. PKCS7 ciphertext length and the Salted__ header are checked before
    a receipt is returned; a runtime that changes the envelope fails closed.
    """
    if os.name != "posix":
        raise ArtifactCryptoError("Encrypted artifact export requires the isolated Linux runtime")
    if not isinstance(key, (bytes, bytearray)) or len(key) != 32:
        raise ArtifactCryptoError("A 32-byte in-memory operation key is required")
    if isinstance(total_size, bool) or not isinstance(total_size, int) or not 0 < total_size <= MAX_PLAINTEXT_BYTES:
        raise ArtifactCryptoError("Plaintext image length is outside the fixed-recipe bound")
    if not isinstance(operation_id, str) or str(uuid.UUID(operation_id)) != operation_id.lower():
        raise ArtifactCryptoError("A canonical operation UUID is required")
    if not callable(check_cancel) or (on_progress is not None and not callable(on_progress)):
        raise ArtifactCryptoError("Cancellation and progress callbacks are invalid")
    check_cancel()
    destination = _checked_destination(output_path, role)
    expected_stored = 16 + 16 * (total_size // 16 + 1)
    key_copy = bytearray(key)
    password = _password(key_copy)
    mac_key = bytearray(hmac.new(key_copy, MAC_DOMAIN, hashlib.sha256).digest())
    clear_hash = hashlib.sha256()
    process = None
    stderr_thread = None
    password_read = password_write = None
    output = None
    created_identity = None
    completed = False
    try:
        descriptor = os.open(destination, os.O_RDWR | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
        output = os.fdopen(descriptor, "r+b", buffering=0)
        created_identity = os.fstat(output.fileno())
        if not stat.S_ISREG(created_identity.st_mode):
            raise ArtifactCryptoError("Encrypted output is not a regular file")
        password_read, password_write = os.pipe()
        # No password in command arguments or environment. Only the anonymous
        # descriptor number is passed; the parent writes one private password line.
        process = subprocess.Popen(
            [_OPENSSL, "enc", "-aes-256-cbc", "-pbkdf2", "-iter", str(ITERATIONS),
             "-md", "sha256", "-salt", "-pass", "fd:" + str(password_read)],
            stdin=subprocess.PIPE, stdout=output, stderr=subprocess.PIPE,
            pass_fds=(password_read,), close_fds=True, bufsize=0,
            env={"PATH": "/usr/bin:/bin", "LANG": "C", "LC_ALL": "C"},
        )
        os.close(password_read)
        password_read = None
        assert process.stderr is not None and process.stdin is not None
        stderr_thread = threading.Thread(target=_drain_stderr, args=(process.stderr,), daemon=True)
        stderr_thread.start()
        password.append(10)
        # 45 bytes fit atomically in an empty anonymous pipe (well below PIPE_BUF).
        if os.write(password_write, password) != len(password):
            raise ArtifactCryptoError("Could not supply the encryption password privately")
        os.close(password_write)
        password_write = None
        _wipe(password)
        _wipe(key_copy)
        os.set_blocking(process.stdin.fileno(), False)
        total = 0
        with selectors.DefaultSelector() as writable:
            writable.register(process.stdin, selectors.EVENT_WRITE)
            for chunk in chunks:
                check_cancel()
                if not isinstance(chunk, (bytes, bytearray, memoryview)):
                    raise ArtifactCryptoError("Image producer must yield nonempty chunks no larger than 1 MiB")
                chunk_view = memoryview(chunk)
                try:
                    if not chunk_view.c_contiguous or not 0 < chunk_view.nbytes <= CHUNK_BYTES:
                        raise ArtifactCryptoError("Image producer must yield contiguous byte chunks no larger than 1 MiB")
                    if total + chunk_view.nbytes > total_size:
                        raise ArtifactCryptoError("Image producer exceeded the declared plaintext boundary")
                    clear = bytearray(chunk_view.cast("B"))
                finally:
                    chunk_view.release()
                view = memoryview(clear)
                try:
                    clear_hash.update(view)
                    sent = 0
                    while sent < len(clear):
                        check_cancel()
                        if process.poll() is not None:
                            raise ArtifactCryptoError("OpenSSL exited before consuming the image")
                        if not writable.select(timeout=0.2):
                            continue
                        try:
                            count = os.write(process.stdin.fileno(), view[sent:])
                        except BlockingIOError:
                            continue
                        if count <= 0:
                            raise ArtifactCryptoError("Encryption input pipe stopped accepting bytes")
                        sent += count
                    total += len(clear)
                    if on_progress is not None:
                        on_progress(total)
                finally:
                    view.release()
                    _wipe(clear)
        if total != total_size:
            raise ArtifactCryptoError("Image producer ended before the declared plaintext boundary")
        process.stdin.close()
        while True:
            check_cancel()
            try:
                status = process.wait(timeout=0.2)
                break
            except subprocess.TimeoutExpired:
                continue
        if status != 0:
            raise ArtifactCryptoError("OpenSSL encryption did not complete successfully")
        stderr_thread.join()
        output.flush()
        os.fsync(output.fileno())
        if os.fstat(output.fileno()).st_size != expected_stored:
            raise ArtifactCryptoError("Ciphertext length differs from the fixed 8-byte-salt PKCS7 envelope")
        output.seek(0)
        header = output.read(16)
        if len(header) != 16 or header[:8] != _MAGIC:
            raise ArtifactCryptoError("OpenSSL did not produce the required Salted__ envelope")
        output.seek(0)
        stored_hash = hashlib.sha256()
        authenticator = hmac.new(mac_key, digestmod=hashlib.sha256)
        stored_total = 0
        while True:
            check_cancel()
            ciphertext = output.read(CHUNK_BYTES)
            if not ciphertext:
                break
            stored_total += len(ciphertext)
            if stored_total > expected_stored:
                raise ArtifactCryptoError("Ciphertext grew while being authenticated")
            stored_hash.update(ciphertext)
            authenticator.update(ciphertext)
        if stored_total != expected_stored or os.fstat(output.fileno()).st_size != expected_stored:
            raise ArtifactCryptoError("Ciphertext changed while being authenticated")
        check_cancel()
        receipt = {
            "file": destination.name,
            "sizeBytes": total_size,
            "sha256": clear_hash.hexdigest(),
            "envelope": {
                "format": FORMAT,
                "iterations": ITERATIONS,
                "hmacSha256": authenticator.hexdigest(),
                "storedSizeBytes": stored_total,
                "storedSha256": stored_hash.hexdigest(),
            },
        }
        # Close failures must prevent receipt publication.
        output.close()
        output = None
        completed = True
        return receipt
    finally:
        try:
            _stop(process)
        finally:
            try:
                for descriptor in (password_read, password_write):
                    if descriptor is not None:
                        os.close(descriptor)
                if output is not None:
                    output.close()
                if stderr_thread is not None:
                    stderr_thread.join()
            finally:
                _wipe(key_copy)
                _wipe(password)
                _wipe(mac_key)
                if not completed and created_identity is not None:
                    # Remove only this invocation's unfinished ciphertext file.
                    # A replaced pathname is never followed or deleted.
                    try:
                        current = destination.lstat()
                        if (current.st_dev, current.st_ino) == (created_identity.st_dev, created_identity.st_ino):
                            destination.unlink()
                    except FileNotFoundError:
                        pass
