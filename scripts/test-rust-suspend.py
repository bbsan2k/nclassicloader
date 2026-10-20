#!/usr/bin/env python3
"""Exercise real ares suspend saves/screenshots and keyboard/gamepad navigation."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile


def main():
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--assets", type=Path, required=True, help="Imported profile with a launchable default game")
    parser.add_argument("--executable", type=Path, default=root / "rust/target/debug/clover-rust")
    parser.add_argument("--output", type=Path, default=root / ".build/rust-suspend-regression")
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ, SDL_VIDEODRIVER="dummy", SDL_AUDIODRIVER="dummy")
    with tempfile.TemporaryDirectory(prefix="suspend-check-", dir=args.output) as temporary:
        data = Path(temporary)
        (data / "preferences.json").write_text(json.dumps({"motion": False, "sound": False}))
        command = [str(args.executable.resolve()), "run", "--assets", str(args.assets.resolve()), "--data-dir", str(data)]
        subprocess.run(command + ["--exercise-ui", str(args.output.resolve())], env=env, check=True, timeout=120)
        preferences = json.loads((data / "preferences.json").read_text())
        if not any(locks.get("0") for locks in preferences.get("suspendLocks", {}).values()):
            raise RuntimeError("Lock preference did not survive saving")
        subprocess.run(command + ["--frames", "2", "--preview-panel", "resume", "--screenshot",
                                  str(args.output / "picker-after-restart.png")], env=env, check=True)
        result = json.loads((args.output / "result.json").read_text())
        if not all(result.values()):
            raise RuntimeError(f"Suspend regression failed: {result}")
    print("Suspend screenshots, picker, lock/overwrite/delete, replay, restart, toolbar and gamepad navigation passed")


if __name__ == "__main__":
    main()
