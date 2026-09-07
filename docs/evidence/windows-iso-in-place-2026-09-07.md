# Windows setup reads the original ISO — 2026-09-07

Implementation and verification model: `gpt-6-astra`.

Windows USB creation (erase and keep-files modes) and direct installation now
retain the ISO at its original path. The elevated helper no longer creates a
second ISO in the operation directory. It still independently authenticates the
official detached signature and SHA-256 before permitting a consumer to proceed.
Signature verification now forwards byte progress to the setup UI.

`iso_source::HeldIso` opens the original file with read-only sharing and keeps
every ancestor open without delete sharing. The open handles reject reparse
points and unexpected file types. The ISO handle rejects unexpected length and
conflicting writers. The guard remains in scope while the provider executes,
then releases on success, failure or cancellation. No permanent ACL or file
attribute change is made to the user's ISO.

The direct builder receives the original ISO and the small helper-owned
signature from separate host paths. Docker mounts the two files read-only under
canonical names in `/input`, without mounting the ISO's parent directory.
Other host platforms keep the existing protected-copy behavior until equivalent
mandatory lifetime protection is implemented; advisory POSIX locking is not
treated as sufficient.

Validation:

- 22 native tests passed, including real Windows sharing behavior, parent/file
  rename rejection, conflicting writer rejection, failure/cancellation cleanup,
  source preservation, and the existing USB review/keep-files boundaries.
- The explicit local container test passed using pinned runtime
  `sha256:020be814fec7b78e7f9baa3a49a53227cd10a98d210d621f9d4fd4f4457a3c8b`.
  It read disposable fixture files from separate host locations while the ISO
  guard remained alive, and an attempted write through the mount was rejected.
  The container used `--pull=never`, `--network none`, and `--rm`.
- The explicit existing-ISO test authenticated all 6,227,752,960 bytes of
  `C:\Users\example\Downloads\Omarchy\omarchy-4.0.2.iso` at that exact path in
  34.050 seconds, with 174 progress events. Only the detached signature appeared
  in the temporary operation directory. This is an observed verification time,
  not a guaranteed duration or a measurement of complete installation speed.
- The direct-provider PowerShell script passed syntax parsing.

No physical USB write, partition mutation or full installed-system construction
was performed for this change. Signature checks and provider readback remain;
the change removes the extra host-side ISO copy.

References:

- [Windows file sharing and reparse-point flags](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew)
- [Docker file and directory bind mounts](https://docs.docker.com/engine/storage/bind-mounts/)
