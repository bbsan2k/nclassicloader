#!/usr/bin/env python3
"""Import/render each supplied region/version; verify GUI resources match that image."""
import argparse
import concurrent.futures
import hashlib
import json
import os
import pathlib
import subprocess
import tarfile
import tempfile

root = pathlib.Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--executable", type=pathlib.Path, default=root / ".build/portable/ClassicMenu.app/Contents/MacOS/ClassicMenu")
parser.add_argument("--images", type=pathlib.Path, default=root / "SNES-Mini-Kernels")
parser.add_argument("--audit", type=pathlib.Path, help="Optional clover_game_audit executable to test every converted ROM's PCM/video")
parser.add_argument("--frames", type=int, default=600)
parser.add_argument("--jobs", type=int, default=2)
parser.add_argument("--work-dir", type=pathlib.Path, help="Resume this audit-owned artifact directory; existing completed reports are reused")
args = parser.parse_args()
work = args.work_dir.resolve() if args.work_dir else pathlib.Path(tempfile.mkdtemp(prefix="regions-integration-", dir=root / ".build"))
work.mkdir(parents=True, exist_ok=True)
environment = {**os.environ, "SDL_VIDEODRIVER": "dummy", "SDL_AUDIODRIVER": "dummy"}
summary = []
audits = []
for archive in sorted(args.images.glob("*.tar.gz")):
    name = archive.name[:-7]
    data = work / name
    command = [str(args.executable.resolve()), "--data-dir", str(data), "--image", str(archive.resolve()), "--smoke-test", "--screenshot", str(work / (name + ".png"))]
    result = subprocess.run(command, env=environment, text=True, capture_output=True, timeout=300)
    (work / (name + ".log")).write_text(result.stdout + result.stderr)
    assert result.returncode == 0, result.stdout + result.stderr
    profiles = list((data / "profiles").glob("*/profile.json"))
    assert len(profiles) == 1
    profile = profiles[0].parent
    manifest = json.loads(profiles[0].read_text())
    catalog = json.loads((profile / "assets/catalog.json").read_text())
    theme = json.loads((profile / "assets/skin/theme.json").read_text())
    strings = json.loads((profile / "assets/skin/strings.json").read_text())
    assert len(catalog["games"]) == 21
    assert catalog["region"] == manifest["region"] == theme["region"]
    assert catalog["locale"] == strings["locale"] == theme["locale"]
    ui = manifest["ui"]
    with tarfile.open(archive) as source:
        atlas = source.extractfile(f"usr/share/ui/{ui}/resources/sprites/packed.png").read()
        assert atlas == (profile / "assets/skin/packed.png").read_bytes()
        table = json.load(source.extractfile(f"usr/share/ui/{ui}/resources/strings/{strings['locale']}/strings.lng"))
        assert strings["strings"] == table["strings"]
    converted = [game for game in catalog["games"] if "rom" in game]
    assert len(converted) == manifest["playableGames"]
    if ui == "shvc":
        assert strings["locale"] == "jpn_ja"
        assert strings["strings"]["sys_gametitlelist_hud_Menu"] == "メニュー"
        assert any(any(ord(c) > 127 for c in game["title"]) for game in catalog["games"])
    if args.audit:
        for game in converted:
            folder = data / "audits" / game["id"]
            audits.append((game, profile, folder))
    summary.append({"image": name, "region": manifest["region"], "locale": strings["locale"], "converted": len(converted),
                    "blocked": 21 - len(converted), "atlas_sha256": hashlib.sha256(atlas).hexdigest(), "profile": str(profile)})
    print(f"{name}: {manifest['region']}, {strings['locale']}, {len(converted)} converted; original GUI resources verified", flush=True)
assert len(summary) == 6
assert len({row["atlas_sha256"] for row in summary}) >= 3


def audit(item):
    game, profile, folder = item
    folder.mkdir(parents=True, exist_ok=True)
    command = [str(args.audit.resolve()), str(profile / "assets" / game["rom"]), str(folder / "saves"), str(profile / "assets/firmware"),
               str(args.frames), str(folder / "report.json"), "--press-start", "300", "--press-b", "420"]
    report_path = folder / "report.json"
    report = {}
    if args.work_dir and report_path.exists():
        try:
            previous = json.loads(report_path.read_text())
            if previous.get("completed_frames") == args.frames or previous.get("errors"):
                report = previous
        except (ValueError, OSError):
            pass
    if not report:
        result = subprocess.run(command, text=True, capture_output=True, timeout=300)
        (folder / "audit.log").write_text(result.stdout + result.stderr)
        report = json.loads(report_path.read_text()) if report_path.exists() else {}
    # FFIII starts music late; an injected early Start can also skip Contra's
    # intro music. Keep the original result and test a clean, uninterrupted intro
    # instead of treating that early window as proof of broken restoration.
    quiet_intro = (not report.get("passed") and not report.get("errors") and report.get("completed_frames") == args.frames
                   and args.frames < 1800 and report.get("audio", {}).get("peak", 0) <= 1e-5
                   and report.get("video", {}).get("nonblack_frames", 0) > 0)
    if quiet_intro:
        extended_path = folder / "extended-report.json"
        extended = {}
        if args.work_dir and extended_path.exists():
            try:
                previous = json.loads(extended_path.read_text())
                if previous.get("completed_frames") == 1800 or previous.get("errors"):
                    extended = previous
            except (ValueError, OSError):
                pass
        if not extended:
            extended_command = [str(args.audit.resolve()), str(profile / "assets" / game["rom"]), str(folder / "extended-saves"),
                                str(profile / "assets/firmware"), "1800", str(extended_path)]
            result = subprocess.run(extended_command, text=True, capture_output=True, timeout=300)
            (folder / "extended-audit.log").write_text(result.stdout + result.stderr)
            extended = json.loads(extended_path.read_text()) if extended_path.exists() else {}
        extended["initial_quiet_audit"] = str(report_path)
        report = extended
    expected_firmware = any("dsp1b.upd7725." in error for error in report.get("errors", []))
    status = "passed" if report.get("passed") else "requires-user-dsp-firmware" if expected_firmware else "failed-audit"
    return {"id": game["id"], "title": game["title"], "passed": report.get("passed", False), "status": status, "report": report, "artifacts": str(folder)}


if audits:
    with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as executor:
        results = list(executor.map(audit, audits))
else:
    results = []
output = {"regions": summary, "game_audits": results, "artifacts": str(work)}
(root / "analysis/regions-verification.json").write_text(json.dumps(output, ensure_ascii=False, indent=2))
print(f"Regional GUI checks passed. Audio/video audits: {sum(r['passed'] for r in results)}/{len(results)}. Artifacts: {work}")
unexpected = [result for result in results if result["status"] == "failed-audit"]
if unexpected:
    raise SystemExit(f"{len(unexpected)} audio/video audits need investigation; see analysis/regions-verification.json")
