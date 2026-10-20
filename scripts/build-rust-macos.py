#!/usr/bin/env python3
"""Build a separate self-contained Rust macOS preview app (no game data bundled)."""
import argparse
from datetime import datetime
import os
import json
from pathlib import Path
import plistlib
import shutil
import subprocess
import sys
import uuid


def main():
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--native-build-dir", type=Path, default=root / ".build/portable")
    parser.add_argument("--output", type=Path, default=root / "dist/Classic Menu Rust.app")
    args = parser.parse_args()
    if sys.platform != "darwin":
        parser.error("Build this bundle on macOS with the native ares/SDL libraries")
    native = args.native_build_dir.resolve()
    if not (native / "portable/core/libclover_ares.a").is_file():
        parser.error("Native libraries missing; first run python3 scripts/build-portable.py")
    env = dict(os.environ, CLOVER_NATIVE_BUILD_DIR=str(native))
    subprocess.run(["cargo", "build", "--manifest-path", str(root / "rust/Cargo.toml"),
                    "--release", "--features", "native-app"], env=env, check=True)
    output = args.output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    stage = output.with_name(f".{output.stem}.staging-{uuid.uuid4().hex}.app")
    binary_dir = stage / "Contents/MacOS"
    resources = stage / "Contents/Resources"
    binary_dir.mkdir(parents=True)
    resources.mkdir()
    binary = binary_dir / "ClassicMenuRust"
    shutil.copy2(root / "rust/target/release/clover-rust", binary)
    binary.chmod(0o755)
    with (stage / "Contents/Info.plist").open("wb") as stream:
        plistlib.dump({
            "CFBundleExecutable": "ClassicMenuRust",
            "CFBundleIdentifier": "org.clover-reconstruction.rust-preview",
            "CFBundleName": "Classic Menu Rust",
            "CFBundleDisplayName": "Classic Menu Rust",
            "CFBundlePackageType": "APPL",
            "CFBundleShortVersionString": "0.1.0",
            "CFBundleVersion": "1",
            "NSHighResolutionCapable": True,
            "LSMinimumSystemVersion": "13.0",
            "NSHumanReadableCopyright": "Clover reconstruction contributors; includes ares and SDL3.",
        }, stream)
    # Reuse the native package's full third-party notices, never its game assets.
    notices = root / "dist/portable/Classic Menu.app/Contents/Resources/licenses"
    if notices.is_dir():
        shutil.copytree(notices, resources / "licenses")
    else:
        parser.error("License notices missing; first run python3 scripts/build-portable.py")
    metadata = json.loads(subprocess.check_output([
        "cargo", "metadata", "--manifest-path", str(root / "rust/Cargo.toml"),
        "--format-version", "1", "--features", "native-app", "--locked",
    ], env=env))
    inventory = []
    for package in metadata["packages"]:
        if not package.get("source"):
            continue
        source = Path(package["manifest_path"]).parent
        destination = resources / "licenses" / "rust" / f'{package["name"]}-{package["version"]}'
        destination.mkdir(parents=True, exist_ok=True)
        for notice in source.iterdir():
            if notice.is_file() and notice.name.upper().startswith(("LICENSE", "LICENCE", "COPYING", "NOTICE")):
                shutil.copy2(notice, destination / notice.name)
        inventory.append({key: package.get(key) for key in ("name", "version", "license", "repository", "source")})
    (resources / "licenses/rust-packages.json").write_text(json.dumps(inventory, indent=2) + "\n")
    shutil.copy2(root / "rust/assets/fonts/OFL.txt", resources / "licenses/Barlow-OFL.txt")
    shutil.copy2(root / "docs/rust-migration.md", resources / "Rust migration.md")
    subprocess.run(["codesign", "--force", "--sign", "-", str(stage)], check=True)
    subprocess.run(["codesign", "--verify", "--strict", str(stage)], check=True)
    if output.exists():
        backup = output.with_name(f"{output.stem}.previous-{datetime.now():%Y%m%d-%H%M%S-%f}.app")
        output.rename(backup)
        print(f"Preserved previous app: {backup}")
    stage.rename(output)
    print(f"Open with: open {str(output)!r}")


if __name__ == "__main__":
    main()
