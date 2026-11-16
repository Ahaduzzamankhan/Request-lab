# RequestLab checkpoint

**Date:** 2026-11-16
**Branch:** `main`
**Version in the tree:** 0.1.1

This file is an honest snapshot of the installer / updater / CLI work: what is
implemented and verified, what is only scaffolding, and what is still missing.
No item below is marked done unless the code exists and its checks were run.

---

## 1. What is done

### 1.1 `requestlab-manager` — the shared engine (implemented, tested)

A new workspace crate that owns every piece of installation and update logic.
The desktop installer, desktop updater, update helper and CLI are all meant to
call into it rather than re-implement anything.

| Module | Responsibility | State |
|---|---|---|
| `version.rs` | SemVer parse/order, prerelease precedence, `v`-prefix tags | done, tested |
| `manifest.rs` | Signed release manifest schema: app id, version, channel, OS/arch package, HTTPS URL, size, SHA-256, `min_updater_version`, notes URL; structural validation | done, tested |
| `verifier.rs` | SHA-256 (streamed), constant-time digest compare, Ed25519 trust store (build-time key, `REQUESTLAB_MANIFEST_PUBLIC_KEY`, `<data dir>/trusted-keys/*.pub`), signing, key generation from the OS RNG | done, tested |
| `github.rs` | GitHub Releases client: configurable API base, token from `REQUESTLAB_GITHUB_TOKEN`/`GITHUB_TOKEN`, user agent, typed rate-limit error with reset time, 404 → `ReleaseNotFound`, bounded metadata downloads | done, tested |
| `releases.rs` | Release/asset models, deterministic latest-release selection per channel, draft/prerelease filtering, exact asset-name matching | done, tested |
| `downloader.rs` | Streaming download to `.part`, HTTP range resume, incremental hashing, progress + speed, cancellation, size ceiling, checksum/size enforcement, corrupt-file cleanup, HTTPS-only (loopback exempt for tests) | done, tested (local HTTP server) |
| `archive.rs` | Zip extraction with entry-name validation, path traversal refusal, symlink refusal, entry count/size caps, truncation detection, single-root-package detection | done, tested |
| `state.rs` | `install.json` metadata (atomic write, per-file size+SHA-256), installation lock (`create_new`, stale-lock reclaim), `manager.json` settings (channel, auto-check interval, skipped version) | done, tested |
| `rollback.rs` | Backup of the installed version, restore, atomic `replace_file`, apply marker, interrupted-update recovery (healthy → clean, broken → restore) | done, tested |
| `installer.rs` | Transactional install: stage-beside-destination → verify copies → back up → swap → record metadata; disk-space check; repair/verify; uninstall (keeps user data unless purged); existing-installation detection | done, tested |
| `updater.rs` | Update check (manifest download + signature verify + tag/version cross-check + asset URL check + `min_updater_version`), staging (download → verify → extract → record), in-process apply, helper launch, check-interval bookkeeping | done, unit-tested (no live GitHub test) |
| `platform.rs` | Free space, desktop/Start-menu shortcuts, HKCU uninstall entry, process wait, detached launch | implemented; **Windows branches not executed here** |
| `paths.rs`, `events.rs`, `lib.rs` | Directory layout, containment checks, progress vocabulary, typed user-facing error enum | done, tested |

### 1.2 Verification actually performed

- `cargo test -p requestlab-manager` → **57 passed, 0 failed** (unit + local-HTTP
  integration tests for download, cancellation, checksum rejection, archive
  safety, install/reinstall/rollback/recovery, lock, metadata, config, signing).
- `cargo check --workspace --all-targets` → succeeds.
- `cargo fmt --all` applied.
- Not run here: any Windows-only code path (`winreg`, `windows-sys` free space,
  `WScript.Shell` shortcuts, `OpenProcess` waiting), and there is no display, so
  no GUI has been launched.

### 1.3 Crates added (scaffolding only, deliberately not filled in yet)

`crates/requestlab-installer`, `crates/requestlab-updater`,
`crates/requestlab-update-helper`, `crates/requestlab-manager-ui` exist as
workspace members with placeholder `main`/`lib` bodies so the workspace stays
buildable. They currently produce "unused dependency" warnings, which disappear
once they are implemented.

---

## 2. What is NOT done

1. **CLI management commands** — `install`, `update`, `update --check`,
   `releases`, `version` (installed), `verify`, `repair`, `rollback`,
   `uninstall`, release-manifest generation/signing for CI. The existing
   `requestlab-cli` still only has its original HTTP/collections/history/
   environments commands.
2. **GPUI installer** — no UI, no flow (welcome → directory/disk/options →
   download/verify/extract/install → finish/launch).
3. **GPUI updater** — no UI (installed version, latest version, channel,
   release notes, progress, cancel, apply, restart).
4. **Shared installer/updater widgets** — `requestlab-manager-ui` is empty.
5. **Windows update helper** — `requestlab-update-helper` prints nothing; it must
   wait for the app pid, recover interrupted state, apply the staged update under
   the lock, launch the new build, and roll back on failure.
6. **Self-update wiring in the app** — the desktop app does not yet check for
   updates, launch the updater, or run recovery at startup.
7. **GitHub Actions** — `release-windows.yml` still publishes bare `.exe` files +
   `SHA256SUMS.txt`; it does not build the installer/updater/helper, produce
   `requestlab-windows-x86_64.zip`, generate `requestlab-manifest.json`, sign it,
   or publish the new assets. `test.yml` does not build the new binaries.
8. **Documentation** — installation, CLI usage, updater usage, asset naming,
   manifest specification, signing/trust setup, self-update architecture,
   troubleshooting. Only the engine README exists.
9. **Integration tests that were planned** — mock-GitHub release-endpoint test,
   end-to-end CLI install/update test, optional live-release test.
10. **The UI bug list from the previous request** — POST/PUT/PATCH/DELETE/HEAD/
    OPTIONS selector, 2xx/3xx/4xx/5xx status colouring, settings buttons,
    response-panel scrolling, visible text caret, renaming, right-click context
    menu, animations. **None of these were changed.**

---

## 3. Blockers and environment limits

- **No Windows host and no display in this runner.** Windows registry,
  shortcut, free-space and process-wait code compiles under `cfg(windows)` but
  has never executed. Every GUI is unbuilt and unseen.
- **No signing key is configured.** Verification therefore fails closed
  (`no trusted signing key`). A maintainer must generate a key with
  `requestlab-cli manifest keygen` (not yet implemented), store the seed as the
  `REQUESTLAB_MANIFEST_SIGNING_KEY` repository secret, and either build with
  `REQUESTLAB_MANIFEST_PUBLIC_KEY` set or install the public key under
  `<data dir>/trusted-keys/*.pub`. Nothing fake is shipped: unsigned manifests
  are rejected.
- **No GitHub token** in the runner, so live API checks were not run (unit tests
  use local HTTP servers and fixture JSON).

---

## 4. Next steps, in order

1. `requestlab-update-helper`: implement the full helper and test its
   wait → recover → apply → launch → rollback sequence locally with a fake
   installation directory.
2. CLI `management` module + dispatch, reusing the engine only: `install`,
   `update`, `update --check`, `releases`, `verify`, `repair`, `rollback`,
   `uninstall`, `manifest build|sign|verify|keygen`, with `--dir`, `--channel`,
   `--version`, `--quiet`, `--json`, `--yes` and documented exit codes.
3. `requestlab-manager-ui`: progress bar, version badge, download status,
   release-notes panel, path selector, error panel, confirmation dialog on the
   existing RequestLab design tokens.
4. GPUI installer and updater apps using those widgets and `tokio` tasks; keep
   the engine on a background executor and the UI responsive.
5. Wire update checking/recovery into `requestlab-app` (config interval, no
   silent installs).
6. Rework `release-windows.yml`: build all binaries, package
   `requestlab-windows-x86_64.zip`, generate + sign the manifest, upload with the
   manifest and signature, keep the existing bare-exe assets for compatibility;
   extend `test.yml` to build/test the new crates and run the engine tests.
7. Docs: `docs/INSTALL.md`, `docs/UPDATER.md`, `docs/CLI.md`,
   `docs/RELEASE_MANIFEST.md`, update `README.md`, `docs/BUILD.md`,
   `docs/ARCHITECTURE.md`.
8. Then the UI bug list from the previous message.

---

## 5. Completeness

Weighted by the effort each deliverable carries in the original request:

| Deliverable | Weight | Done | Weighted |
|---|---|---|---|
| Shared `manager-core` engine | 30% | 95% (Windows branches unexecuted) | 28.5% |
| Windows self-update helper | 8% | 5% (stub) | 0.4% |
| CLI installer + updater | 12% | 0% | 0% |
| GPUI installer | 15% | 0% (scaffold) | 0% |
| GPUI updater | 15% | 0% (scaffold) | 0% |
| GitHub Actions release pipeline | 8% | 0% | 0% |
| Documentation | 5% | 10% (engine README only) | 0.5% |
| Shared GUI widgets | 4% | 0% | 0% |
| App integration (auto-check, recovery) | 3% | 0% | 0% |

**Requested installer/updater/CLI system: ≈ 30% complete.**

Counting the earlier, still-unfixed UI bug list (method selector, status
colours, settings buttons, scrolling, caret, rename, context menu, animation) as
part of the same "finish the product" effort, the **whole project is ≈ 25%
complete**.

The foundation is real and tested — the release format, trust model, download,
verification, extraction, transactional install, backup and recovery all work —
but nothing user-facing beyond the CLI's existing commands has been built on top
of it yet.
