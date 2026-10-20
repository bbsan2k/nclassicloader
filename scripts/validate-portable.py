#!/usr/bin/env python3
"""Audit and relocate the built package, then run it without source paths."""
import argparse
import hashlib
import json
import os
import pathlib
import shutil
import subprocess
import tempfile

root = pathlib.Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--package", type=pathlib.Path, default=root / "dist/portable")
parser.add_argument("--test-rom", type=pathlib.Path, help="Optional self-authored/homebrew compatible ROM for integrated video/audio smoke test")
parser.add_argument("--image", type=pathlib.Path, help="Local .hsqs or .tar.gz filesystem to import in the relocated package")
args = parser.parse_args()
package = args.package.resolve()
apps = list(package.glob("*.app"))
exe = apps[0] / "Contents/MacOS/ClassicMenu" if apps else package / ("ClassicMenu.exe" if os.name == "nt" else "ClassicMenu")
assert exe.is_file(), f"Missing executable: {exe}"
audit = {"executable_sha256": hashlib.sha256(exe.read_bytes()).hexdigest(), "system_libraries": [], "relocation": False, "emulation": False}
if apps:
    subprocess.run(["codesign", "--verify", "--deep", "--strict", str(apps[0])], check=True)
    libraries = subprocess.check_output(["otool", "-L", str(exe)], text=True).splitlines()[1:]
    for line in libraries:
        library = line.strip().split(" (", 1)[0]
        if not library.startswith(("/usr/lib/", "/System/Library/")):
            raise SystemExit(f"Non-system runtime dependency: {library}")
        audit["system_libraries"].append(library)
    assert audit["system_libraries"]
for file in package.rglob("*"):
    if file.is_file() and file.suffix.lower() in [".py", ".sh", ".sfrom", ".img", ".hsqs"]:
        raise SystemExit(f"Unexpected runtime payload: {file}")
# Preserve audit artifacts, including the independent relocated package and data.
work = pathlib.Path(tempfile.mkdtemp(prefix="relocation-", dir=root / ".build"))
relocated = work / "package"
shutil.copytree(package, relocated)
target = relocated / exe.relative_to(package)
environment = {**os.environ, "SDL_VIDEODRIVER": "dummy", "SDL_AUDIODRIVER": "dummy"}
command = [str(target), "--data-dir", str(work / "data"), "--smoke-test", "--screenshot", str(work / "menu.png")]
catalogs = list(relocated.rglob("catalog.json"))
has_assets = bool(catalogs)
if args.image:
    image = work / args.image.name
    shutil.copy2(args.image, image)
    command += ["--image", str(image)]
elif not has_assets:
    command = [str(target), "--data-dir", str(work / "data"), "--render-launcher", str(work / "launcher.png")]
else:
    command += ["--assets", str(catalogs[0].parent)]
subprocess.run(command, cwd=work, env=environment, timeout=30, check=True)
audit["relocation"] = True
if args.image:
    image.rename(image.with_name(image.name + ".offline"))
    subprocess.run([str(target), "--data-dir", str(work / "data"), "--smoke-test", "--screenshot", str(work / "cached-menu.png")], cwd=work, env=environment, timeout=30, check=True)
    audit["source_independent_cache"] = True
if args.test_rom:
    if not args.image and not has_assets:
        raise SystemExit("--test-rom requires --image or bundled legacy assets")
    command = [str(target), "--data-dir", str(work / ("data" if args.image else "game-data")), "--smoke-test", "--frames", "12", "--rom", str(args.test_rom.resolve()), "--screenshot", str(work / "game.png")]
    if not args.image:
        command += ["--assets", str(catalogs[0].parent)]
    subprocess.run(command, cwd=work, env=environment, timeout=30, check=True)
    audit["emulation"] = True
audit["artifacts"] = str(work)
(root / "analysis").mkdir(exist_ok=True)
(root / "analysis/portable-package-audit.json").write_text(json.dumps(audit, indent=2))
print(json.dumps(audit, indent=2))
