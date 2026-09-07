# Verified reusable portable cache

The previous portable launcher expanded about 280 MB across roughly 3,400 files into a
new temporary folder on every run and removed it on exit. The repeat cost was a
packaging choice. Windows can clean temporary files according to Storage Sense
settings, but temporary storage does not provide predictable cache retention:
[Microsoft Storage Sense documentation](https://support.microsoft.com/en-us/windows/experience/storage-filemanagement/manage-drive-space-with-storage-sense).

## Cache identity and validation

The launcher now keeps the payload in `%LOCALAPPDATA%\OmarchySetup\p\<id>`.
The ID is the first 20 hexadecimal digits of a SHA-256 digest of the complete
payload manifest. The **full SHA-256 digest** is compiled into the launcher and
checked every time. The manifest includes every payload file's relative path,
size and SHA-256, including the application, providers, notices and payload file
list. A version label or a writable “extraction complete” flag is insufficient.

A 276 KB native verifier and the expected manifest are freshly extracted from
the opened executable before any cached program runs. The verifier checks the
embedded manifest digest, scans for missing and unexpected entries, rejects
reparse points, checks file sizes and hashes every file. It transfers verified
read handles to the launcher, denying writes or deletion of the checked files
while the app runs. Directory handles protect the checked directory locations
from renaming. This is cache integrity checking, not a replacement for signing
the portable executable or an isolation boundary against a compromised account.

The 20-digit directory name keeps Windows paths short. A collision in that
abbreviation still cannot authenticate different bytes: the full manifest hash
and every payload file must match the executable being opened.

## Cold launch, recovery and simultaneous launch

A cache hit skips payload extraction. A missing or damaged cache is rebuilt in
a separate `.part` directory. The new generation is checked before publication,
then reopened and checked at its final name before launch. A mutex serializes
extraction and publication for the same user and payload; separate launches can
then share the verified files. The mutex is released after the app starts.

An interrupted staging directory or invalid older cache is renamed aside before
replacement. These directories remain within the application's cache folder;
the launcher does not recursively delete unknown files. Older payload versions
also remain available. This uses about 280 MB per retained complete generation.
The entire application cache can be removed while its apps are closed, and the
next launch recreates it. Downloads and installation records live elsewhere.

Normal exit deletes only the small temporary verifier/plugin directory and
retains the reusable payload. A new payload manifest selects a new cache ID.
Changes solely to the launcher/verifier can reuse identical payload bytes; the
fresh verifier always comes from the executable being opened.

Compression now uses independent zlib blocks so a cache hit can read the small
bootstrap without decompressing the payload. The launcher retains its opening
indicator until the app window exists, closes the indicator, then activates the
app once with normal foreground behavior.

## Verification

- Native tests cover same-size tampering, missing files, unexpected DLLs,
  changed-version contents, manifest mismatch, unsafe paths, interrupted staging,
  repeated reuse, and checked-file write/rename protection.
- A real directory-junction fixture was rejected without changing its target.
- Packaged checks cover cold, warm and damaged-cache startup, payload integrity,
  cache retention, unchanged cache creation timestamps on reuse, file protection,
  foreground focus and temporary verifier cleanup.
- `verify-portable-concurrent-startup.ps1` launches two instances against an
  absent cache and checks that both open from the completed, verified cache.

The package directory contains the payload manifest, verifier source/lockfile
hashes, package hashes and startup measurements. Before this change, the measured
startup was about 11.7 seconds on the packaging machine. Initial cache tests
measured about 16.8 seconds for extraction plus verification and 3.8 seconds for
a fully verified repeat launch. These are local observations, not guarantees
for other disks, antivirus configurations or machines.

Final package `39a9689f` measured a 3.75-second verified repeat launch and a
17.46-second automatic repair after same-size cache corruption. Its two concurrent
cold launches both opened successfully in 22.13 seconds and shared a valid cache.
Warm and repair launches retained foreground focus with no intervening input.
The initial cold-launch cache checks passed, but its foreground check failed;
that earlier sample did not record input activity. Later checks record input and
mark interrupted foreground assessments inconclusive rather than claiming a pass.
Test generations are retained with the project's test artifacts; only the current
valid payload remains in the application's cache folder on the test machine.
