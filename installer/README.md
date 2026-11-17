# Windows installer

RequestLab ships on Windows through **Inno Setup 6**. This directory holds the
only installer definition; there is no Rust installer program anymore.

| File | Purpose |
|---|---|
| `requestlab.iss` | The installer script: files, shortcuts, uninstall entry, metadata. |
| `../assets/branding/requestlab.ico` | Application and installer icon. |

## What it installs

- `RequestLab.exe` — desktop application. It also dispatches the CLI and owns the
  updater (`RequestLab.exe update`), so there is no separate updater binary.
- `requestlab-cli.exe` — standalone command line interface.
- `LICENSE` and `version.json`.

Start Menu entries are always created; the desktop shortcut is an opt-in task
that is offered once. Per-user installation is the default, with the standard
Inno privileges dialog for a machine-wide install.

## What it never touches

User data lives in `%APPDATA%\RequestLab` (database, collections, environments,
history, settings). The installer does not read or write that directory, so
installing, upgrading and uninstalling all preserve it. The uninstaller removes
only the files it placed in the install directory.

## Building locally

Install Inno Setup 6 from <https://jrsoftware.org/isinfo.php>, build and stage the
binaries, then compile the script:

```powershell
cargo build --release -p requestlab-app -p requestlab-cli
New-Item -ItemType Directory -Force dist | Out-Null
Copy-Item target\release\requestlab.exe, target\release\requestlab-cli.exe dist\
Copy-Item LICENSE, version.json dist\
& "C:\Program Files (x86)\Inno Setup 6\ISCC.exe" `
  "/DAppVersion=0.1.2" "/DSourceDir=$PWD\dist" "installer\requestlab.iss"
```

The result is `installer/Output/RequestLab-Setup-0.1.2-x64.exe`.

`release-windows.yml` runs exactly these steps, from the version in
`version.json`, and publishes the setup executable together with the standalone
CLI, the signed update manifest and `SHA256SUMS.txt`.

## Silent use

```powershell
RequestLab-Setup-0.1.2-x64.exe /VERYSILENT /SUPPRESSMSGBOXES /NORESTART
"%LOCALAPPDATA%\Programs\RequestLab\unins000.exe" /VERYSILENT
```

`/DIR="..."` selects the install directory and `/ALLUSERS` requests a
machine-wide installation.

## Icon

`SetupIconFile` points at `assets/branding/requestlab.ico`, and the installed
`RequestLab.exe` carries its own icon from the same source.
