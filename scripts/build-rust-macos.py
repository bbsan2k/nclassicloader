#!/usr/bin/env python3
"""Build the self-contained Rust macOS application (no game data bundled)."""
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
from rust_licenses import collect as collect_rust_licenses


def main():
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--native-build-dir", type=Path, default=root / ".build/native")
    parser.add_argument("--output", type=Path, default=root / "dist/Classic Menu.app")
    args = parser.parse_args()
    if sys.platform != "darwin":
        parser.error("Build this bundle on macOS with the native ares/SDL libraries")
    native = args.native_build_dir.resolve()
    if not (native / "portable/core/libclover_ares.a").is_file():
        parser.error("Native libraries missing; first run python3 scripts/build-native.py")
    env = dict(os.environ, CLOVER_NATIVE_BUILD_DIR=str(native))
    subprocess.run(["cargo", "build", "--locked", "--manifest-path", str(root / "rust/Cargo.toml"),
                    "--release", "--features", "native-app"], env=env, check=True)
    output = args.output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    stage = output.with_name(f".{output.stem}.staging-{uuid.uuid4().hex}.app")
    binary_dir = stage / "Contents/MacOS"
    resources = stage / "Contents/Resources"
    binary_dir.mkdir(parents=True)
    resources.mkdir()
    icon = root / ".build/icon/ClassicMenu.icns"
    subprocess.run([sys.executable, str(root / "scripts/generate-app-icon.py"),
                    "--output", str(icon)], check=True)
    shutil.copy2(icon, resources / icon.name)
    binary = binary_dir / "ClassicMenu"
    shutil.copy2(root / "rust/target/release/clover-rust", binary)
    binary.chmod(0o755)
    with (stage / "Contents/Info.plist").open("wb") as stream:
        plistlib.dump({
            "CFBundleExecutable": "ClassicMenu",
            "CFBundleIdentifier": "org.clover-reconstruction.classic-menu",
            "CFBundleName": "Classic Menu",
            "CFBundleDisplayName": "Classic Menu",
            "CFBundlePackageType": "APPL",
            "CFBundleIconFile": "ClassicMenu.icns",
            "CFBundleShortVersionString": "0.1.0",
            "CFBundleVersion": "1",
            "NSHighResolutionCapable": True,
            "LSMinimumSystemVersion": "13.0",
            "NSHumanReadableCopyright": "Clover reconstruction contributors; includes ares and SDL3.",
        }, stream)
    # Reuse the native package's full third-party notices, never its game assets.
    notices = native / "native-package/licenses"
    if notices.is_dir():
        shutil.copytree(notices, resources / "licenses")
    else:
        parser.error("License notices missing; first run python3 scripts/build-native.py")
    collect_rust_licenses(root, resources / "licenses", env)
    shutil.copy2(root / "docs/rust-migration.md", resources / "Rust migration.md")
    (resources / "docs").mkdir()
    for name in ("controller-support.md", "platform-builds.md", "nes-menu-analysis.md"):
        shutil.copy2(root / "docs" / name, resources / "docs" / name)
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
