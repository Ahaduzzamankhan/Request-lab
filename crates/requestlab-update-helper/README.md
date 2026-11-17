# requestlab-update-helper

The self-update helper, and the release tooling that signs what it installs.

## Applying a staged update

```text
requestlab-update-helper --install-dir "C:\Apps\RequestLab" --pid 1234 --launch
```

It waits for the given process to exit, recovers an interrupted earlier attempt,
applies the staged update transactionally and writes
`<install-dir>\work\update-helper.log` so a failed update can be diagnosed even if
no window appears afterwards.

## Release tooling

```text
requestlab-update-helper generate-key --out keys --trust-dir "%APPDATA%\RequestLab\trusted-keys"
requestlab-update-helper build-manifest --dist dist --version-for 0.1.2 --notes-url URL --out dist/manifest.json
requestlab-update-helper sign-manifest --manifest dist/manifest.json --out dist/manifest.json.sig
```

The signing seed comes from `--key-file` or `REQUESTLAB_SIGNING_KEY`; it is never
printed. `generate-key` writes the secret with owner-only permissions on Unix and
copies the public half into the trust directory so this machine accepts the
releases it signs. `build-manifest` describes `dist/requestlab-windows-x86_64.zip`
exactly as the engine validates it.

## Tests

`cargo test -p requestlab-update-helper` covers argument parsing, refusal paths
(missing installation, no staged record, mismatched record), manifest generation
and a sign/verify round trip with a freshly generated key.
