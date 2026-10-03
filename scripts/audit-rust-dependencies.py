"""Query OSV for registry packages in Cargo.lock (Python 3.11+, no dependencies).

Reports the entire lockfile, including target-specific and build dependencies.
An advisory can be an informational maintenance notice, not a vulnerability.
Use cargo tree --target x86_64-pc-windows-msvc -i NAME to assess Windows reachability.
"""
import concurrent.futures
import json
import pathlib
import tomllib
import urllib.request

root = pathlib.Path(__file__).resolve().parents[1]
lock = tomllib.loads((root / "src-tauri/Cargo.lock").read_text(encoding="utf8"))
packages = [p for p in lock["package"] if p.get("source", "").startswith("registry+")]


def check(batch):
    queries = [
        {"package": {"name": p["name"], "ecosystem": "crates.io"}, "version": p["version"]}
        for p in batch
    ]
    request = urllib.request.Request(
        "https://api.osv.dev/v1/querybatch",
        data=json.dumps({"queries": queries}).encode(),
        headers={"Content-Type": "application/json"},
    )
    with urllib.request.urlopen(request, timeout=45) as response:
        result = json.load(response)
    if len(result["results"]) != len(batch):
        raise RuntimeError("Incomplete OSV response")
    return [
        {"package": p["name"], "version": p["version"], "advisories": item["vulns"]}
        for p, item in zip(batch, result["results"])
        if item.get("vulns")
    ]


batches = [packages[i:i + 100] for i in range(0, len(packages), 100)]
with concurrent.futures.ThreadPoolExecutor(max_workers=5) as executor:
    findings = [f for batch in executor.map(check, batches) for f in batch]
report = {
    "packages_checked": len(packages),
    "findings": findings,
    "local_overrides": [
        {"package": p["name"], "version": p["version"],
         "note": "Local source, excluded from registry scan; inspect vendor/glib/PATCHES.md"}
        for p in lock["package"] if "source" not in p and p["name"] != "babelhack"
    ],
}
output = root / "artifacts/rust-osv-audit.json"
output.parent.mkdir(parents=True, exist_ok=True)
output.write_text(json.dumps(report, indent=2), encoding="utf8")
print(json.dumps(report, indent=2))
