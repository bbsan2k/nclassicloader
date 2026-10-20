#!/usr/bin/env python3
"""Build and package the SDL3/ares application without system-wide installs."""
import argparse
from datetime import datetime
import pathlib
import shutil
import subprocess
import sys
import uuid

root = pathlib.Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--build-dir", type=pathlib.Path, default=root / ".build/portable")
parser.add_argument("--output", type=pathlib.Path, default=root / "dist/portable")
parser.add_argument("--filesystem", type=pathlib.Path, default=root / "analysis/eur")
parser.add_argument("--assets", type=pathlib.Path, default=root / ".build/portable-assets")
parser.add_argument("--roms", type=pathlib.Path, help="Optional user-owned compatible .sfc/.smc files named by game code")
parser.add_argument("--without-assets", action="store_true", help="Build code only (the default); import an image at runtime")
parser.add_argument("--legacy-assets", action="store_true", help="Also bundle the old pre-extracted art-only catalog; normally images are imported at runtime")
parser.add_argument("--skip-tests", action="store_true")
parser.add_argument("--jobs", default="4")
args = parser.parse_args()
subprocess.run([sys.executable, str(root / "scripts/bootstrap-portable.py")], check=True)
if args.roms and not args.legacy_assets:
    parser.error("--roms is for the legacy asset bundler; use --legacy-assets or import an image in the app")
if args.legacy_assets and not args.without_assets:
    command = [sys.executable, str(root / "scripts/prepare-portable-assets.py"), "--filesystem", str(args.filesystem), "--output", str(args.assets)]
    if args.roms:
        command += ["--roms", str(args.roms)]
    subprocess.run(command, check=True)
assets = args.assets.resolve() if args.legacy_assets and not args.without_assets else root / ".build/no-assets-selected"
subprocess.run(["cmake", "-S", str(root), "-B", str(args.build_dir), "-DCMAKE_BUILD_TYPE=Release", "-DCLOVER_BUILD_TESTS=ON", f"-DCLOVER_ASSET_DIR={assets}"], check=True)
subprocess.run(["cmake", "--build", str(args.build_dir), "--config", "Release", "--parallel", args.jobs], check=True)
if not args.skip_tests:
    subprocess.run(["ctest", "--test-dir", str(args.build_dir), "-C", "Release", "--output-on-failure"], check=True)
output = args.output.resolve()
output.parent.mkdir(parents=True, exist_ok=True)
stage = output.with_name("." + output.name + ".staging-" + uuid.uuid4().hex)
subprocess.run(["cmake", "--install", str(args.build_dir), "--config", "Release", "--prefix", str(stage)], check=True)
app = stage / "ClassicMenu.app"
if app.exists():
    app = app.rename(stage / "Classic Menu.app")
    subprocess.run(["codesign", "--force", "--sign", "-", str(app)], check=True)
if output.exists():
    backup = output.with_name(output.name + ".previous-" + datetime.now().strftime("%Y%m%d-%H%M%S-%f"))
    output.rename(backup)
    print(f"Preserved previous portable package: {backup}")
stage.rename(output)
print(f"Self-contained runtime package: {output}")
