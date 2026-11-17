# requestlab-updater

The RequestLab update program.

## What it does

- Checks GitHub for a release newer than the installed version.
- Shows the new version, its download size and the release notes link.
- Downloads and verifies the signed package, then applies it.
- With `--pid` it hands the staged update to `requestlab-update-helper`, which
  waits for that RequestLab process to exit before swapping files.

## Usage

```text
requestlab-updater                       # window, checks on open
requestlab-updater --check               # exit 0 = update available, 3 = current
requestlab-updater --silent --pid 1234 --launch
requestlab-updater --help
```

## Tests

`cargo test -p requestlab-updater` covers argument parsing and installed-version
detection. The window needs a display and is not covered by automated tests.
