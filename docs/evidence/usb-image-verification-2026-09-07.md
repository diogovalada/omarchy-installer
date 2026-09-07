# USB image verification

Implementation and verification: `gpt-6-astra`.

After USB approval, the elevated helper already verified the ISO's OpenPGP
signature and SHA-256 in one pass. The physical writer then hashed the same
source before writing and again after USB readback. On Windows, both additional
ISO scans were redundant: the helper retained a handle denying writes and
deletion, with every ancestor held against renaming, throughout the operation.

The helper now passes a live source binding only after successful authentication.
The writer checks the parent PID, volume serial and full 64-bit file index against
its opened descriptor. It checks identity, metadata and parent liveness before
writing and before success. Invalid bindings fail closed. The helper retains its
guard until child exit; the worker terminates if its lifetime pipe or parent is
lost. Callers without this Windows handoff retain both source hash passes.

The UI now calls the combined signature and checksum stage "Verifying the
official image". SDK verification, flushing, full USB SHA-256 readback and
final-sector padding verification remain in place.

Validation:

- 24 media tests passed, including wrong-file, malformed-binding, lost-parent,
  changed-source and 64-bit identity precision cases. The valid handoff performs
  no source reads and emits no extra hashing stage.
- 23 native tests passed. The Windows guard blocks writes and renames, and its
  file identity agrees with Node's bigint metadata.
- A separate integration check used the bundled Node 22.13.1 and the actual
  adapter under a retained native guard. Both verification calls completed
  without reading the disposable source fixture again.

No physical USB write was performed for this change. Hardware qualification
remains outstanding.

Node's volume and file identifiers follow the pinned
[libuv Windows stat implementation](https://github.com/libuv/libuv/blob/v1.49.2/src/win/fs.c).
The comparison uses the low 32 bits of Node's volume serial to match Win32's
`GetFileInformationByHandle`, and preserves all 64 file-index bits.
