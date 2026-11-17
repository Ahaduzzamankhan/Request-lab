# requestlab-installer

The RequestLab setup program.

## What it does

- Finds the newest release on the stable channel through the GitHub API.
- Verifies the release manifest against the trusted Ed25519 keys before any
  download starts, then verifies the package's size and SHA-256.
- Unpacks and installs transactionally through `requestlab-manager`: a failure
  restores the previous installation.
- Creates desktop and start menu shortcuts and an Add/Remove Programs entry.
- Can start RequestLab when it finishes.

## Usage

```text
requestlab-installer                    # window
requestlab-installer --silent           # unattended, console output
requestlab-installer --silent --install-dir "C:\Apps\RequestLab" --launch
requestlab-installer --help
```

The window performs exactly the same steps and shows the engine's own progress
events. Without a trusted signing key the installer stops with a clear message
instead of installing an unverified build.

## Tests

`cargo test -p requestlab-installer` covers argument parsing and the suggested
install location. The window itself needs a display and is not covered by
automated tests.
