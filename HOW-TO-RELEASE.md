# How to release Agenthub

Publishing a GitHub Release for a tag such as `v0.1.0` starts
[`.github/workflows/release.yml`](.github/workflows/release.yml). After validation
and every package build succeeds, it attaches artifacts to that existing
release. You create the release and tag manually; the workflow never creates
or publishes a release or changes its title, notes, or draft/prerelease status.
Pushing a tag alone does not start packaging. No AUR account or AUR SSH key
is required.

## Artifacts

For `vX.Y.Z`, the release contains:

| Platform | Artifact |
| --- | --- |
| Windows x64 | `agenthub-X.Y.Z-windows-x86_64.msi` |
| Windows ARM64 | `agenthub-X.Y.Z-windows-aarch64.msi` |
| macOS Intel | `agenthub-X.Y.Z-macos-x86_64.dmg` |
| macOS Apple Silicon | `agenthub-X.Y.Z-macos-aarch64.dmg` |
| Debian/Ubuntu amd64 | `agenthub-X.Y.Z-debian-amd64.deb` |
| Debian/Ubuntu ARM64 | `agenthub-X.Y.Z-debian-arm64.deb` |
| Application sources | `agenthub-X.Y.Z-source.tar.gz` |
| Arch source package | `agenthub-X.Y.Z-1.src.tar.gz` |

`SHA256SUMS` covers all eight packages. `SIGNING.txt` records their signing modes.
Optional GPG signing adds `SHA256SUMS.asc`
and `release-signing-key.asc`. Arch remains **source-only**, not an Arch binary
package and not published to AUR. The `1` is Arch's packaging revision (`pkgrel`).
The Debian binaries are built on Ubuntu 22.04, and require the WebKitGTK 4.1,
GTK 3, libsecret, and D-Bus packages declared in their package dependencies.
This does not guarantee compatibility with every Debian-derived distribution.

## One-time GitHub configuration

1. Enable GitHub Actions and allow the actions used in the workflow:
   `actions/checkout`, `actions/setup-node`, `actions/upload-artifact`,
   `actions/download-artifact`, and `dtolnay/rust-toolchain`.
2. Allow workflows to write repository contents. The workflow gives only the
   publication job `contents: write`; builds have read-only access.
   `GITHUB_TOKEN` is supplied automatically. Do **not** create a personal token
   secret for releases.
3. Ensure your repository/plan supports the runners in the workflow, especially
   `windows-11-arm`, `macos-15-intel`, and `ubuntu-22.04-arm`. Runner labels can
   change or be retired; update the matrix before releasing when necessary.
4. Restrict who can create `v*` tags with a GitHub repository ruleset.
   Tagged code and dependency build scripts run with signing credentials when
   signing is enabled, so release tags must point to reviewed, trusted commits.
5. Configure signing as described below, or leave it disabled for initial
   unsigned releases.
6. Keep release immutability disabled for this upload-after-publication flow:
   the workflow must be able to add assets and replace same-name assets on retries.

Add secrets under **Settings → Secrets and variables → Actions → Secrets →
New repository secret**. Add variables on the adjacent **Variables** tab.
Use exact names and set enable variables to the lowercase string `true`.
An unset enable variable disables that signing method; incomplete credentials
with signing enabled fail the release instead of silently producing unsigned
packages.

Never commit certificates, private keys, certificate passwords, or Apple
credentials. Use an encrypted backup and rotate credentials if exposed.

## macOS signing and notarization

Requires a paid Apple Developer membership and a **Developer ID Application**
certificate (not an Apple Distribution or Developer ID Installer certificate).

1. Create the certificate in your Apple Developer account and install it in
   macOS Keychain Access alongside its private key.
2. Export the certificate **with its private key** as a password-protected
   `.p12` from Keychain Access → My Certificates.
3. Encode it as a single-line base64 string:

   ```bash
   openssl base64 -A -in certificate.p12 -out certificate-base64.txt
   ```

4. Create these repository secrets:

   | Secret | Value |
   | --- | --- |
   | `APPLE_CERTIFICATE` | Contents of `certificate-base64.txt` |
   | `APPLE_CERTIFICATE_PASSWORD` | `.p12` export password |
   | `APPLE_SIGNING_IDENTITY` | Full identity, e.g. `Developer ID Application: Example Name (TEAMID)` |
   | `APPLE_ID` | Apple account email used for notarization |
   | `APPLE_PASSWORD` | Apple **app-specific password**, not your normal account password |
   | `APPLE_TEAM_ID` | Apple Developer team ID |

   Obtain the identity with `security find-identity -v -p codesigning`. Create
   the app-specific password at [account.apple.com](https://account.apple.com/).
   Accept any pending Apple Developer agreements first.
5. Set repository variable `MACOS_SIGNING_ENABLED` to `true`.

Tauri imports the certificate into the ephemeral runner, signs, submits for
notarization, waits for Apple's result, and staples the ticket. The workflow
verifies the app signature and stapled ticket before uploading the DMG.
The initial notarization can take a long time; do not bypass failures or use
`--skip-stapling` for a public signed release.

Without this variable, macOS bundles receive an **ad-hoc** signature (`-`).
This is not Developer ID signing/notarization, and users may need to approve
the application in Privacy & Security. The attached `SIGNING.txt` discloses
the signing mode; mention it in your manually written release notes as well.

See [Tauri's macOS signing guide](https://v2.tauri.app/distribute/sign/macos/).

## Windows Authenticode signing

The implemented workflow supports an **exportable PFX code-signing certificate
with its private key**. This is suitable for supported legacy/software-held
certificates or internal certificates; it is not a solution for hardware-bound
EV certificates or most newly issued public-trust certificates. Modern issuers
typically require hardware or a cloud signing service. If your issuer requires
one, integrate its tooling with Tauri's `bundle.windows.signCommand` before
enabling signing; uploading an ordinary PFX will not work.

For a supported PFX:

1. Obtain a code-signing certificate, including its private key and chain, and
   export it to a password-protected `.pfx`. An HTTPS/TLS certificate cannot be
   used for Authenticode.
2. Encode it in PowerShell:

   ```powershell
   [Convert]::ToBase64String([IO.File]::ReadAllBytes("certificate.pfx")) |
     Set-Content -NoNewline certificate-base64.txt
   ```

3. Add repository secrets:

   | Secret | Value |
   | --- | --- |
   | `WINDOWS_CERTIFICATE` | Raw base64 contents of `certificate-base64.txt` (no PEM headers) |
   | `WINDOWS_CERTIFICATE_PASSWORD` | PFX export password |

4. Add repository variable `WINDOWS_TIMESTAMP_URL` with your issuer's
   **RFC 3161** timestamp service URL, e.g. `http://timestamp.digicert.com`.
5. Set repository variable `WINDOWS_SIGNING_ENABLED` to `true`.

The workflow imports the certificate into `Cert:\CurrentUser\My`, passes its
thumbprint and SHA-256/RFC 3161 settings to Tauri, and signs the executable and
MSI. It verifies the MSI's signature and signer thumbprint, then removes the
certificate from the runner. A private CA must be trusted by the runner for
verification to pass, and by users for the signature to be trusted.

Without signing, SmartScreen may warn and Smart App Control may block the
application. Signing does not guarantee immediate SmartScreen reputation.
See [Tauri's Windows signing guide](https://v2.tauri.app/distribute/sign/windows/)
for cloud/hardware alternatives.

## Optional GPG signature for all release assets

This signs the checksum manifest, covering Windows/macOS installers, Debian
packages, and both source archives. It does not embed a Debian package signature
or replace platform code signing. No apt repository or automatic updater is
configured.

1. Create a passphrase-protected GPG signing key (or a dedicated release signing
   subkey) in a secure environment:

   ```bash
   gpg --full-generate-key
   gpg --list-secret-keys --keyid-format long --with-subkey-fingerprint
   gpg --armor --export-secret-keys YOUR_PRIMARY_FINGERPRINT > release-private-key.asc
   gpg --armor --export YOUR_PRIMARY_FINGERPRINT > release-public-key.asc
   ```

   Prefer a dedicated release key; export only signing subkeys if your policy
   keeps the primary key offline. Test that the exported material can sign.
2. Add secrets `RELEASE_GPG_PRIVATE_KEY` (the ASCII-armored private key text,
   **not** base64) and `RELEASE_GPG_PASSPHRASE`.
3. Add variable `RELEASE_GPG_FINGERPRINT` containing the primary key fingerprint,
   and variable `RELEASE_GPG_SIGNING_ENABLED` set to `true`.
4. Publish the fingerprint/public key through an independently trusted channel.
   A key downloaded beside an artifact alone does not establish trust.

The workflow uses a temporary GPG home, signs `SHA256SUMS`, verifies the
signature, and exports the public key. Windows/macOS signing is independent.
Tauri updater signing secrets are not needed because Agenthub has no updater.

## Prepare and create a release

1. Update the application version in:
   - `package.json`
   - `src-tauri/tauri.conf.json`
   - `[package].version` in `src-tauri/Cargo.toml`
2. Refresh both lockfiles:

   ```bash
   npm install --package-lock-only
   cargo check --manifest-path src-tauri/Cargo.toml
   ```

   Confirm the root version in `package-lock.json` and the `agenthub` package
   entry in `src-tauri/Cargo.lock` match. Do not bump `website/package.json` for
   an application release.
3. Update `APP.md` and the website docs for behavior/support changes, then run:

   ```bash
   npm test
   npm run build
   npm run website:build
   cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
   cargo test --manifest-path src-tauri/Cargo.toml --locked
   cargo clippy --manifest-path src-tauri/Cargo.toml --locked --all-targets -- -D warnings
   RELEASE_TAG=v0.1.0 node scripts/check-release.mjs
   ```

   Replace `v0.1.0` with your release version. The last command uses POSIX shell
   syntax; on PowerShell set `$env:RELEASE_TAG = "v0.1.0"` before running Node.
4. Commit and push all changes, and wait for Quality CI to pass. Ensure the
   release workflow exists **in the tagged commit**.
5. In GitHub, open **Releases → Draft a new release**. Choose a new `vX.Y.Z` tag,
   select the reviewed commit, enter the title and release notes, then click
   **Publish release**. GitHub can create the tag together with the release.
   This starts the artifact workflow.

   Alternatively, create and push the tag locally first:

   ```bash
   git tag -a v0.1.0 -m "Agenthub v0.1.0"
   git push origin v0.1.0
   ```

   Prefer a GPG-signed Git tag (`git tag -s`) if configured. Tags must be exactly
   `vX.Y.Z`, without leading zeros, prerelease suffixes, or build metadata.
    Tags pushed using another workflow's `GITHUB_TOKEN` do not trigger this
    workflow by themselves. After pushing, manually publish a GitHub Release
    for that tag. Publishing releases using `GITHUB_TOKEN` also does not trigger
    another workflow; use a human action or suitable GitHub App/token.
6. Watch **Actions → Release**. It validates versions and runs the checks again,
   builds native x64/ARM64 packages, and assembles all eight assets before
    upload. Arch source packaging runs as a non-root user.
7. Open **Releases**, review your notes and the attached `SIGNING.txt`,
   download the matching installers, verify checksums/signatures, and
   smoke-test installation, launch, and gateway requests on each platform.

If a check or build fails, no artifacts are uploaded; your existing release
remains unchanged. Successful individual builds are available as Actions
artifacts for 14 days. The GitHub Release assets are separate from these
temporary artifacts.

### Existing releases and manual retries

For an already-created release, open **Actions → Release → Run workflow**,
choose the branch with the updated workflow (normally `main`), and enter its
existing tag in the `tag` field. The workflow checks out and builds that tag,
not the selected branch. The manual trigger must exist on the default branch.

You can also run it from GitHub CLI:

```bash
gh workflow run release.yml --ref main -f tag=v0.1.0
```

The target release must already exist. Uploads use `--clobber`, so retries
replace only assets with the same filenames; unrelated assets and manually
written release metadata remain untouched. Do not change a published tag's
source to repair a release.

## Verify downloads

Download all eight assets and `SHA256SUMS` into the same directory:

```bash
sha256sum --check SHA256SUMS
```

On macOS use `shasum -a 256 -c SHA256SUMS`; for one Windows artifact use
`Get-FileHash -Algorithm SHA256 file.msi` and compare with its manifest entry.
Checksums detect corruption; unsigned checksums do not authenticate a publisher.

If GPG signing is enabled, first compare the key fingerprint with the trusted
published fingerprint, then:

```bash
gpg --show-keys --with-fingerprint release-signing-key.asc
gpg --import release-signing-key.asc
gpg --verify SHA256SUMS.asc SHA256SUMS
sha256sum --check SHA256SUMS
```

For signed Windows installers, check `Get-AuthenticodeSignature file.msi`.
For signed macOS apps, after mounting/copying the DMG, use
`codesign --verify --deep --strict /Applications/agenthub.app` and
`spctl --assess --type execute --verbose /Applications/agenthub.app`.

## Failures and reruns

- **Version mismatch:** fix the manifests/lockfiles before tagging. Do not move
  a published tag; create a new patch version for source changes.
- **Missing/expired signing credentials:** update the corresponding secrets
  and rerun the failed jobs for the same tag if its source is unchanged.
- **Runner/dependency/notarization failure:** inspect the failing job. Retry
  transient failures; source/workflow fixes require a new tag.
- **Git dubious ownership in the Arch container:** the source job explicitly
  trusts `$GITHUB_WORKSPACE` for container Git operations before archiving.
  Keep that step after checkout; do not use a wildcard `safe.directory=*`.
- **Upload interrupted:** some files may already be attached. Rerun the upload
  job or manually dispatch the workflow for the same tag; same-name files are
  replaced. Verify all eight packages against the final `SHA256SUMS`.
- **Release missing:** create it manually for the exact tag, then retry. The
  workflow never creates it for you.
- **Workflow fix for an existing tag:** merge the fix onto the default branch
  and use the manual trigger above. An old run's rerun uses its old workflow.
- **Immutable release:** asset uploads/replacements are blocked by GitHub.
  This workflow requires mutable releases; do not delete a published release
  just to retry an upload.
