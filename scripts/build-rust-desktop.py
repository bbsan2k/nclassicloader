#!/usr/bin/env python3
"""Build and package the default Rust desktop application."""
import os
from pathlib import Path
import shutil
import subprocess
import sys
from rust_licenses import collect as collect_rust_licenses

ROOT = Path(__file__).resolve().parents[1]
BUILD = ROOT / ".build/native"
DIST = ROOT / "dist/rust-desktop"


def run(command, *, env=None):
    print("+", " ".join(map(str, command)), flush=True)
    subprocess.run(list(map(str, command)), cwd=ROOT, env=env, check=True)


def main():
    env = os.environ.copy()
    # CI selects Ninja. Existing local CMake trees keep their generator.
    if not (BUILD / "CMakeCache.txt").is_file():
        env.setdefault("CMAKE_GENERATOR", "Ninja")
    if sys.platform == "win32":
        # Match the static MSVC runtime used by native libraries and Rust.
        env["RUSTFLAGS"] = (env.get("RUSTFLAGS", "") + " -C target-feature=+crt-static").strip()
    run([sys.executable, ROOT / "scripts/build-native.py"], env=env)
    if sys.platform == "darwin":
        run([sys.executable, ROOT / "scripts/build-rust-macos.py", "--native-build-dir", BUILD])
        DIST.mkdir(parents=True, exist_ok=True)
        run(["ditto", "-c", "-k", "--sequesterRsrc", "--keepParent",
             ROOT / "dist/Classic Menu.app", DIST / "ClassicMenu-macOS.zip"])
        return

    app_env = env.copy()
    app_env["CLOVER_NATIVE_BUILD_DIR"] = str(BUILD.resolve())
    run(["cargo", "build", "--locked", "--manifest-path", ROOT / "rust/Cargo.toml",
         "--release", "--features", "native-app"], env=app_env)

    smoke_env = app_env.copy()
    smoke_env.update({"SDL_VIDEODRIVER": "dummy", "SDL_AUDIODRIVER": "dummy",
                      "CLOVER_RUST_WINDOW_FRAMES": "2"})
    executable = ROOT / "rust/target/release" / ("clover-rust.exe" if sys.platform == "win32" else "clover-rust")
    run([executable, "window-smoke"], env=smoke_env)
    run([executable, "run", "--launcher-preview", BUILD / "launcher-smoke.png",
         "--data-dir", BUILD / "launcher-smoke-data"], env=smoke_env)

    package = DIST / ("windows" if sys.platform == "win32" else "linux")
    package.mkdir(parents=True, exist_ok=True)
    shutil.copy2(executable, package / executable.name)
    # Package native and Rust notices only; image content is imported locally.
    licenses = BUILD / "native-package/licenses"
    if not licenses.is_dir():
        raise SystemExit("Third-party license directory missing from portable package")
    shutil.copytree(licenses, package / "licenses", dirs_exist_ok=True)
    collect_rust_licenses(ROOT, package / "licenses", app_env)
    shutil.copy2(ROOT / "docs/rust-migration.md", package / "Rust migration.md")
    (package / "docs").mkdir(exist_ok=True)
    for name in ("controller-support.md", "platform-builds.md", "nes-menu-analysis.md"):
        shutil.copy2(ROOT / "docs" / name, package / "docs" / name)
    shutil.make_archive(str(DIST / f"ClassicMenu-Rust-{package.name}"),
                        "zip" if sys.platform == "win32" else "gztar",
                        root_dir=package.parent, base_dir=package.name)
    print(f"Rust desktop package: {package}")


if __name__ == "__main__":
    main()
