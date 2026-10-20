#!/usr/bin/env python3
"""Exercise actual SDL audio queue pacing with a supplied game and disposable saves.

The dummy device verifies buffering, not subjective speaker quality or physical
display VSync. No commercial ROM/audio fixtures are bundled in the test.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile


def main():
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--assets", type=Path, required=True)
    parser.add_argument("--executable", type=Path, default=root / "rust/target/release/clover-rust")
    parser.add_argument("--output", type=Path, default=root / ".build/rust-audio-regression")
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="audio-check-", dir=args.output) as temporary:
        data = Path(temporary)
        (data / "preferences.json").write_text(json.dumps({"sound": False, "motion": False}))
        subprocess.run([str(args.executable.resolve()), "run", "--assets", str(args.assets.resolve()),
                        "--data-dir", str(data.resolve()), "--exercise-audio", str(args.output.resolve())],
                       env=dict(os.environ, SDL_VIDEODRIVER="dummy", SDL_AUDIODRIVER="dummy"),
                       check=True, timeout=60)
    report = json.loads((args.output / "audio.json").read_text())
    for phase in report["phases"]:
        assert phase["underruns"] == 0
        print(f"{phase['displayHz']} Hz presentation: {phase['emulationHz']:.2f} Hz emulation, no queue underruns")


if __name__ == "__main__":
    main()
