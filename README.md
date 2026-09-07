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
Direct installation on Linux and Intel Macs is not implemented.

See the [support matrix](docs/support-matrix.md) for testing status and
[platform behavior](docs/platform-behavior.md) for installation requirements.

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
