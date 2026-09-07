# Shared Windows and macOS storage controls

Date: 2026-09-06. Development source; disk execution remains unqualified.

The user requested removing the adapter's existing-install restriction and using
our frontend for macOS partition management. Windows and Mac now both mount
`StorageControls.svelte`, sharing target selection, allocation entry, replacement
disclosure and the typed-confirmation modal. Windows keeps its existing eligible
free-space, NTFS shrink and unused-partition deletion choices and backend guards.

On Mac, the retained native backend selects suitable free space or macOS resize
for alongside installation. Its signed preparation can surface existing Omarchy
installations. The frontend then offers alongside or full replacement of one
detected installation. Arbitrary foreign partitions, repair, and manually chosen
raw extents are not exposed by this native session API.

Each native replacement option receives an opaque session-local choice ID and a
human identifier such as `Installation 1`. The shared modal names the source and
full size, warns that its operating system and files will be lost, and requires
typing that identifier. Allocation is fixed for replacement. A prior custom
alongside allocation is prefilled when the native existing-install screen appears;
clearing the entry restores the native recommendation.

The Tauri client checks the current revision, choice and typed identifier. Swift
resolves the ID to an actual retained `ExistingInstallDisplay`, confirms that it
still belongs to the current session options, repeats the typed check, and calls
the original native session API. It forwards the actual native selection through
the environment, replacing the old automatic-only restriction. Advisory layout
JSON remains display-only and cannot authorize a disk extent.

No disk changes occur on choice submission. The prepared plan includes explicit
replacement context for frontend review and both native confirmation stages.
Upstream still owns signed fresh inspection, exact candidate/plan binding,
approval, machine-owner authorization, helper execution, journals and Recovery.
The upstream submodule and release/source pins are unchanged. The catalog still
enables only `apple,j314s`; `apple,j614s`, Intel and Rosetta remain blocked.

Validation: frontend type check passes with zero errors and four pre-existing
warnings; Windows Rust library check and formatting pass. The updated Windows
native build passed; its result is recorded in the
[build record](shared-storage-build-2026-09-06.json). No UI, disk,
helper, VM, installation, reboot or Recovery execution tests were run, following
the user's instruction to defer testing. The Windows build does not compile the
macOS-only Rust module or Swift bridge. Those require a macOS toolchain, signed
packaging and subsequent qualification; no Mac build success is claimed.
