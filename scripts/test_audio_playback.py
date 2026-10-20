#!/usr/bin/env python3
"""Exercise Rust SDL playback and gapless menu-loop refill without audible output."""
import argparse
import json
import math
import os
import pathlib
import shutil
import struct
import subprocess
import tempfile
import wave

root = pathlib.Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--assets", type=pathlib.Path, required=True, help="An imported profile's assets directory (read-only)")
parser.add_argument("--executable", type=pathlib.Path, default=root / "rust/target/release/clover-rust")
args = parser.parse_args()
work = pathlib.Path(tempfile.mkdtemp(prefix="music-loop-", dir=root / ".build"))
assets = work / "assets"
assets.mkdir()
for directory in ["skin", "games"]:
    shutil.copytree(args.assets / directory, assets / directory)
shutil.copy2(args.assets / "catalog.json", assets / "catalog.json")
# Replace only our test copy's BGM with a self-authored one-second sine loop.
rate = 44100
signal = b"".join(struct.pack("<hh", *(2 * [int(16000 * math.sin(2 * math.pi * 440 * i / rate))])) for i in range(rate))
with wave.open(str(assets / "skin/bgm_home.wav"), "wb") as output:
    output.setnchannels(2)
    output.setsampwidth(2)
    output.setframerate(rate)
    output.writeframes(signal)
data = work / "data"
data.mkdir()
(data / "preferences.json").write_text(json.dumps({"schema": 1, "sound": True, "motion": False}))
raw = work / "playback.raw"
environment = {**os.environ, "SDL_VIDEODRIVER": "dummy", "SDL_AUDIODRIVER": "disk", "SDL_AUDIO_DISK_OUTPUT_FILE": str(raw)}
result = subprocess.run([str(args.executable.resolve()), "run", "--assets", str(assets), "--data-dir", str(data), "--frames", "240"],
                        env=environment, text=True, capture_output=True, timeout=30)
(work / "playback.log").write_text(result.stdout + result.stderr)
assert result.returncode == 0, result.stdout + result.stderr
samples = [left for left, _ in struct.iter_unpack("<hh", raw.read_bytes())]
first = next(i for i, sample in enumerate(samples) if abs(sample) > 100)
checked = samples[first + 2048:-2048]
assert len(checked) >= 3 * rate, "Capture too short to exercise loop boundaries"
run = longest = 0
for sample in checked:
    run = run + 1 if abs(sample) <= 1 else 0
    longest = max(longest, run)
assert longest < 40, f"Audio gap: {longest} consecutive silent samples"
report = {"test": "self-authored one-second menu music loop through SDL disk output", "rate": rate,
          "checked_samples": len(checked), "longest_silence_samples": longest, "passed": True, "artifacts": str(work)}
(root / "analysis/menu-audio-loop-verification.json").write_text(json.dumps(report, indent=2))
print(json.dumps(report, indent=2))
