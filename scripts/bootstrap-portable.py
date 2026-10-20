#!/usr/bin/env python3
"""Acquire pinned build sources locally; never installs system software."""
import json
import pathlib
import subprocess

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
