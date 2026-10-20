#!/usr/bin/env python3
"""Acquire pinned build sources locally; never installs system software."""
import hashlib
import json
import pathlib
import subprocess
import urllib.request

root = pathlib.Path(__file__).resolve().parents[1]
lock = json.loads((root / "dependencies.lock.json").read_text())
for name, spec in lock["repositories"].items():
    directory = root / "third_party" / name
    if not directory.exists():
        directory.parent.mkdir(parents=True, exist_ok=True)
        subprocess.run(["git", "init", str(directory)], check=True)
        subprocess.run(["git", "remote", "add", "origin", spec["url"]], cwd=directory, check=True)
        subprocess.run(["git", "fetch", "--depth", "1", "origin", spec["commit"]], cwd=directory, check=True)
        subprocess.run(["git", "checkout", "--detach", "FETCH_HEAD"], cwd=directory, check=True)
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=directory, text=True).strip()
    if revision != spec["commit"]:
        raise SystemExit(f"{directory} is at {revision}, expected {spec['commit']}; refusing to replace existing work")
    status = subprocess.check_output(["git", "status", "--porcelain"], cwd=directory, text=True)
    if status:
        raise SystemExit(f"{directory} has local changes; review them before building")
    print(f"Verified {name}: {revision}")
vendor = root / "third_party/vendor"
vendor.mkdir(parents=True, exist_ok=True)
for name, spec in lock["files"].items():
    file = vendor / name
    data = file.read_bytes() if file.exists() else urllib.request.urlopen(spec["url"], timeout=60).read()
    if hashlib.sha256(data).hexdigest() != spec["sha256"]:
        raise SystemExit(f"Digest mismatch for {file}; not overwriting it")
    if not file.exists():
        file.write_bytes(data)
    print(f"Verified {name}")
