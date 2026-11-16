# requestlab-manager

Shared installation, update and verification engine used by the RequestLab
desktop installer, the desktop updater, the update helper and the CLI. No front
end contains its own copy of this logic.

- `github`, `releases` — GitHub Releases discovery and deterministic selection.
- `manifest`, `verifier` — signed release manifest, SHA-256 and Ed25519 checks.
- `downloader`, `archive` — resumable downloads and safe zip extraction.
- `installer`, `rollback`, `state`, `paths` — transactional installation,
  metadata, locks, backups and interrupted-update recovery.
- `updater` — checking, staging and handing an update to the helper.
- `platform` — Windows shortcuts, uninstall registration, free space, process
  waiting; explicit "unsupported" elsewhere.
- `events` — the progress vocabulary shared by the GUIs and the CLI.
