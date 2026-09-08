# Retaining verified Windows ISO files

The download controller now acquires a Windows deny-write/delete file handle and
holds every ancestor against renaming before verifying an existing ISO. It keeps
those handles in native state after showing Verified. A fresh download verifies
the final saved ISO under the same lifetime guard: the cache is a different file.
Changing download state releases its ownership; an active operation retains its
own reference. Closing the application releases idle locks.

The native Rust controller is the verification authority, following the trusted
policy-core boundary. The webview cannot supply release metadata, mark a file
verified, create a lease or send helper protocol frames. Installation obtains a
lease only when its complete source constraints match the verified native state.
No saved receipt, path or verified boolean enables reuse.

Over the existing authenticated native/helper channel, the helper opens its own
deny-write/delete handle, checks volume serial, full file index, length, checksum,
signature digest, parent PID and process creation time, then requests a live
acknowledgement. The desktop acknowledges only while retaining the matching
verified lease. This closes the handoff gap even if download state is reset.
Malformed or stale handoffs fail closed; callers without a handoff still perform
the full pinned signature/checksum verification. USB readback remains unchanged.

For Windows direct installation, the provider acquires an additional source and
ancestor guard. It validates the helper-owned request, pinned release constraints,
signature file, actual parent process and its creation time before reuse. It holds
the guard through Docker completion/cleanup. The private staging-input protocol
then carries the verified source constraints to the pinned construction runtime.
The container requires exact individual read-only bind mounts for the ISO and
signature and checks their size/signature digest. This removes redundant ISO hash
and signature scans while preserving the verified input's lifetime. Legacy private
input and standalone construction retain full source authentication.

Validation without administrator prompts or physical disk writes:

- 35 native tests passed; five environment-dependent tests excluded from the
  default run. Tests cover lock lifetime/cancellation, mismatched identities and
  constraints, missing/negative acknowledgements, and reuse without rescanning.
- An additional test verified the existing 6,227,752,960-byte official ISO under
  its retained lock. The final run took 20.494 seconds for authentication and
  0.000539 seconds for helper identity handoff, with no second source scan.
- 22 UI tests passed; Svelte check reported no errors and four existing warnings.
- 14 construction tests passed in the pinned Linux runtime, including private
  protocol compatibility, malformed proofs and non-read-only mount rejection.
- Windows PowerShell 5.1 compiled and exercised the C# source guard against
  disposable files. It checked identity, blocked writes/ancestor renames, released
  handles correctly, and parsed all direct-provider scripts.
- A real Docker fixture used the C# guard and private input protocol with individual
  read-only mounts. The production construction verifier reused source verification
  with zero ISO hash passes. The fixture did not construct or deploy an OS image.

The Linux container tests avoid Windows' administrator requirement for creating
the existing symbolic-link fixture. Physical installation qualification is unchanged.
