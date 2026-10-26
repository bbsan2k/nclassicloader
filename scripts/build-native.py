#!/usr/bin/env python3
"""Build the pinned native ares/SDL libraries and run their focused tests."""
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
BUILD = ROOT / ".build/native"


def run(command, *, env=None):
    print("+", " ".join(map(str, command)), flush=True)
    subprocess.run(list(map(str, command)), cwd=ROOT, env=env, check=True)


def main():
    env = os.environ.copy()
    if not (BUILD / "CMakeCache.txt").is_file():
        env.setdefault("CMAKE_GENERATOR", "Ninja")
    run([sys.executable, ROOT / "scripts/bootstrap-portable.py"], env=env)
    run([sys.executable, ROOT / "scripts/test-apple-compat.py"], env=env)
    run(["cmake", "-S", ROOT, "-B", BUILD, "-DCMAKE_BUILD_TYPE=Release",
         "-DCLOVER_BUILD_TESTS=ON", "-DCLOVER_BUILD_APP_SMOKE=ON"], env=env)
    run(["cmake", "--build", BUILD, "--config", "Release", "--parallel", "3"], env=env)
    run(["ctest", "--test-dir", BUILD, "-C", "Release", "--output-on-failure"], env=env)
    run(["cmake", "--install", BUILD, "--config", "Release", "--prefix", BUILD / "native-package"], env=env)


if __name__ == "__main__":
    main()
