# Building RequestLab

## Prerequisites

| Requirement | Notes |
|---|---|
| Rust stable | `rustup toolchain install stable`; the workspace uses edition 2021 |
| C linker | MSVC on Windows; `build-essential`/`clang` on Linux |
| Windows | Visual Studio Build Tools with the **Desktop development with C++** workload |

On Linux, GPUI links against system libraries. Development packages are needed
to link binaries (not for `cargo check`):

```bash
sudo apt-get install libxcb1-dev libxkbcommon-dev libxkbcommon-x11-dev
```

## Common commands

```bash
# Type-check everything (no linking; works on any platform)
cargo check --workspace

# Formatting and lints (CI runs both)
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings

# Tests
cargo test --workspace

# Desktop application
cargo run -p requestlab-app

# CLI
cargo run -p requestlab-cli -- --help
```

The tests that link GPUI (`requestlab-ui`, `requestlab-app`) require the Linux
libraries above. On a machine without them, run the non-GUI suites with:

```bash
cargo test -p requestlab-models -p requestlab-http -p requestlab-storage \
           -p requestlab-collections -p requestlab-environments \
           -p requestlab-core -p requestlab-cli
```

## Windows release build

```powershell
cargo build --release -p requestlab-app -p requestlab-cli
```

Outputs:

- `target\release\requestlab.exe` - GUI (release builds use the `windows`
  subsystem, so no console window appears).
- `target\release\requestlab-cli.exe` - CLI.

The release profile uses `opt-level = 3`, thin LTO, one codegen unit and symbol
stripping (see the root `Cargo.toml`).

### Expected binary size

Release binaries are typically 15-30 MB because GPUI and Rustls are linked
statically. The exact size is recorded in the release report produced by CI;
do not assume a number that has not been measured.

## Versioning

`version.json` is authoritative:

```json
{
  "name": "RequestLab",
  "version": "0.1.2",
  "channel": "stable",
  "platforms": ["windows-x64"],
  "artifacts": { "gui": "requestlab.exe", "cli": "requestlab-cli.exe" }
}
```

`[workspace.package].version` in `Cargo.toml` must match it. To bump:

1. Edit `version.json`.
2. Edit `version` in the `[workspace.package]` section of `Cargo.toml`.
3. Run `cargo update --workspace --offline` (or `cargo check`) so `Cargo.lock`
   records the new version.
4. Run `scripts/verify-version.sh` (used by CI as well).
5. Commit: `chore: release vX.Y.Z`.

## Release signing

The application refuses unsigned update manifests, so a release needs an
Ed25519 key pair:

```bash
# once, on a trusted machine
cargo run -p requestlab-update-helper -- generate-key --out keys \
    --trust-dir "%APPDATA%\RequestLab\trusted-keys"
```

`keys/signing-key.txt` is the secret seed. Store its contents (and nothing else)
in the repository secret `REQUESTLAB_SIGNING_KEY`. `keys/public-key.txt` is the
public half; the workflow derives it from the seed and compiles it into the
application, so released builds trust exactly that key. Losing the seed means
shipping a new key in a new build; leaking it means anyone can sign an update.

## Release process

`.github/workflows/release-windows.yml` runs on a `v*` tag or manual dispatch:

1. Validates `version.json` and that it matches `Cargo.toml`.
2. Refuses to continue if the tag already exists, or if
   `REQUESTLAB_SIGNING_KEY` is not configured.
3. Runs formatting, workspace clippy with `-D warnings`, and the test suite.
4. Builds the update helper first, derives the public key, then builds
   `requestlab.exe`, `requestlab-cli.exe`, `requestlab-installer.exe`,
   `requestlab-updater.exe` and `requestlab-update-helper.exe` with that key
   compiled in.
5. Verifies every executable answers `--version`.
6. Packages `requestlab-windows-x86_64.zip` (all five executables plus README,
   LICENSE and version.json) and writes `SHA256SUMS.txt`.
7. Builds `manifest.json` for the zip, signs it to `manifest.json.sig`, and adds
   both checksums.
8. Uploads a build artifact, creates the tag (on manual dispatch) and publishes
   the GitHub Release with the binaries, the zip, the manifest, its signature and
   the checksums.

Required permissions are `contents: write`; no personal token is used - the
workflow relies on `GITHUB_TOKEN`.

## Troubleshooting

| Symptom | Cause / fix |
|---|---|
| `unable to find library -lxcb` | Linux link step only; install the X11/xkbcommon dev packages |
| The Windows icon is missing from the exe | `build.rs` needs a resource compiler (`rc.exe` with MSVC); the build still succeeds without one and logs a warning |
| `ely-gpui-component` fails to resolve | It is a git dependency that must match `gpui-pre` 0.3.8 exactly; run `cargo update -p ely-gpui-component` |
| `database is locked` | Another RequestLab instance is running; close it (WAL allows one writer) |
| App starts but shows an empty sidebar | New data directory; create a collection or use the CLI |
| Corrupted database | The file is moved to `requestlab.sqlite.corrupt-<timestamp>` and a fresh one is created |
