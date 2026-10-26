#!/usr/bin/env python3
"""Build and run generated-ROM checks inside an owned iOS/tvOS simulator app."""

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import time
import uuid
import http.client as http_client
import hashlib


ROOT = Path(__file__).resolve().parents[1]

NATIVE_NOTICES = ("project-LICENSE.txt", "THIRD-PARTY-NOTICES.md", "ares-core-LICENSE.txt",
                  "ares-LICENSE.txt", "SDL-LICENSE.txt", "SDL-HIDAPI-LICENSE.txt", "SDL-yuv2rgb-LICENSE.txt")


def validate_bundle_notices(app, rust_preview=True):
    """Verify built bundle contents, not just the CMake notice source directory."""
    # CMake maps Resources/licenses to the root resource directory on iOS/tvOS.
    notices = app / "licenses"
    required = list(NATIVE_NOTICES)
    if rust_preview:
        required += ["rust-packages.json", "Barlow-OFL.txt", "flag-icons-MIT.txt", "flag-icons-attribution.md"]
    for name in required:
        if not (notices / name).is_file():
            raise RuntimeError(f"Bundle is missing a required dependency notice: {name}")
    if rust_preview:
        inventory = json.loads((notices / "rust-packages.json").read_text())
        if not isinstance(inventory, list) or not inventory:
            raise RuntimeError("Bundle Rust notice inventory is empty or invalid")
        for package in inventory:
            if not isinstance(package, dict):
                raise RuntimeError("Invalid bundled Rust package notice entry")
            name, version, files = package.get("name"), package.get("version"), package.get("notices")
            if not all(isinstance(value, str) and value and "/" not in value and "\\" not in value and value not in (".", "..")
                       for value in (name, version)) or not isinstance(files, list) or not files:
                raise RuntimeError("Invalid bundled Rust package notice entry")
            directory = notices / "rust" / f"{name}-{version}"
            for filename in files:
                if not isinstance(filename, str) or Path(filename).name != filename or filename in (".", "..") or "\\" in filename:
                    raise RuntimeError("Invalid bundled Rust notice filename")
                if not (directory / filename).is_file():
                    raise RuntimeError(f"Bundle is missing a Rust dependency notice: {name}/{filename}")


def run(args, **kwargs):
    kwargs.setdefault("timeout", 600)
    return subprocess.run(args, check=True, text=True, **kwargs)


def sim_json(*args):
    return json.loads(run(["xcrun", "simctl", *args, "--json"], capture_output=True).stdout)


def open_interactive_simulator(device):
    developer = Path(run(["xcode-select", "-p"], capture_output=True).stdout.strip())
    simulator = developer / "Applications/Simulator.app"
    hub = developer.parent / "Applications/DeviceHub.app"
    if simulator.is_dir():
        command = ["open", "-a", str(simulator), "--args", "-CurrentDeviceUDID", device]
    elif hub.is_dir():
        command = ["open", "-a", str(hub), f"devices://device/open?id={device}"]
    else:
        command = ["open", "-a", "Simulator", "--args", "-CurrentDeviceUDID", device]
    try:
        result = subprocess.run(command,
                                capture_output=True, text=True, timeout=15, check=False)
        if result.returncode == 0:
            print(f"Simulator GUI opened for {device}")
            return True
    except subprocess.TimeoutExpired:
        pass
    print("Simulator/Device Hub GUI unavailable: the app is running headlessly. Xcode 27 uses Device Hub; older Xcode uses Simulator.app.")
    print(f"Device Hub: open -a DeviceHub 'devices://device/open?id={device}'")
    return False


def select_runtime(platform):
    prefix = "com.apple.CoreSimulator.SimRuntime." + ("iOS-" if platform == "ios" else "tvOS-")
    runtimes = [runtime for runtime in sim_json("list", "runtimes")["runtimes"]
                if runtime["identifier"].startswith(prefix) and runtime.get("isAvailable")]
    if not runtimes:
        raise RuntimeError(f"No {platform} simulator runtime installed; install it through Xcode first")
    family = "iPhone" if platform == "ios" else "Apple TV"
    available_types = sim_json("list", "devicetypes")["devicetypes"]
    candidates = []
    for runtime in runtimes:
        supported = {item["identifier"] for item in runtime.get("supportedDeviceTypes", [])}
        types = [item for item in available_types if item["name"].startswith(family)
                 and (not supported or item["identifier"] in supported)]
        for item in types:
            # A newer runtime can support only a multi-display device. Prefer an
            # older runtime with a conventional phone for deterministic screenshots.
            rank = 0 if "Duo" in item["name"] or "Fold" in item["name"] else 1
            if " Pro" in item["name"] and "Max" not in item["name"]:
                rank = 2
            candidates.append((rank, tuple(int(n) for n in runtime["version"].split(".")),
                               runtime["identifier"], item["identifier"]))
    if not candidates:
        raise RuntimeError(f"No compatible {family} device type in installed runtimes")
    _, _, runtime, device_type = max(candidates)
    return runtime, device_type


def valid_report(report, platform, app, run_id):
    if not isinstance(report, dict):
        return False
    if not (type(report.get("schema")) is int and report["schema"] == 1
            and report.get("platform") == platform and report.get("run_id") == run_id
            and report.get("passed") is True and report.get("error") == ""):
        return False
    if app == "core":
        checks = report.get("checks")
        return (isinstance(checks, list) and len(checks) == 2
                and all(isinstance(check, dict) and check.get("passed") is True
                        and check.get("error") == "" and check.get("system") in ("nes", "snes") for check in checks)
                and {check.get("system") for check in checks} == {"nes", "snes"})
    checksum = report.get("checksum")
    video_valid = (app in ("preview", "game", "input", "touch") and type(report.get("frames")) is int and report["frames"] >= 3
            and isinstance(checksum, str) and len(checksum) <= 20 and checksum.isascii() and checksum.isdecimal()
            and 0 < int(checksum) <= 2**64 - 1)
    if app not in ("game", "input", "touch"):
        return video_valid
    game = report.get("game")
    if not video_valid or not isinstance(game, dict):
        return False
    integers = ["emulated_frames", "video_changes", "audio_samples", "max_audio_queue_bytes",
                "audio_queue_bytes", "audio_limit_bytes", "audio_drained_bytes", "max_steps_per_callback", "pauses"]
    if any(type(game.get(key)) is not int or game[key] < 0 for key in integers):
        return False
    policy_passed = (report.get("lifecycle_probe_passed") is True if app == "game" else
                     report.get("input_probe_passed") is True and type(report.get("input_checks")) is int
                      and report["input_checks"] == 12 and game.get("buttons") == 256)
    if app == "touch":
        policy_passed = (report.get("touch_probe_passed") is True and type(report.get("touch_checks")) is int
                         and report["touch_checks"] == 17 and type(game.get("buttons")) is int and game["buttons"] == 1
                         and report.get("touch_ui_supported") is (platform == "ios"))
    return (policy_passed and game["emulated_frames"] >= 60
            and game["video_changes"] >= 2 and game["audio_samples"] > 0
            and game.get("audio_nonzero") is True and game.get("audio_started") is True
            and game["audio_drained_bytes"] > 0
            and 0 < game["max_audio_queue_bytes"] <= game["audio_limit_bytes"] == 38400
            and game["audio_queue_bytes"] <= game["audio_limit_bytes"]
            and 1 <= game["max_steps_per_callback"] <= 3 and game["pauses"] >= 1)


def valid_lifecycle_snapshot(snapshot, platform, run_id, phase):
    if not isinstance(snapshot, dict) or not isinstance(snapshot.get("game"), dict):
        return False
    if not (type(snapshot.get("schema")) is int and snapshot["schema"] == 1
            and snapshot.get("platform") == platform and snapshot.get("run_id") == run_id
            and snapshot.get("phase") == phase and isinstance(snapshot.get("instance_id"), str)
            and snapshot["instance_id"]):
        return False
    game = snapshot["game"]
    for key in ["callbacks", "background_events", "foreground_events", "command_sequence", "resume_frame_delta"]:
        if type(snapshot.get(key)) is not int or snapshot[key] < 0:
            return False
    for key in ["emulated_frames", "audio_queue_bytes", "audio_drained_bytes", "buttons", "generation", "presented_frames",
                "max_audio_queue_bytes", "audio_limit_bytes", "max_steps_per_callback"]:
        if type(game.get(key)) is not int or game[key] < 0:
            return False
    return (type(game.get("paused")) is bool and type(game.get("audio_started")) is bool
            and 0 < game["generation"] and game["audio_limit_bytes"] == 38400
            and game["audio_queue_bytes"] <= game["audio_limit_bytes"]
            and game["max_audio_queue_bytes"] <= game["audio_limit_bytes"]
            and game["max_steps_per_callback"] <= 3)


def verify_lifecycle_cycle(before, background, foreground, stable, resumed, running, cycle):
    snapshots = [before, background, foreground, stable, resumed, running]
    identity = {(item["instance_id"], item["game"]["generation"]) for item in snapshots}
    if len(identity) != 1:
        raise RuntimeError("Lifecycle test restarted or replaced the core host")
    frozen_frames = background["game"]["emulated_frames"]
    for item in [background, foreground, stable]:
        game = item["game"]
        if not (item["background_events"] >= cycle and game["paused"] and not game["audio_started"]
                and game["audio_queue_bytes"] == 0 and game["buttons"] == 0
                and game["emulated_frames"] == frozen_frames):
            raise RuntimeError("Background/foreground did not retain a paused, silent, neutral core")
    if not (foreground["foreground_events"] >= cycle and stable["callbacks"] >= foreground["callbacks"] + 30
            and background["callbacks"] >= before["callbacks"]
            and foreground["background_events"] > before["background_events"]
            and foreground["foreground_events"] > before["foreground_events"]
            and stable["game"]["presented_frames"] == background["game"]["presented_frames"]
            and frozen_frames >= before["game"]["emulated_frames"]):
        raise RuntimeError("Foreground pause observation was stale or frames advanced")
    if not (resumed["command_sequence"] == cycle and not resumed["game"]["paused"]
            and resumed["resume_frame_delta"] <= 3
            and 0 <= resumed["game"]["emulated_frames"] - frozen_frames <= 3
            and running["command_sequence"] == cycle and not running["game"]["paused"]
            and running["game"]["audio_started"] and running["game"]["emulated_frames"] > frozen_frames + 15
            and running["game"]["audio_drained_bytes"] > foreground["game"]["audio_drained_bytes"]):
        raise RuntimeError("Explicit resume failed or performed a catch-up burst")


def run_lifecycle_probe(device, bundle, container, output, platform, run_id, timeout):
    caches = container / "Library/Caches"
    deadline = time.monotonic() + timeout

    def wait(phase, predicate=lambda snapshot: True):
        path = caches / f"clover-lifecycle-{phase}.json"
        while time.monotonic() < deadline:
            if path.is_file():
                snapshot = json.loads(path.read_text())
                if valid_lifecycle_snapshot(snapshot, platform, run_id, phase) and predicate(snapshot):
                    return snapshot
            time.sleep(0.1)
        raise RuntimeError(f"Timed out waiting for current-run lifecycle {phase}: {path}")

    before = wait("ready", lambda s: s["game"]["emulated_frames"] >= 60 and s["game"]["audio_started"])
    cycles = []
    settings = "com.apple.Preferences" if platform == "ios" else "com.apple.TVSettings"
    try:
        for cycle in [1, 2]:
            run(["xcrun", "simctl", "launch", device, settings])
            background = wait("background", lambda s: s["background_events"] > before["background_events"])
            # Exercise actual suspension time, not just two coalesced callbacks.
            time.sleep(1)
            run(["xcrun", "simctl", "launch", device, bundle])
            foreground = wait("foreground", lambda s: s["background_events"] >= background["background_events"]
                              and s["foreground_events"] > before["foreground_events"])
            stable = wait("status", lambda s: s["background_events"] >= background["background_events"]
                          and s["foreground_events"] >= foreground["foreground_events"]
                          and s["callbacks"] >= foreground["callbacks"] + 30)
            command = caches / "clover-lifecycle-command.json"
            temporary = command.with_suffix(".tmp")
            temporary.write_text(json.dumps({"run_id": run_id, "sequence": cycle, "action": "resume"}))
            temporary.replace(command)
            resumed = wait("resumed", lambda s: s["command_sequence"] == cycle)
            running = wait("status", lambda s: s["command_sequence"] == cycle
                           and s["game"]["emulated_frames"] > resumed["game"]["emulated_frames"] + 15
                           and s["game"]["audio_drained_bytes"] > foreground["game"]["audio_drained_bytes"])
            evidence = dict(before=before, background=background, foreground=foreground,
                            stable=stable, resumed=resumed, running=running)
            cycles.append(evidence)
            verify_lifecycle_cycle(**evidence, cycle=cycle)
            before = running
    finally:
        (output / "lifecycle-observations.json").write_text(json.dumps(cycles, indent=2) + "\n")
        artifacts = output / f"lifecycle-artifacts-{run_id}"
        artifacts.mkdir(exist_ok=True)
        for path in caches.glob("clover-lifecycle-*.json"):
            shutil.copy2(path, artifacts / path.name)
    return {"schema": 1, "run_id": run_id, "platform": platform, "passed": True,
            "cycles": cycles, "scope": "OS background/foreground; sandbox-injected explicit resume"}


def valid_interruption_snapshot(snapshot, platform, run_id, phase):
    return (valid_lifecycle_snapshot(snapshot, platform, run_id, phase)
            and type(snapshot["game"].get("audio_interrupted")) is bool
            and all(type(snapshot.get(key)) is int and snapshot[key] >= 0
                    for key in ["interruption_began", "interruption_ended"]))


def verify_interruption_cycle(before, began, blocked, ended, stable, resumed, running, sequence):
    snapshots = [before, began, blocked, ended, stable, resumed, running]
    if len({(s["instance_id"], s["game"]["generation"]) for s in snapshots}) != 1:
        raise RuntimeError("Interruption probe restarted the host")
    frozen = began["game"]["emulated_frames"]
    for snapshot in [began, blocked, ended, stable]:
        game = snapshot["game"]
        if not (game["paused"] and not game["audio_started"] and game["audio_queue_bytes"] == 0
                and game["buttons"] == 0 and game["emulated_frames"] == frozen):
            raise RuntimeError("Interruption did not retain a paused, neutral, empty-queue core")
    if not (began["game"].get("audio_interrupted") is True and blocked["game"].get("audio_interrupted") is True
            and ended["game"].get("audio_interrupted") is False and stable["game"].get("audio_interrupted") is False
            and began.get("interruption_began", 0) > before.get("interruption_began", 0)
            and ended.get("interruption_ended", 0) > before.get("interruption_ended", 0)
            and [s["command_sequence"] for s in [began, blocked, ended, resumed]] == list(range(sequence, sequence + 4))
            and stable["callbacks"] >= ended["callbacks"] + 30):
        raise RuntimeError("Interruption notification or blocked-resume evidence is missing/stale")
    if not (not resumed["game"]["paused"] and resumed["game"].get("audio_interrupted") is False
            and resumed["resume_frame_delta"] <= 3 and 0 <= resumed["game"]["emulated_frames"] - frozen <= 3
            and not running["game"]["paused"] and running["game"]["audio_started"]
            and running["game"]["emulated_frames"] > frozen + 15
            and running["game"]["audio_drained_bytes"] > stable["game"]["audio_drained_bytes"]):
        raise RuntimeError("Post-interruption explicit resume did not restart bounded playback")


def run_interruption_probe(container, output, platform, run_id, timeout):
    caches = container / "Library/Caches"
    deadline = time.monotonic() + timeout
    cycles = []

    def wait(phase, predicate=lambda s: True):
        path = caches / f"clover-interruption-{phase}.json"
        while time.monotonic() < deadline:
            if path.is_file():
                snapshot = json.loads(path.read_text())
                if valid_interruption_snapshot(snapshot, platform, run_id, phase) and predicate(snapshot):
                    return snapshot
            time.sleep(0.1)
        raise RuntimeError(f"Timed out waiting for injected interruption {phase}")

    def command(action, sequence):
        path = caches / "clover-interruption-command.json"
        temporary = path.with_suffix(".tmp")
        temporary.write_text(json.dumps(dict(run_id=run_id, action=action, sequence=sequence)))
        temporary.replace(path)

    try:
        before = wait("ready", lambda s: s["game"]["audio_started"] and s["game"]["emulated_frames"] >= 60)
        for sequence in [1, 5]:
            command("begin", sequence)
            began = wait("began", lambda s: s["command_sequence"] == sequence)
            command("resume", sequence + 1)
            blocked = wait("blocked", lambda s: s["command_sequence"] == sequence + 1)
            command("end", sequence + 2)
            ended = wait("ended", lambda s: s["command_sequence"] == sequence + 2)
            stable = wait("status", lambda s: s["command_sequence"] == sequence + 2
                          and s["callbacks"] >= ended["callbacks"] + 30)
            command("resume", sequence + 3)
            resumed = wait("resumed", lambda s: s["command_sequence"] == sequence + 3)
            running = wait("status", lambda s: s["command_sequence"] == sequence + 3
                           and s["game"]["emulated_frames"] > resumed["game"]["emulated_frames"] + 15
                           and s["game"]["audio_drained_bytes"] > stable["game"]["audio_drained_bytes"])
            evidence = dict(before=before, began=began, blocked=blocked, ended=ended, stable=stable, resumed=resumed, running=running)
            cycles.append(evidence)
            verify_interruption_cycle(**evidence, sequence=sequence)
            before = running
    finally:
        (output / "interruptions-observations.json").write_text(json.dumps(cycles, indent=2) + "\n")
        artifacts = output / f"interruptions-artifacts-{run_id}"
        artifacts.mkdir(exist_ok=True)
        for path in caches.glob("clover-interruption-*.json"):
            shutil.copy2(path, artifacts / path.name)
    return dict(schema=1, run_id=run_id, platform=platform, passed=True, cycles=cycles,
                scope="Injected AVAudioSession notifications; not OS-issued call/Siri/route interruption evidence")


def valid_import_snapshot(snapshot, platform, run_id, *, require_source=True):
    if not (isinstance(snapshot, dict) and type(snapshot.get("schema")) is int and snapshot["schema"] == 1
            and snapshot.get("platform") == platform and snapshot.get("run_id") == run_id
            and isinstance(snapshot.get("instance_id"), str) and snapshot["instance_id"]
            and type(snapshot.get("source_unchanged")) is bool
            and (snapshot["source_unchanged"] or not require_source) and isinstance(snapshot.get("game"), dict)):
        return False
    status = snapshot.get("import")
    if not isinstance(status, dict):
        return False
    game = snapshot["game"]
    if status.get("active") is True:
        for key in ["emulated_frames", "audio_samples", "audio_drained_bytes", "audio_queue_bytes",
                    "max_audio_queue_bytes", "audio_limit_bytes", "max_steps_per_callback", "video_changes", "generation"]:
            if type(game.get(key)) is not int or game[key] < 0:
                return False
        if any(type(game.get(key)) is not bool for key in ["paused", "audio_started", "audio_nonzero"]):
            return False
        if not (game["audio_limit_bytes"] == 38400 and game["max_audio_queue_bytes"] <= 38400
                and game["audio_queue_bytes"] <= 38400 and game["max_steps_per_callback"] <= 3):
            return False
    elif game:
        return False
    if any(type(snapshot.get(key)) is not int or snapshot[key] < 0 for key in ["command_sequence", "callbacks"]):
        return False
    if any(type(status.get(key)) is not int or status[key] < 0 for key in ["selected", "completed", "cancelled", "rejected"]):
        return False
    return (all(type(status.get(key)) is bool for key in ["busy", "active", "picker_supported", "picker_open", "picker_presented"])
            and status["picker_supported"] == (platform == "ios") and isinstance(status.get("error"), str)
            and isinstance(snapshot.get("rom"), str) and type(snapshot.get("save_persisted")) is bool)


def verify_import_flow(evidence, platform):
    if len({s["instance_id"] for s in evidence.values()}) != 1:
        raise RuntimeError("Import flow restarted the host")
    if platform == "ios" and not evidence["picker"]["import"]["picker_presented"]:
        raise RuntimeError("iOS Files picker was not presented")
    for phase in ["cancelled", "invalid", "stopped", "late_cancelled"]:
        status = evidence[phase]["import"]
        if status["busy"] or status["active"] or evidence[phase]["game"]:
            raise RuntimeError(f"Import {phase} unexpectedly activated a session")
    if evidence["invalid"]["import"]["rejected"] < 1:
        raise RuntimeError("Invalid cartridge was not rejected")
    if evidence["cancelled"]["import"]["selected"] != 0:
        raise RuntimeError("Stale picker selection was accepted after cancellation")
    first = evidence["loaded"]
    last = evidence["reloaded"]
    path = Path(first["rom"])
    digest = path.parent.name
    if not (path.is_absolute() and path.name == "game.nes" and len(digest) == 64
            and path.parent.parent.name == "CloverRawNES"
            and ("Application Support" if platform == "ios" else "Caches") in path.parts
            and all(ch in "0123456789abcdef" for ch in digest) and last["rom"] == first["rom"]):
        raise RuntimeError("Content-addressed import identity was not retained")
    for snapshot in [first, last]:
        if not (snapshot["import"]["active"] and snapshot["game"].get("paused") is True
                and snapshot["game"].get("emulated_frames") == 0):
            raise RuntimeError("Import started gameplay before explicit Play")
    if not (evidence["late_cancelled"]["import"]["completed"] == first["import"]["completed"]
            and evidence["late_cancelled"]["import"]["cancelled"] > evidence["stopped"]["import"]["cancelled"]
            and last["import"]["completed"] == first["import"]["completed"] + 1):
        raise RuntimeError("Cancelled worker result was activated or reimport did not complete")
    if not (evidence["stopped"]["save_persisted"] and last["save_persisted"]):
        raise RuntimeError("Generated cartridge SRAM was not retained across stop and reimport")
    for phase in ["playing", "replaying"]:
        game = evidence[phase]["game"]
        if not (game.get("paused") is False and type(game.get("emulated_frames")) is int
                and game["emulated_frames"] >= 60 and game.get("audio_started") is True
                and game.get("audio_nonzero") is True and game.get("audio_drained_bytes", 0) > 0
                and 0 < game.get("max_audio_queue_bytes", 0) <= game.get("audio_limit_bytes") == 38400
                and 1 <= game.get("max_steps_per_callback", 0) <= 3):
            raise RuntimeError("Imported cartridge did not produce bounded video/audio playback")


def run_import_probe(device, container, output, platform, run_id, timeout):
    caches = container / "Library/Caches"
    deadline = time.monotonic() + timeout
    evidence = {}

    def wait(predicate):
        path = caches / "clover-import-status.json"
        while time.monotonic() < deadline:
            if path.is_file():
                snapshot = json.loads(path.read_text())
                if valid_import_snapshot(snapshot, platform, run_id) and predicate(snapshot):
                    return snapshot
            time.sleep(0.1)
        raise RuntimeError("Timed out waiting for the sandbox import probe; see import artifacts")

    def command(action, sequence):
        path = caches / "clover-import-command.json"
        temporary = path.with_suffix(".tmp")
        temporary.write_text(json.dumps(dict(run_id=run_id, action=action, sequence=sequence)))
        temporary.replace(path)

    try:
        evidence["ready"] = wait(lambda s: s["callbacks"] >= 120)
        command("show", 1)
        evidence["picker"] = wait(lambda s: s["command_sequence"] == 1
                                  and (s["import"]["picker_presented"] if platform == "ios" else True))
        if platform == "ios":
            picker_image = output / "import-picker-runtime.png"
            run(["xcrun", "simctl", "io", device, "screenshot", str(picker_image)])
            visible = run(["xcrun", "swift", str(ROOT / "scripts/check-apple-screenshot.swift"), str(picker_image), "core"], capture_output=True)
            (output / "import-picker-visible-result.json").write_text(visible.stdout)
        command("cancel", 2)
        evidence["cancelled"] = wait(lambda s: s["command_sequence"] == 2 and not s["import"]["picker_open"]
                                     and not s["import"]["picker_presented"])
        command("invalid", 3)
        evidence["invalid"] = wait(lambda s: s["command_sequence"] == 3 and s["import"]["rejected"] >= 1 and not s["import"]["busy"])
        command("select", 4)
        evidence["loaded"] = wait(lambda s: s["command_sequence"] == 4 and s["import"]["active"])
        command("play", 5)
        evidence["playing"] = wait(lambda s: s["command_sequence"] == 5 and s["game"].get("emulated_frames", 0) >= 60)
        command("stop", 6)
        evidence["stopped"] = wait(lambda s: s["command_sequence"] == 6 and not s["import"]["active"])
        command("cancel-import", 7)
        evidence["late_cancelled"] = wait(lambda s: s["command_sequence"] == 7 and not s["import"]["busy"])
        command("select", 8)
        evidence["reloaded"] = wait(lambda s: s["command_sequence"] == 8 and s["import"]["active"])
        command("play", 9)
        evidence["replaying"] = wait(lambda s: s["command_sequence"] == 9 and s["game"].get("emulated_frames", 0) >= 60)
        verify_import_flow(evidence, platform)
    finally:
        (output / "import-observations.json").write_text(json.dumps(evidence, indent=2) + "\n")
        artifacts = output / f"import-artifacts-{run_id}"
        artifacts.mkdir(exist_ok=True)
        for path in caches.glob("clover-import-*.json"):
            shutil.copy2(path, artifacts / path.name)
    return dict(schema=1, run_id=run_id, platform=platform, passed=True, evidence=evidence,
                scope="Sandbox-injected source selections; iOS picker presentation/cancel, not external-provider selection proof")


def valid_library_snapshot(snapshot, platform, run_id):
    if not valid_import_snapshot(snapshot, platform, run_id, require_source=False):
        return False
    if type(snapshot.get("source_exists")) is not bool or snapshot["source_exists"] != snapshot["source_unchanged"]:
        return False
    status = snapshot["import"]
    for key in ["library_count", "library_revision", "library_skipped", "opened"]:
        if type(status.get(key)) is not int or status[key] < 0:
            return False
    if not (type(status.get("scanning")) is bool and type(status.get("library_limited")) is bool):
        return False
    scene = snapshot.get("library_scene")
    if not isinstance(scene, dict) or type(scene.get("visible")) is not bool:
        return False
    for key in ["revision", "presented_revision"]:
        if type(scene.get(key)) is not int or scene[key] < 0:
            return False
    entries = scene.get("entries")
    if not (isinstance(entries, list) and len(entries) <= 64
            and all(isinstance(key, str) and len(key) == 64 and all(c in "0123456789abcdef" for c in key) for key in entries)
            and len(entries) == len(set(entries))):
        return False
    checksum = scene.get("checksum")
    return (isinstance(scene.get("selected"), str) and (not scene["selected"] or scene["selected"] in entries)
            and isinstance(checksum, str) and 0 < len(checksum) <= 20 and checksum.isascii()
            and checksum.isdecimal() and int(checksum) < 2**64)


def library_idle(snapshot, count):
    status, scene = snapshot["import"], snapshot["library_scene"]
    return (not status["active"] and not status["busy"] and not status["scanning"]
            and status["library_count"] == count and len(scene["entries"]) == count
            and scene["visible"] and scene["revision"] == scene["presented_revision"] == status["library_revision"]
            and int(scene["checksum"]) > 0)


def verify_library_flow(evidence):
    warm, cold = evidence["stored"], evidence["cold"]
    if warm["instance_id"] == cold["instance_id"]:
        raise RuntimeError("Library persistence probe did not restart the process")
    if not (library_idle(warm, 1) and library_idle(cold, 1) and not cold["source_exists"]
            and warm["library_scene"]["entries"] == cold["library_scene"]["entries"]
            and warm["library_scene"]["checksum"] == cold["library_scene"]["checksum"]):
        raise RuntimeError("Real library scene was not retained after source removal and process restart")
    for phase in ["reopened", "restored_open"]:
        snapshot = evidence[phase]
        if not (snapshot["instance_id"] == cold["instance_id"] and not snapshot["source_exists"]
                and snapshot["import"]["active"] and snapshot["import"]["opened"] >= 1
                and snapshot["import"]["completed"] == 0 and snapshot["game"].get("paused") is True
                and snapshot["game"].get("emulated_frames") == 0 and snapshot["rom"] == evidence["loaded"]["rom"]
                and snapshot["save_persisted"]):
            raise RuntimeError("Stored cartridge open reused source/provider or lost identity/SRAM")
    corrupt = evidence["corrupt"]
    if not (library_idle(corrupt, 0) and corrupt["import"]["library_skipped"] >= 1
            and library_idle(evidence["corrupt_open"], 0) and evidence["corrupt_open"]["import"]["opened"] == 1
            and library_idle(evidence["restored"], 1)):
        raise RuntimeError("Corrupt library content was exposed or could activate a stale selection")
    for phase in ["playing", "replaying", "restored_playing"]:
        snapshot = evidence[phase]
        game = snapshot["game"]
        if not (snapshot["import"]["active"] and game.get("paused") is False and game.get("emulated_frames", 0) >= 60
                and game.get("audio_started") is True and game.get("audio_drained_bytes", 0) > 0):
            raise RuntimeError("Library cartridge did not produce bounded video/audio")


def run_library_probe(device, bundle, container, output, platform, run_id, timeout, env):
    caches = container / "Library/Caches"
    deadline = time.monotonic() + timeout
    evidence = {}
    rom = original = None

    def wait(predicate):
        path = caches / "clover-import-status.json"
        while time.monotonic() < deadline:
            if path.is_file():
                snapshot = json.loads(path.read_text())
                if valid_library_snapshot(snapshot, platform, run_id) and predicate(snapshot):
                    return snapshot
            time.sleep(0.1)
        raise RuntimeError("Timed out waiting for the real library probe")

    def command(action, sequence):
        path = caches / "clover-import-command.json"
        temporary = path.with_suffix(".tmp")
        temporary.write_text(json.dumps(dict(run_id=run_id, action=action, sequence=sequence)))
        temporary.replace(path)

    def playing(sequence):
        return wait(lambda s: s["command_sequence"] == sequence and s["game"].get("emulated_frames", 0) >= 60
                    and s["game"].get("audio_started") is True and s["game"].get("audio_drained_bytes", 0) > 0)

    try:
        evidence["empty"] = wait(lambda s: library_idle(s, 0))
        command("seed", 1)
        evidence["loaded"] = wait(lambda s: s["command_sequence"] == 1 and s["import"]["active"])
        command("play", 2); evidence["playing"] = playing(2)
        command("stop", 3)
        evidence["stored"] = wait(lambda s: s["command_sequence"] == 3 and library_idle(s, 1) and s["save_persisted"])
        source = caches / "import-probe-source.nes"
        with source.open("rb") as stream:
            original = stream.read(1024 * 1024 + 1)
        if len(original) > 1024 * 1024:
            raise RuntimeError("Owned source fixture exceeds the import limit")
        candidate = Path(evidence["loaded"]["rom"])
        if not (not candidate.is_symlink() and candidate.resolve().is_relative_to(container.resolve())
                and candidate.name == "game.nes" and candidate.stat().st_size == len(original) and candidate.read_bytes() == original):
            raise RuntimeError("Library probe path is not its owned sandbox fixture")
        rom = candidate
        source.unlink()  # Only this runner's generated source, never a user-selected URL.
        run(["xcrun", "simctl", "terminate", device, bundle])
        run(["xcrun", "simctl", "launch", device, bundle], env=env)
        evidence["cold"] = wait(lambda s: s["instance_id"] != evidence["stored"]["instance_id"] and library_idle(s, 1))
        image = output / "library-menu-runtime.png"
        run(["xcrun", "simctl", "io", device, "screenshot", str(image)])
        visible = run(["xcrun", "swift", str(ROOT / "scripts/check-apple-screenshot.swift"), str(image), "preview"], capture_output=True)
        (output / "library-menu-visible-result.json").write_text(visible.stdout)
        command("open", 1)
        evidence["reopened"] = wait(lambda s: s["command_sequence"] == 1 and s["import"]["active"])
        command("play", 2); evidence["replaying"] = playing(2)
        command("stop", 3)
        evidence["restopped"] = wait(lambda s: s["command_sequence"] == 3 and library_idle(s, 1))
        # Deliberate corruption/restoration is confined to this fresh simulator's
        # generated fixture and happens with no live core. No user data is touched.
        rom.write_bytes(b"invalid owned probe cartridge")
        command("refresh", 4)
        evidence["corrupt"] = wait(lambda s: s["command_sequence"] == 4 and library_idle(s, 0))
        command("open", 5)
        evidence["corrupt_open"] = wait(lambda s: s["command_sequence"] == 5 and library_idle(s, 0))
        rom.write_bytes(original)
        command("refresh", 6)
        evidence["restored"] = wait(lambda s: s["command_sequence"] == 6 and library_idle(s, 1))
        command("open", 7)
        evidence["restored_open"] = wait(lambda s: s["command_sequence"] == 7 and s["import"]["active"])
        command("play", 8); evidence["restored_playing"] = playing(8)
        verify_library_flow(evidence)
    finally:
        if (rom is not None and original is not None and rom.is_file() and not rom.is_symlink()
                and rom.resolve().is_relative_to(container.resolve())):
            rom.write_bytes(original)  # Restore only our own fixture before cleanup/inspection.
        (output / "library-observations.json").write_text(json.dumps(evidence, indent=2) + "\n")
    return dict(schema=1, run_id=run_id, platform=platform, passed=True, evidence=evidence,
                scope="Generated owned library, source removal and cold process restart; not user-provider/device validation")


def valid_transfer_snapshot(snapshot, platform, run_id):
    if not (isinstance(snapshot, dict) and type(snapshot.get("schema")) is int and snapshot["schema"] == 1
            and snapshot.get("run_id") == run_id and snapshot.get("platform") == platform
            and isinstance(snapshot.get("instance_id"), str) and snapshot["instance_id"]
            and type(snapshot.get("sequence")) is int and snapshot["sequence"] >= 0
            and type(snapshot.get("active")) is bool):
        return False
    state, connection, game = snapshot.get("transfer"), snapshot.get("connection"), snapshot.get("game")
    if not (isinstance(state, dict) and isinstance(connection, dict) and isinstance(game, dict)
            and type(state.get("enabled")) is bool and type(state.get("busy")) is bool
            and isinstance(state.get("message"), str)):
        return False
    if any(type(state.get(key)) is not int or state[key] < 0 for key in ("received", "total", "revision")):
        return False
    if not (state["revision"] > 0 and 0 <= state["received"] <= state["total"] <= 1024**3):
        return False
    if connection and not (set(connection) <= {"port", "code"} and type(connection.get("port")) is int and 0 < connection["port"] < 65536
                           and isinstance(connection.get("code"), str) and len(connection["code"]) == 6
                           and connection["code"].isascii() and connection["code"].isdigit()):
        return False
    profiles = state.get("profiles")
    if not isinstance(profiles, list) or len(profiles) > 64:
        return False
    for profile in profiles:
        if not (isinstance(profile, dict) and isinstance(profile.get("id"), str) and len(profile["id"]) <= 80
                and all(c in "0123456789abcdef-v" for c in profile["id"])
                and isinstance(profile.get("label"), str) and isinstance(profile.get("games"), list)
                and 0 < len(profile["games"]) <= 64):
            return False
        for row in profile["games"]:
            if not (isinstance(row, dict) and isinstance(row.get("id"), str) and row["id"]
                    and isinstance(row.get("title"), str) and type(row.get("playable")) is bool
                    and isinstance(row.get("reason"), str)):
                return False
    if snapshot["active"]:
        if not (type(game.get("paused")) is bool and type(game.get("audio_started")) is bool):
            return False
        if any(type(game.get(key)) is not int or game[key] < 0 for key in
               ("emulated_frames", "video_changes", "audio_samples", "audio_drained_bytes", "audio_queue_bytes",
                "max_audio_queue_bytes", "audio_limit_bytes", "max_steps_per_callback")):
            return False
        if not (game["audio_limit_bytes"] == 38400 and game["audio_queue_bytes"] <= 38400
                and game["max_audio_queue_bytes"] <= 38400 and game["max_steps_per_callback"] <= 3):
            return False
    return True


def run_transfer_probe(device, bundle, container, output, platform, run_id, timeout, env, image, extra_image=None):
    caches = container / "Library/Caches"
    deadline = time.monotonic() + timeout
    evidence = {}

    def wait(predicate):
        path = caches / "clover-transfer-status.json"
        last = None
        while time.monotonic() < deadline:
            if path.is_file():
                snapshot = json.loads(path.read_text())
                last = {key: value for key, value in snapshot.items() if key != "connection"}
                if valid_transfer_snapshot(snapshot, platform, run_id) and predicate(snapshot):
                    return snapshot
            time.sleep(0.1)
        if last:
            (output / "transfer-timeout.json").write_text(json.dumps(last, indent=2) + "\n")
        raise RuntimeError("Timed out waiting for the profile transfer probe")

    def remember(name, snapshot):
        # Pairing credentials belong only in the fresh simulator's private probe
        # channel; never persist them in exported evidence/logs.
        evidence[name] = {k: v for k, v in snapshot.items() if k != "connection"}
        (output / "transfer-observations.json").write_text(json.dumps(evidence, indent=2) + "\n")
        return snapshot

    def http(connection, method, path, body=b"", *, token=None, extra=None):
        client = http_client.HTTPConnection("127.0.0.1", connection["port"], timeout=max(10, timeout))
        headers = {"Authorization": "Bearer " + (connection.get("token", "") if token is None else token)}
        headers.update(extra or {})
        try:
            client.request(method, path, body=body, headers=headers)
            response = client.getresponse()
            return response.status, response.read(1024 * 1024).decode()
        finally:
            client.close()

    def command(action, sequence, **values):
        path = caches / "clover-transfer-command.json"
        temporary = path.with_suffix(".tmp")
        temporary.write_text(json.dumps(dict(run_id=run_id, action=action, sequence=sequence, **values)))
        temporary.replace(path)

    def pair_browser(connection):
        status, body = http(connection, "POST", "/pair", connection["code"].encode())
        if status != 200:
            raise RuntimeError(f"Six-digit pairing failed: {status}: {body}")
        token = json.loads(body).get("token")
        if not (isinstance(token, str) and len(token) == 32 and all(c in "0123456789abcdef" for c in token)):
            raise RuntimeError("Pairing did not issue a private session credential")
        return dict(connection, token=token)

    def screenshot(name):
        path = output / f"transfer-{name}.png"
        run(["xcrun", "simctl", "io", device, "screenshot", str(path)])
        mode = "native" if platform == "ios" and name in ("empty-launcher", "imported-launcher") else "preview"
        visible = run(["xcrun", "swift", str(ROOT / "scripts/check-apple-screenshot.swift"), str(path), mode], capture_output=True)
        (output / f"transfer-{name}-visible.json").write_text(visible.stdout)

    with image.open("rb") as source:
        source_hash = hashlib.file_digest(source, "sha256").hexdigest()
    initial = remember("empty", wait(lambda s: not s["transfer"]["busy"] and
                                    (platform == "ios" or s.get("connection", {}).get("port"))))
    if initial["active"] or initial["transfer"]["profiles"]:
        raise RuntimeError("Fresh launch did not show an empty profile launcher")
    screenshot("empty-launcher")
    offset = 0
    def local_import(source, name):
        nonlocal offset
        offset += 1
        revision = json.loads((caches / "clover-transfer-status.json").read_text())["transfer"]["revision"]
        command("select-file", offset, path=str(source))
        return remember(name, wait(lambda s: s["sequence"] == offset and not s["transfer"]["busy"]
                                  and s["transfer"]["revision"] > revision))

    if platform == "ios":
        if initial.get("connection") or initial["transfer"]["enabled"]:
            raise RuntimeError("Files-first iOS opened a LAN listener")
        fixture = caches / "transfer-probe-source.hsqs"
        with image.open("rb") as source, fixture.open("xb") as target:
            while chunk := source.read(65536):
                target.write(chunk)
        selected = local_import(fixture, "local-files-import")
        if not selected["transfer"]["message"].startswith("Profile ready"):
            raise RuntimeError("Coordinated Files import failed")
    else:
        qr = run(["xcrun", "swift", str(ROOT / "scripts/check-apple-transfer-qr.swift"),
                  str(output / "transfer-empty-launcher.png")], capture_output=True)
        (output / "transfer-qr-result.json").write_text(qr.stdout)
        connection = initial["connection"]
        if http(connection, "GET", "/")[0] != 200:
            raise RuntimeError("Upload page unavailable")
        if http(connection, "GET", "/status", token="incorrect")[0] != 403:
            raise RuntimeError("Unauthenticated status was exposed")
        if http(connection, "GET", "/status", token=connection["code"])[0] != 403:
            raise RuntimeError("Short code incorrectly acts as an upload/session credential")
        wrong = f"{(int(connection['code']) + 1) % 1000000:06d}"
        if http(connection, "POST", "/pair", wrong.encode())[0] != 403:
            raise RuntimeError("Wrong pairing code was accepted")
        if http(connection, "POST", "/pair", connection["code"].encode(), extra={"Origin": "http://attacker.invalid"})[0] != 403:
            raise RuntimeError("Cross-origin pairing was accepted")
        connection = pair_browser(connection)
        if http(connection, "GET", "/status", extra={"Origin": "http://attacker.invalid"})[0] != 403:
            raise RuntimeError("Cross-origin request was accepted")
        if http(connection, "POST", "/upload", b"invalid image")[0] != 422:
            raise RuntimeError("Invalid upload was accepted")
        with image.open("rb") as body:
            status, message = http(connection, "POST", "/upload", body, extra={"Content-Length": str(image.stat().st_size)})
        if status != 200:
            raise RuntimeError(f"Image upload/import failed: {status}: {message}")
    imported = remember("imported", wait(lambda s: not s["transfer"]["busy"] and len(s["transfer"]["profiles"]) == 1))
    if imported["active"]:
        raise RuntimeError("Import automatically started gameplay")
    profile = imported["transfer"]["profiles"][0]
    if not profile["id"].startswith(source_hash):
        raise RuntimeError("Published profile does not match uploaded bytes")
    screenshot("imported-launcher")
    if platform == "ios":
        invalid = caches / "transfer-invalid.hsqs"
        with invalid.open("xb") as target:
            target.write(b"hsqs damaged")
        preserved = local_import(invalid, "invalid-preserved")
        invalid.unlink()
        if preserved["transfer"]["profiles"] != imported["transfer"]["profiles"] or preserved["active"]:
            raise RuntimeError("Rejected Files import changed the collection")
        selected = local_import(fixture, "local-files-reimport")
        if selected["active"] or selected["transfer"]["profiles"] != imported["transfer"]["profiles"]:
            raise RuntimeError("Coordinated local reimport changed identity or auto-started a game")
        with fixture.open("rb") as source:
            if hashlib.file_digest(source, "sha256").hexdigest() != source_hash:
                raise RuntimeError("Coordinated local selection changed its source")
        fixture.unlink()  # Only our newly created copy, after its worker completed.
    else:
        if http(connection, "POST", "/upload", b"hsqs damaged")[0] != 422:
            raise RuntimeError("Damaged SquashFS was accepted")
        preserved = remember("invalid-preserved", wait(lambda s: not s["transfer"]["busy"] and s["transfer"]["profiles"] == imported["transfer"]["profiles"]))
        if preserved["active"]:
            raise RuntimeError("Rejected upload changed gameplay")
        command("new-code", 1)
        rotated = remember("new-code", wait(lambda s: s["sequence"] == 1 and s.get("connection", {}).get("code") not in (None, connection["code"])))
        if rotated["connection"]["port"] != connection["port"]:
            raise RuntimeError("Refreshing the PIN changed the browser address")
        if http(rotated["connection"], "GET", "/status", token=connection["token"])[0] != 403:
            raise RuntimeError("New code did not invalidate the previous browser session")
        if http(rotated["connection"], "POST", "/pair", connection["code"].encode())[0] != 403:
            raise RuntimeError("New code still accepts the previous PIN")
        connection = pair_browser(rotated["connection"])
        offset = 1
        command("focus-library", 1 + offset)
        remember("focused", wait(lambda s: s["sequence"] == 1 + offset and s["transfer"].get("focused_collection") == profile["id"]))
        screenshot("focused-library")
        offset += 1
    game = next((g for g in profile["games"] if g["playable"]), None)
    if not game:
        raise RuntimeError("Imported profile has no supported playable games")
    sequence = offset

    def step(action, predicate, **values):
        nonlocal sequence
        sequence += 1
        command(action, sequence, **values)
        return wait(lambda s: s["sequence"] == sequence and predicate(s))

    def real_menu(s):
        menu = s.get("classic", {})
        return (s.get("classic_active") is True and not s["active"] and menu.get("kind") == "rust-classic"
                and menu.get("visible") is True and menu.get("frames", 0) >= 3
                and menu.get("embedded_core_loaded") is False and int(menu.get("checksum", "0")) > 0)

    def collection_screen(s):
        ready = not s["classic_active"] and not s["active"]
        return ready and (not s.get("connection") and not s["transfer"]["enabled"] if platform == "ios" else s.get("connection", {}).get("port"))

    if extra_image:
        with extra_image.open("rb") as source:
            extra_hash = hashlib.file_digest(source, "sha256").hexdigest()
        if extra_hash == source_hash:
            raise RuntimeError("Multi-collection check requires two different images")
        if platform == "ios":
            second_source = caches / "transfer-second-source.hsqs"
            with extra_image.open("rb") as source, second_source.open("xb") as target:
                while chunk := source.read(65536):
                    target.write(chunk)
            sequence += 1
            command("select-file", sequence, path=str(second_source))
        else:
            with extra_image.open("rb") as body:
                status, message = http(connection, "POST", "/upload", body, extra={"Content-Length": str(extra_image.stat().st_size)})
            if status != 200:
                raise RuntimeError(f"Second collection import failed: {status}: {message}")
        imported = remember("multiple-imported", wait(lambda s: not s["transfer"]["busy"] and len(s["transfer"]["profiles"]) == 2))
        if platform == "ios":
            second_source.unlink()
        screenshot("imported-launcher")
        profiles = imported["transfer"]["profiles"]
        if not any(p["id"].startswith(extra_hash) for p in profiles):
            raise RuntimeError("Second collection identity does not match its source")
        for index, target in enumerate(profiles):
            if platform == "tvos":
                current = wait(lambda s: collection_screen(s) and s["transfer"].get("focused_collection"))
                current_index = next(i for i, p in enumerate(profiles) if p["id"] == current["transfer"]["focused_collection"])
                if current_index != index:
                    current = remember(f"collection-move-{index}", step("collection-down" if index > current_index else "collection-up",
                        lambda s: collection_screen(s) and s["transfer"].get("focused_collection") == target["id"]))
                reloads = current["transfer"]["table_reloads"]
                refreshed = remember(f"collection-stable-{index}", step("new-code", lambda s: collection_screen(s)
                    and s["transfer"].get("focused_collection") == target["id"] and s["callbacks"] >= current["callbacks"] + 30))
                if refreshed["transfer"]["table_reloads"] != reloads:
                    raise RuntimeError("Status/pairing update unnecessarily reloaded collection rows")
            opened = remember(f"collection-open-{index}", step("open", lambda s: real_menu(s) and target["id"] in s["classic"]["assets"], profile=target["id"]))
            if opened["classic"]["game_count"] != len(target["games"]):
                raise RuntimeError("Opened the wrong collection")
            remember(f"collection-return-{index}", step("menu-back", lambda s: collection_screen(s)
                and (platform == "ios" or s["transfer"].get("focused_collection") == target["id"])))
        if platform == "tvos":
            remember("collection-burst", step("collection-burst", lambda s: collection_screen(s)
                and s["transfer"].get("focused_collection") == profiles[0]["id"]))
            primary_index = next(i for i, p in enumerate(profiles) if p["id"] == profile["id"])
            if primary_index != 0:
                remember("collection-primary", step("collection-down", lambda s: collection_screen(s)
                    and s["transfer"].get("focused_collection") == profile["id"]))
        with extra_image.open("rb") as source:
            if hashlib.file_digest(source, "sha256").hexdigest() != extra_hash:
                raise RuntimeError("Second source image was changed")

    loaded = remember("classic-menu", step("open", real_menu, profile=profile["id"]))
    if loaded["game"] or loaded["classic"]["game_count"] != len(profile["games"]):
        raise RuntimeError("Collection selection bypassed the real Classic menu or created a core")
    selected_id = loaded["classic"]["selected"]
    screenshot("classic-menu")
    remember("classic-right", step("menu-right", lambda s: real_menu(s) and s["classic"]["selected"] != selected_id))
    remember("classic-left", step("menu-left", lambda s: real_menu(s) and s["classic"]["selected"] == selected_id))
    remember("collections-return", step("menu-back", collection_screen))
    remember("classic-reopen", step("open", lambda s: real_menu(s) and s["classic"]["selected"] == selected_id, profile=profile["id"]))
    playing = remember("playing", step("menu-confirm", lambda s: s["active"] and s["game"].get("emulated_frames", 0) >= 600))
    core = playing["game"]
    if not (core.get("paused") is False and core.get("audio_started") is True
            and core.get("video_changes", 0) >= 2
            and core.get("audio_samples", 0) > 0 and core.get("audio_drained_bytes", 0) > 0
            and 0 < core.get("max_audio_queue_bytes", 0) <= core.get("audio_limit_bytes", 0) == 38400
            and 1 <= core.get("max_steps_per_callback", 0) <= 3):
        raise RuntimeError("Imported game did not produce bounded/draining audio and frames")
    screenshot("game")
    settings = "com.apple.Preferences" if platform == "ios" else "com.apple.TVSettings"
    run(["xcrun", "simctl", "launch", device, settings]);time.sleep(1)
    run(["xcrun", "simctl", "launch", device, bundle], env=env)
    paused = remember("game-foreground-paused", wait(lambda s: s["active"] and s["game"].get("paused") is True))
    frozen = remember("game-still-paused", wait(lambda s: s.get("callbacks", 0) >= paused["callbacks"] + 30))
    if not (frozen["game"]["emulated_frames"] == paused["game"]["emulated_frames"] and frozen["game"]["paused"]
            and frozen["game"]["audio_queue_bytes"] == 0 and frozen["game"]["audio_started"] is False
            and frozen["game"]["generation"] == playing["game"]["generation"]):
        raise RuntimeError("Classic gameplay automatically resumed or replaced its core after foregrounding")
    remember("game-explicit-resume", step("play", lambda s: s["active"] and not s["game"]["paused"] and s["game"]["emulated_frames"] >= frozen["game"]["emulated_frames"] + 60))
    returned = remember("classic-after-game", step("stop", lambda s: real_menu(s) and s["classic"]["selected"] == selected_id))
    if returned.get("connection") or returned["transfer"]["enabled"]:
        raise RuntimeError("Stopping a game returned to acquisition instead of the Classic menu")
    screenshot("classic-after-game")
    stopped = remember("stopped", step("menu-back", collection_screen))
    # The app must survive an OS background/foreground transition and re-pair.
    old = pair_browser(stopped["connection"]) if platform == "tvos" else None
    settings = "com.apple.Preferences" if platform == "ios" else "com.apple.TVSettings"
    run(["xcrun", "simctl", "launch", device, settings])
    time.sleep(1)
    try:
        status, _ = http(old, "GET", "/status") if old else (0, "")
        if status == 200:
            raise RuntimeError("Background service still accepts paired requests")
    except (OSError, http_client.HTTPException):
        pass
    run(["xcrun", "simctl", "launch", device, bundle], env=env)
    foreground = remember("foreground", wait(lambda s: s.get("callbacks", 0) > stopped["callbacks"] and collection_screen(s)
                                            and (not old or s.get("connection", {}).get("code") not in (None, old["code"]))))
    if old:
        if http(foreground["connection"], "GET", "/status", token=old["token"])[0] != 403:
            raise RuntimeError("Old browser session survived backgrounding")
        paired = pair_browser(foreground["connection"])
        if http(paired, "GET", "/status", token=old["token"])[0] != 403:
            raise RuntimeError("Re-pairing restored an old browser session")
    run(["xcrun", "simctl", "terminate", device, bundle])
    run(["xcrun", "simctl", "launch", device, bundle], env=env)
    cold = remember("cold", wait(lambda s: s["instance_id"] != stopped["instance_id"]
                                and not s["transfer"]["busy"] and s["transfer"]["profiles"] == imported["transfer"]["profiles"]))
    if cold["active"] or cold.get("classic_active"):
        raise RuntimeError("Cold restart bypassed initial collection selection")
    if platform == "ios" and (cold.get("connection") or cold["transfer"]["enabled"]):
        raise RuntimeError("Cold iOS restart opened a LAN listener")
    with image.open("rb") as source:
        if hashlib.file_digest(source, "sha256").hexdigest() != source_hash:
            raise RuntimeError("Source image was changed")
    store = container / ("Library/Caches" if platform == "tvos" else "Library/Application Support") / "CloverProfiles"
    if list(store.glob(".upload-*")):
        raise RuntimeError("Completed/rejected uploads left temporary source files")
    return dict(schema=1, run_id=run_id, platform=platform, passed=True, evidence=evidence,
                scope="HTTP/Files image import → collection selector → real Rust Classic menu/navigation → game → Classic menu → collections; injected controls, not physical-device proof")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--platform", choices=["ios", "tvos"], required=True)
    parser.add_argument("--app", choices=["core", "preview", "game", "lifecycle", "interruptions", "input", "import", "library", "touch", "transfer"], default="core")
    parser.add_argument("--build-only", action="store_true", help="Compile/link only; not runtime evidence")
    parser.add_argument("--keep-device", action="store_true", help="Keep this runner's simulator for inspection")
    parser.add_argument("--interactive", action="store_true", help="Launch the real profile/upload launcher and retain its simulator; not automated test evidence")
    parser.add_argument("--image", type=Path, help="User-supplied .hsqs source for --app transfer (never modified)")
    parser.add_argument("--extra-image", type=Path, help="Second distinct .hsqs for multi-collection navigation checks")
    parser.add_argument("--timeout", type=float, default=180, help="Maximum seconds waiting for app test results")
    args = parser.parse_args()
    if args.timeout <= 0:
        parser.error("--timeout must be positive")
    if args.extra_image and (args.app != "transfer" or not args.extra_image.is_file() or args.extra_image.suffix.lower() != ".hsqs"
                             or not 0 < args.extra_image.stat().st_size <= 1024**3):
        parser.error("--extra-image requires --app transfer and a nonempty .hsqs file at most 1 GiB")
    if args.interactive and (args.app != "preview" or args.build_only):
        parser.error("--interactive requires --app preview and cannot use --build-only")
    if args.app == "transfer" and not args.build_only and (not args.image or not args.image.is_file()
            or args.image.suffix.lower() != ".hsqs" or not 0 < args.image.stat().st_size <= 1024**3):
        parser.error("--app transfer requires --image pointing to a local .hsqs file, at most 1 GiB")
    sdk = "iphonesimulator" if args.platform == "ios" else "appletvsimulator"
    system = "iOS" if args.platform == "ios" else "tvOS"
    bundle = "org.cloverproject.core-smoke" if args.app == "core" else "org.cloverproject.apple-preview"
    executable = "CloverCoreSmoke" if args.app == "core" else "CloverApplePreview"
    target = "clover_apple_smoke" if args.app == "core" else "clover_apple_preview"
    output = ROOT / ".build" / "apple-app" / args.platform
    output.mkdir(parents=True, exist_ok=True)
    runtime = device_type = None
    if not args.build_only:
        runtime, device_type = select_runtime(args.platform)
    run(["cmake", "-S", str(ROOT / "portable/apple"), "-B", str(output), "-G", "Xcode",
         f"-DCMAKE_SYSTEM_NAME={system}", f"-DCMAKE_OSX_SYSROOT={sdk}", "-DCMAKE_OSX_ARCHITECTURES=arm64",
         "-DCMAKE_OSX_DEPLOYMENT_TARGET=15.0", "-DCMAKE_XCODE_ATTRIBUTE_CODE_SIGNING_ALLOWED=NO",
         f"-DCLOVER_BUILD_RUST_PREVIEW={'ON' if args.app != 'core' else 'OFF'}"])
    with (output / "build.log").open("w") as log:
        try:
            run(["cmake", "--build", str(output), "--config", "Release", "--target", target, "--parallel", "3"],
                stdout=log, stderr=subprocess.STDOUT)
        except subprocess.CalledProcessError as error:
            raise RuntimeError(f"Apple build failed; see {output / 'build.log'}") from error
    app = output / "Release-iphonesimulator" / f"{executable}.app"
    if args.platform == "tvos":
        app = output / "Release-appletvsimulator" / f"{executable}.app"
    if not app.is_dir():
        raise RuntimeError(f"Expected simulator app was not built: {app}")
    validate_bundle_notices(app, rust_preview=args.app != "core")
    if args.build_only:
        print(f"COMPILE/LINK ONLY: {app}")
        return
    run_id = uuid.uuid4().hex
    device = run(["xcrun", "simctl", "create", f"Clover-{args.platform}-{run_id[:8]}",
                  device_type, runtime], capture_output=True).stdout.strip()
    try:
        metadata_name = "interactive" if args.interactive else args.app
        (output / f"{metadata_name}-simulator.json").write_text(json.dumps({
            "runtime": runtime, "device_type": device_type, "device": device, "sdk": sdk,
            "run_id": run_id if not args.interactive else None, "interactive": args.interactive,
        }, indent=2) + "\n")
        run(["xcrun", "simctl", "boot", device])
        run(["xcrun", "simctl", "bootstatus", device, "-b"], timeout=180)
        run(["xcrun", "simctl", "install", device, str(app)])
        container = Path(run(["xcrun", "simctl", "get_app_container", device, bundle, "data"],
                             capture_output=True).stdout.strip())
        receipt_name = {"core": "clover-smoke-result.json", "preview": "clover-preview-result.json",
                          "game": "clover-game-result.json", "input": "clover-input-result.json", "touch": "clover-touch-result.json",
                          "lifecycle": "unused", "interruptions": "unused", "import": "unused", "library": "unused", "transfer": "unused"}[args.app]
        receipt = container / "Library/Caches" / receipt_name
        env = dict(os.environ, SIMCTL_CHILD_CLOVER_SMOKE_RUN_ID=run_id)
        env["SIMCTL_CHILD_CLOVER_APPLE_GAME"] = "1" if args.app in ("game", "lifecycle", "interruptions", "input", "touch") else "0"
        env["SIMCTL_CHILD_CLOVER_APPLE_TOUCH"] = "1" if args.app == "touch" else "0"
        env["SIMCTL_CHILD_CLOVER_APPLE_TRANSFER"] = "1" if args.app == "transfer" else "0"
        env["SIMCTL_CHILD_CLOVER_APPLE_INPUT"] = "1" if args.app == "input" else "0"
        env["SIMCTL_CHILD_CLOVER_APPLE_IMPORT"] = "1" if args.app == "import" else "0"
        env["SIMCTL_CHILD_CLOVER_APPLE_LIBRARY"] = "1" if args.app == "library" else "0"
        env["SIMCTL_CHILD_CLOVER_APPLE_LIFECYCLE"] = "1" if args.app == "lifecycle" else "0"
        env["SIMCTL_CHILD_CLOVER_APPLE_INTERRUPTION"] = "1" if args.app == "interruptions" else "0"
        if args.interactive:
            env.pop("SIMCTL_CHILD_CLOVER_SMOKE_RUN_ID", None)
        run(["xcrun", "simctl", "launch", device, bundle], env=env)
        if args.interactive:
            open_interactive_simulator(device)
            print(f"INTERACTIVE ONLY: {args.platform} Classic profile/upload launcher; owned simulator retained: {device}")
            print(f"Cleanup: xcrun simctl shutdown {device}; xcrun simctl delete {device}")
            return
        if args.app in ("lifecycle", "interruptions", "import", "library", "transfer"):
            if args.app == "lifecycle":
                report = run_lifecycle_probe(device, bundle, container, output, args.platform, run_id, args.timeout)
            elif args.app == "interruptions":
                report = run_interruption_probe(container, output, args.platform, run_id, args.timeout)
            elif args.app == "import":
                report = run_import_probe(device, container, output, args.platform, run_id, args.timeout)
            elif args.app == "library":
                report = run_library_probe(device, bundle, container, output, args.platform, run_id, args.timeout, env)
            else:
                report = run_transfer_probe(device, bundle, container, output, args.platform, run_id, args.timeout, env, args.image, args.extra_image)
            (output / f"{args.app}-runtime-result.json").write_text(json.dumps(report, indent=2) + "\n")
            screenshot = output / f"{args.app}-runtime.png"
            run(["xcrun", "simctl", "io", device, "screenshot", str(screenshot)])
            visible_mode = ("native" if args.platform == "ios" else "preview") if args.app == "transfer" else "game"
            visible = run(["xcrun", "swift", str(ROOT / "scripts/check-apple-screenshot.swift"),
                            str(screenshot), visible_mode], capture_output=True)
            (output / f"{args.app}-visible-result.json").write_text(visible.stdout)
            print(f"PASS: {args.platform}: {report['scope']}")
            return
        deadline = time.monotonic() + args.timeout
        while time.monotonic() < deadline:
            if receipt.is_file():
                report = json.loads(receipt.read_text())
                if isinstance(report, dict) and report.get("run_id") == run_id:
                    report_path = output / f"{args.app}-runtime-result.json"
                    report_path.write_text(json.dumps(report, indent=2) + "\n")
                    artifact_value = report.get("artifacts", "")
                    artifact = Path(artifact_value if isinstance(artifact_value, str) else "").resolve()
                    caches = (container / "Library/Caches").resolve()
                    if artifact.is_relative_to(caches) and artifact.name.startswith("clover-core-smoke-") and artifact.is_dir():
                        shutil.copytree(artifact, output / f"{args.app}-artifacts-{run_id}")
                    if not valid_report(report, args.platform, args.app, run_id):
                        raise RuntimeError(f"App checks failed: {json.dumps(report)}")
                    screenshot = output / f"{args.app}-runtime.png"
                    run(["xcrun", "simctl", "io", device, "screenshot", str(screenshot)])
                    visible = run(["xcrun", "swift", str(ROOT / "scripts/check-apple-screenshot.swift"),
                                   str(screenshot), "game" if args.app == "touch" else args.app], capture_output=True)
                    (output / f"{args.app}-visible-result.json").write_text(visible.stdout)
                    print(f"PASS: {args.platform} {args.app} app checks; receipt: {report_path}")
                    return
            time.sleep(0.25)
        raise RuntimeError(f"App produced no current-run receipt within {args.timeout}s; simulator {device}")
    finally:
        if args.keep_device or args.interactive:
            print(f"Owned simulator retained: {device}")
        else:
            # Never stop/delete another simulator; this UUID was created above.
            subprocess.run(["xcrun", "simctl", "shutdown", device], check=False)
            subprocess.run(["xcrun", "simctl", "delete", device], check=False)


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, subprocess.CalledProcessError, subprocess.TimeoutExpired) as error:
        raise SystemExit(str(error)) from error
