# requestlab-updater

The RequestLab update flow. It is a library, not a program: the code is linked
into `RequestLab.exe`, so a release ships no separate updater executable.

## What it does

- Checks GitHub for a release newer than the installed version.
- Shows the new version, its download size and the release notes link.
- Downloads and verifies the signed package, then applies it.
- With `--pid` it hands the staged update to `requestlab-update-helper`, which
  waits for that RequestLab process to exit before swapping files.

## Usage

Reached through the desktop executable:

```text
requestlab.exe update                    # window, checks on open
requestlab.exe update --check            # exit 0 = update available, 3 = current
requestlab.exe update --silent --pid 1234 --launch
requestlab.exe update --help
```

## Tests

`cargo test -p requestlab-updater` covers argument parsing and installed-version
detection. The window needs a display and is not covered by automated tests.
