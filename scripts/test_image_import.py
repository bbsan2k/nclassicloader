#!/usr/bin/env python3
"""End-to-end image import tests. --run-games runs three no-extra-firmware regressions."""
import argparse
import json
import os
import pathlib
import subprocess
import tempfile

root = pathlib.Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--executable", type=pathlib.Path, default=root / ".build/portable/ClassicMenu.app/Contents/MacOS/ClassicMenu")
parser.add_argument("--image", type=pathlib.Path, default=root / "SNES-Mini-Kernels/dp-sneseur-release-v2.0.14-0-gd8b65c6.hsqs")
parser.add_argument("--run-games", action="store_true")
args = parser.parse_args()
work = pathlib.Path(tempfile.mkdtemp(prefix="image-integration-", dir=root / ".build"))
environment = {**os.environ, "SDL_VIDEODRIVER": "dummy", "SDL_AUDIODRIVER": "dummy"}
base = [str(args.executable.resolve()), "--data-dir", str(work / "data")]


def run(options, success=True):
    result = subprocess.run(base + options, cwd=work, env=environment, text=True, capture_output=True, timeout=60)
    if (result.returncode == 0) != success:
        raise AssertionError(result.stdout + result.stderr)
    return result.stdout + result.stderr


run(["--image", str(args.image.resolve()), "--smoke-test", "--screenshot", str(work / "menu.png")])
profiles = list((work / "data/profiles").glob("*/profile.json"))
assert len(profiles) == 1
profile = profiles[0]
manifest = json.loads(profile.read_text())
catalog = json.loads((profile.parent / "assets/catalog.json").read_text())
regressions = {"CLV-P-SADGE", "CLV-P-SADJE", "CLV-P-SADKE"} if manifest["ui"] != "shvc" else {"CLV-P-VADGJ", "CLV-P-VADJJ", "CLV-P-VADKJ"}
assert len(catalog["games"]) == 21
assert sum("rom" in game for game in catalog["games"]) == 21
assert sum("importError" in game for game in catalog["games"]) == 0
assert manifest["playableGames"] == 21
restored = [item for item in manifest["conversions"] if item["status"] == "converted-restored-pcm"]
assert len(restored) >= 17
assert all(item["metadata"]["pcm_signal_exact"] for item in restored)
assert (profile.parent / "assets/firmware/ipl.rom").stat().st_size == 64
before = profile.stat().st_mtime_ns
assert "Using verified cache" in run(["--image", str(args.image.resolve()), "--smoke-test"])
assert profile.stat().st_mtime_ns == before
run(["--smoke-test", "--screenshot", str(work / "cached-menu.png")])
run(["--render-launcher", str(work / "launcher.png")])
error = run(["--smoke-test", "--game", "CLV-P-INVALID", "--start"], success=False)
assert "not in profile" in error
kart = "CLV-P-VAAFJ" if manifest["ui"] == "shvc" else "CLV-P-SAAFE"
error = run(["--smoke-test", "--game", kart, "--start"], success=False)
assert "dsp1b.upd7725.program.rom" in error and "Required user firmware" in error
if args.run_games:
    for code in sorted(regressions):
        output = run(["--smoke-test", "--game", code, "--start", "--frames", "600", "--screenshot", str(work / (code + ".png"))])
        assert "embedded ares=running" in output
summary = {"catalog": 21, "converted": 21, "blocked": 0, "restored_pcm": len(restored), "cache_reused": True, "launcher_rendered": True,
           "startup_frames_per_supported_game": 600 if args.run_games else 0, "artifacts": str(work)}
(root / "analysis/image-import-verification.json").write_text(json.dumps(summary, indent=2))
print(json.dumps(summary, indent=2))
