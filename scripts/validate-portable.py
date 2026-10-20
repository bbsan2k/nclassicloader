#!/usr/bin/env python3
"""Audit and relocate the Rust package, then launch it without checkout paths."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

root = Path(__file__).resolve().parents[1]
default_package = (root / "dist/Classic Menu.app" if sys.platform == "darwin" else
                   root / "dist/rust-desktop/windows" if os.name == "nt" else
                   root / "dist/rust-desktop/linux")
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--package", type=Path, default=default_package)
parser.add_argument("--image", type=Path, help="Optional user-supplied image for offline-cache validation")
args = parser.parse_args()
package = args.package.resolve()
if package.suffix == ".app":
    executable = package / "Contents/MacOS/ClassicMenu"
else:
    executable = package / ("clover-rust.exe" if os.name == "nt" else "clover-rust")
assert executable.is_file(), f"Missing executable: {executable}"
audit = {"executable_sha256": hashlib.sha256(executable.read_bytes()).hexdigest(),
         "system_libraries": [], "relocation": False, "offline_cache": False}
if package.suffix == ".app":
    subprocess.run(["codesign", "--verify", "--deep", "--strict", str(package)], check=True)
    libraries = subprocess.check_output(["otool", "-L", str(executable)], text=True).splitlines()[1:]
    for line in libraries:
        library = line.strip().split(" (", 1)[0]
        if not library.startswith(("/usr/lib/", "/System/Library/")):
            raise SystemExit(f"Non-system runtime dependency: {library}")
        audit["system_libraries"].append(library)
    assert audit["system_libraries"]
for file in package.rglob("*"):
    if file.is_file() and file.suffix.lower() in {".py", ".sh", ".sfrom", ".img", ".hsqs"}:
        raise SystemExit(f"Unexpected runtime payload: {file}")

work = Path(tempfile.mkdtemp(prefix="relocation-", dir=root / ".build"))
relocated = work / package.name
shutil.copytree(package, relocated)
target = relocated / executable.relative_to(package)
environment = {**os.environ, "SDL_VIDEODRIVER": "dummy", "SDL_AUDIODRIVER": "dummy"}
data = work / "data"
if args.image:
    image = work / args.image.name
    shutil.copy2(args.image, image)
    subprocess.run([str(target), "run", "--data-dir", str(data), "--image", str(image),
                    "--frames", "2", "--screenshot", str(work / "menu.png")],
                   cwd=work, env=environment, timeout=300, check=True)
    image.rename(image.with_name(image.name + ".offline"))
    profiles = list((data / "profiles").glob("*/profile.json"))
    if len(profiles) != 1:
        raise SystemExit(f"Expected exactly one imported profile; found {len(profiles)}")
    profile = profiles[0].parent
    subprocess.run([str(target), "run", "--assets", str(profile / "assets"),
                    "--data-dir", str(profile / "data"), "--frames", "2",
                    "--screenshot", str(work / "cached-menu.png")],
                   cwd=work, env=environment, timeout=60, check=True)
    audit["offline_cache"] = True
else:
    subprocess.run([str(target), "run", "--data-dir", str(data), "--launcher-preview",
                    str(work / "launcher.png")], cwd=work, env=environment,
                   timeout=60, check=True)
audit["relocation"] = True
audit["artifacts"] = str(work)
(root / "analysis").mkdir(exist_ok=True)
(root / "analysis/portable-package-audit.json").write_text(json.dumps(audit, indent=2))
print(json.dumps(audit, indent=2))
