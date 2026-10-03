"""Collect every required native package under unambiguous release filenames."""
import json
from pathlib import Path
import shutil
import subprocess
import sys

target, platform = sys.argv[1:]
root = Path(__file__).resolve().parents[1]
version = json.loads((root / "package.json").read_text())["version"]
release = root / "src-tauri/target" / target / "release"
output = root / "artifacts/packages"
output.mkdir(parents=True, exist_ok=True)
prefix = f"BabelHack-{version}-{platform}"


def copy_one(pattern, suffix):
    candidates = list(release.glob(pattern))
    if len(candidates) != 1:
        raise RuntimeError(f"Expected exactly one {pattern}, found {candidates}")
    shutil.copy2(candidates[0], output / (prefix + suffix))


if platform.startswith("windows"):
    copy_one("bundle/nsis/*.exe", "-setup.exe")
    copy_one("babelhack.exe", ".exe")
elif platform.startswith("macos"):
    copy_one("bundle/dmg/*.dmg", ".dmg")
    app = release / "bundle/macos/Babel Hack.app"
    subprocess.run(["codesign", "--verify", "--deep", "--strict", str(app)], check=True)
    arch = "arm64" if platform.endswith("arm64") else "x86_64"
    subprocess.run(["lipo", str(app / "Contents/MacOS/babelhack"), "-verify_arch", arch], check=True)
    subprocess.run(["ditto", "-c", "-k", "--sequesterRsrc", "--keepParent", str(app), str(output / (prefix + ".app.zip"))], check=True)
elif platform.startswith("linux"):
    for kind, extension in [("deb", "deb"), ("rpm", "rpm"), ("appimage", "AppImage")]:
        copy_one(f"bundle/{kind}/*.{extension}", f".{extension}")
else:
    raise ValueError(platform)
print("Collected", prefix)
