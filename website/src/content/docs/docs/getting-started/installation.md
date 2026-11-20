---
title: Installation
description: Build and run Agenthub from source.
---

Agenthub is currently installed from source. Packaged release downloads are not yet published.

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

The output is `artifacts/aur/agenthub-<version>-1.src.tar.gz`. It contains the
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
