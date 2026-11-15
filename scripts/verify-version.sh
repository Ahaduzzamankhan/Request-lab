#!/usr/bin/env bash
# Validate version.json and confirm the workspace version matches it.
#
# Used locally and by CI; exits non-zero with a readable message on any
# mismatch so a release can never be tagged with an inconsistent version.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

if [[ ! -f version.json ]]; then
  echo "error: version.json is missing" >&2
  exit 1
fi

python3 - <<'PY'
import json
import re
import sys
from pathlib import Path

version_file = Path("version.json")
data = json.loads(version_file.read_text())

required = {"name", "version", "channel"}
missing = required - data.keys()
if missing:
    sys.exit(f"error: version.json is missing keys: {sorted(missing)}")

version = data["version"]
if not re.fullmatch(r"\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?", version):
    sys.exit(f"error: '{version}' is not a semantic version")

if data["channel"] not in {"stable", "beta", "nightly"}:
    sys.exit(f"error: unknown channel '{data['channel']}'")

if not data.get("name"):
    sys.exit("error: version.json has an empty name")

cargo = Path("Cargo.toml").read_text()
section = re.search(r"^\[workspace\.package\](.*?)(?=^\[)", cargo, re.S | re.M)
if not section:
    sys.exit("error: [workspace.package] not found in Cargo.toml")

match = re.search(r'^version\s*=\s*"([^"]+)"', section.group(1), re.M)
if not match:
    sys.exit("error: [workspace.package].version not found")

cargo_version = match.group(1)
if cargo_version != version:
    sys.exit(
        f"error: version mismatch: version.json has {version}, "
        f"Cargo.toml [workspace.package].version has {cargo_version}"
    )

# Every crate must inherit the workspace version instead of pinning its own.
for manifest in sorted(Path("crates").glob("*/Cargo.toml")):
    text = manifest.read_text()
    if re.search(r'^version\s*=\s*"', text, re.M):
        sys.exit(f"error: {manifest} pins its own version; use version.workspace = true")

print(f"ok: version {version} ({data['channel']}) is consistent across the workspace")
PY
