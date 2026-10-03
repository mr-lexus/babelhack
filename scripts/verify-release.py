"""Fail closed on missing assets; write SHA-256 checksums for the complete matrix."""
import hashlib
import json
from pathlib import Path
import tomllib

root = Path(__file__).resolve().parents[1]
version = json.loads((root / "package.json").read_text())["version"]
assert version == json.loads((root / "src-tauri/tauri.conf.json").read_text())["version"]
assert version == tomllib.loads((root / "src-tauri/Cargo.toml").read_text())["package"]["version"]
directory = root / "artifacts/packages"
expected = set()
for platform, suffixes in {
    "windows-x64": ["-setup.exe", ".exe"],
    "macos-x64": [".dmg", ".app.zip"],
    "macos-arm64": [".dmg", ".app.zip"],
    "linux-x64": [".deb", ".rpm", ".AppImage"],
    "linux-arm64": [".deb", ".rpm", ".AppImage"],
}.items():
    expected.update(f"BabelHack-{version}-{platform}{suffix}" for suffix in suffixes)
actual = {p.name for p in directory.iterdir() if p.name != "SHA256SUMS.txt"}
if actual != expected:
    raise RuntimeError(f"Missing: {expected - actual}; unexpected: {actual - expected}")
checksums = []
for name in sorted(expected):
    path = directory / name
    if path.stat().st_size < 100_000:
        raise RuntimeError(f"Suspiciously small package: {name}")
    with path.open("rb") as handle:
        digest = hashlib.file_digest(handle, "sha256").hexdigest()
    checksums.append(f"{digest}  {name}\n")
(directory / "SHA256SUMS.txt").write_text("".join(checksums), encoding="utf-8")
print(f"Verified {len(expected)} release assets for Babel Hack {version}")
