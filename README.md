# Omarchy Setup

A desktop app for downloading Omarchy, creating an installation USB, and
installing alongside an existing operating system.

This is an unofficial community project. Installation features are still
experimental and have not completed physical hardware and boot testing.

## What it does

- Downloads official releases with resume support, SHA-256 checks and OpenPGP
  signature verification.
- Creates bootable USB drives with readback verification. Keeping existing files
  is supported only on compatible USB layouts.
- Provides a Windows installation flow with space allocation, encryption and a
  boot menu. Direct installation requires administrator access and a local
  Docker runtime.
- Integrates a native Apple Silicon installer through a pinned upstream
  submodule. macOS builds require signed companion tools.

On Windows, the app runs from one portable executable. It keeps verified
application files in a local cache and uses the downloaded ISO in place.
After verification, it keeps the ISO read-only while the app is open so USB
creation and Windows installation can reuse that verification. Close the app
before moving or editing the ISO.
Completed operation receipts are saved in `Omarchy-Setup-Records` beside the
portable executable. If that location is unwritable or on the selected USB,
the app asks for another folder. Protected temporary workspaces are removed
after successful USB operations and verified record export. Unresolved operation
and direct-install recovery files remain under `%ProgramData%\OmarchySetup\Operations`.
Direct installation on Linux and Intel Macs is not implemented.

See the [support matrix](docs/support-matrix.md) for testing status and
[platform behavior](docs/platform-behavior.md) for installation requirements.

## Installation testing status

Testing status on physical hardware, grouped by the operating system running
the app. Implementation availability is listed in the support matrix above.

❌ Untested · ✅ Tested

| Host operating system | USB | No USB (direct installation) |
| --- | --- | --- |
| Windows | ✅ Tested | ❌ Untested |
| macOS — Intel (x86_64) | ❌ Untested | ❌ Untested |
| macOS — Apple Silicon (ARM64) | ❌ Untested | ❌ Untested |
| Linux | ❌ Untested | ❌ Untested |

Windows USB testing covers erase-and-write preparation, full image readback and
EFI boot-file verification on a physical USB drive. Booting and completing an
installation, and the keep-existing-files method, remain untested.

## Future work

The goal is a cross-platform Omarchy installer that runs on Windows, Linux and
macOS. On Apple Silicon Macs, it will reuse the installation backend from
[Omarchy MX Mac](https://github.com/maralcbr/omarchy-mx-mac) through the same
interface. Completing and testing that integration, and adding direct
installation from Linux, are planned next steps.

Other planned improvements:

- **Resize non-native partitions:** investigate resizing ext4/Btrfs partitions
  from Windows or macOS, APFS partitions from Windows or Linux, and other
  combinations of host operating systems and filesystems.
- **Integrate Try Omarchy:** let users try Omarchy in a virtual machine from
  within the app before installing. This is also tracked in the
  [project roadmap](ROADMAP.md).
- **Reduce installer size:** prune unused SDK features and dependencies,
  exclude binaries for other architectures, and bundle only the USB provider
  components the app needs. Consider reimplementing the required parts of
  Etcher SDK in Rust to avoid shipping a separate Node.js runtime and its
  JavaScript dependencies for USB creation.

## Development

Use Rust 1.93, Node.js 22 or 24, pnpm 10.2.1, and the
[Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your platform.
Clone with submodules, or run `git submodule update --init --recursive`.

```sh
pnpm install --frozen-lockfile
npm --prefix providers/media-etcher ci
pnpm providers:stage
pnpm desktop:native
```

Run the main checks with:

```sh
cargo test --workspace --all-features --locked
pnpm desktop:check
pnpm desktop:test
pnpm desktop:test:native
pnpm media:test
```

See [desktop development and packaging](apps/desktop/README.md) for build
instructions, [CONTRIBUTING.md](CONTRIBUTING.md) for contribution guidelines,
and [SECURITY.md](SECURITY.md) for reporting vulnerabilities.

## License

Original project code is available under the [MIT license](LICENSE), the same
license used by [Omarchy](https://github.com/omacom/omarchy).
Dependencies and included third-party components retain their own licenses;
see [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

The Omarchy name and artwork belong to their respective owners. This project
is not an official Omacom Foundation release.
