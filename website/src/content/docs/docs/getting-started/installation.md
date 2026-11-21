---
title: Installation
description: Build and run Agenthub from source.
---

Publishing a manually created GitHub Release automatically builds and attaches
installers for Windows, macOS, and Debian/Ubuntu, plus an Arch source package.
The workflow does not create releases or change their notes or status. Check
[GitHub Releases](https://github.com/merlos/agenthub/releases) for available
downloads; source builds remain available when no release has been published.

## Release downloads

Choose the installer matching your operating system and CPU:

- Windows: `.msi` for x64 or ARM64
- macOS: `.dmg` for Intel or Apple Silicon
- Debian/Ubuntu: `.deb` for amd64 or ARM64, built on Ubuntu 22.04
- Arch Linux: `agenthub-<version>-1-arch.src.tar.gz` to build locally with `makepkg -si`; no AUR publication

Windows installers may download WebView2 if it is not installed. Debian packages
require WebKitGTK 4.1, GTK 3, libsecret, and D-Bus; install using
`sudo apt install ./agenthub-<version>-debian-<arch>.deb` to resolve dependencies.
Mount the macOS DMG and copy Agenthub to Applications.

Each package includes a matching `<filename>.sha256` file. Verify it with
`sha256sum --check <filename>.sha256`. Platform signing/notarization and
GPG signatures for individual checksum files are optional, and the attached `SIGNING.txt` states
whether they were enabled. Unsigned Windows installers may show security warnings or be
blocked by Smart App Control; ad-hoc-signed macOS apps may require approval in
Privacy & Security. Review the
[release guide](https://github.com/merlos/agenthub/blob/main/HOW-TO-RELEASE.md)
for verification and signing details.

## Requirements

- Node.js 20 or newer
- Rust stable
- Git
- The [Tauri 2 system dependencies](https://v2.tauri.app/start/prerequisites/) for your operating system

## Build and run

```bash
git clone https://github.com/merlos/agenthub.git
cd agenthub
npm install
npm run tauri dev
```

This starts the React frontend through Vite and opens it in the native Tauri window.

To create an optimized application bundle:

```bash
npm run tauri build
```

Tauri writes platform-specific artifacts under `src-tauri/target/release/bundle/`.

## Arch Linux source package

Arch packaging currently creates a local source package only; Agenthub is not
published to AUR. On Arch Linux with `makepkg` installed, run from the repository
root:

```bash
npm run package:source
npm run package:aur
```

The local output is `artifacts/aur/agenthub-<version>-1.src.tar.gz`; the release
pipeline renames it to `agenthub-<version>-1-arch.src.tar.gz`. It contains the
PKGBUILD, `.SRCINFO`, and application sources, without compiling a binary or
requiring a published GitHub release. The source archive is created from
committed `HEAD`, so commit packaging changes before generating it.

Extract this package, enter the extracted `agenthub/` directory, and run
`makepkg -si` to build and install. Downloading npm and Cargo dependencies
still requires network access.

:::note
The browser-only demo (`npm run demo`) contains fake, in-memory data for screenshots. It does not start proxy listeners or access your provider credentials.
:::

## Verify the source tree

```bash
npm test
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
```

Continue by configuring your [first provider](./first-provider/).
