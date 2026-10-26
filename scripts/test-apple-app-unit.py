#!/usr/bin/env python3
"""Receipt validation must reject stale, malformed, or compile-only results."""
import copy
import importlib.util
from pathlib import Path
import unittest
import subprocess
import sys
import tempfile
import json
from unittest.mock import patch
from types import SimpleNamespace

spec = importlib.util.spec_from_file_location("apple_app", Path(__file__).with_name("test-apple-app.py"))
app = importlib.util.module_from_spec(spec)
spec.loader.exec_module(app)


class ReceiptTests(unittest.TestCase):
    def setUp(self):
        self.report = {"schema": 1, "platform": "ios", "run_id": "this-run", "passed": True, "error": "",
                       "checks": [{"system": name, "passed": True, "error": ""} for name in ["nes", "snes"]]}

    def test_bundle_notice_audit_checks_the_actual_package(self):
        (app.ROOT / ".build").mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="bundle-notices-", dir=app.ROOT / ".build") as temporary:
            bundle = Path(temporary)
            notices = bundle / "licenses"
            notices.mkdir(parents=True)
            with self.assertRaisesRegex(RuntimeError, "required dependency notice"):
                app.validate_bundle_notices(bundle, rust_preview=False)
            for filename in app.NATIVE_NOTICES:
                (notices / filename).write_text("fixture notice\n")
            app.validate_bundle_notices(bundle, rust_preview=False)
            for filename in ["Barlow-OFL.txt", "flag-icons-MIT.txt", "flag-icons-attribution.md"]:
                (notices / filename).write_text("fixture notice\n")
            inventory = [{"name": "fixture", "version": "1.0", "notices": ["LICENSE"]}]
            (notices / "rust-packages.json").write_text(json.dumps(inventory))
            with self.assertRaisesRegex(RuntimeError, "Rust dependency notice"):
                app.validate_bundle_notices(bundle)
            directory = notices / "rust/fixture-1.0"
            directory.mkdir(parents=True)
            (directory / "LICENSE").write_text("fixture license\n")
            app.validate_bundle_notices(bundle)
            inventory[0]["notices"] = ["../outside"]
            (notices / "rust-packages.json").write_text(json.dumps(inventory))
            with self.assertRaisesRegex(RuntimeError, "Invalid bundled Rust notice filename"):
                app.validate_bundle_notices(bundle)

    def test_interactive_mode_rejects_incompatible_targets_before_building(self):
        script = Path(__file__).with_name("test-apple-app.py")
        for arguments in [["--platform", "tvos", "--app", "game"],
                          ["--platform", "ios", "--app", "game"],
                          ["--platform", "ios", "--app", "preview", "--build-only"]]:
            result = subprocess.run([sys.executable, str(script), "--interactive", *arguments], capture_output=True, text=True)
            self.assertEqual(result.returncode, 2)
            self.assertIn("--interactive requires", result.stderr)

    def test_interactive_gui_open_uses_selected_xcode_and_handles_missing_gui(self):
        with patch.object(app, "run", return_value=SimpleNamespace(stdout="/mock/Xcode/Contents/Developer\n")), \
                patch.object(Path, "is_dir", return_value=True), \
                patch.object(app.subprocess, "run", return_value=SimpleNamespace(returncode=0)) as opened, \
                patch("builtins.print"):
            self.assertTrue(app.open_interactive_simulator("owned-device"))
            self.assertEqual(opened.call_args.args[0], ["open", "-a", "/mock/Xcode/Contents/Developer/Applications/Simulator.app",
                                                        "--args", "-CurrentDeviceUDID", "owned-device"])
        with patch.object(app, "run", return_value=SimpleNamespace(stdout="/mock/Developer\n")), \
                patch.object(Path, "is_dir", return_value=False), \
                patch.object(app.subprocess, "run", return_value=SimpleNamespace(returncode=1)), \
                patch("builtins.print") as messages:
            self.assertFalse(app.open_interactive_simulator("owned-device"))
            self.assertTrue(any("running headlessly" in str(call) for call in messages.call_args_list))

    def test_xcode_27_uses_device_hub_deep_link(self):
        with patch.object(app, "run", return_value=SimpleNamespace(stdout="/mock/Xcode/Contents/Developer\n")), \
                patch.object(Path, "is_dir", side_effect=[False, True]), \
                patch.object(app.subprocess, "run", return_value=SimpleNamespace(returncode=0)) as opened, \
                patch("builtins.print"):
            self.assertTrue(app.open_interactive_simulator("owned-device"))
            self.assertEqual(opened.call_args.args[0], ["open", "-a", "/mock/Xcode/Contents/Applications/DeviceHub.app",
                                                        "devices://device/open?id=owned-device"])

    def test_transfer_requires_an_explicit_bounded_hsqs_source_before_building(self):
        script = Path(__file__).with_name("test-apple-app.py")
        for arguments in [[], ["--image", "does-not-exist.hsqs"]]:
            result = subprocess.run([sys.executable, str(script), "--platform", "tvos", "--app", "transfer", *arguments], capture_output=True, text=True)
            self.assertEqual(result.returncode, 2)
            self.assertIn("--app transfer requires", result.stderr)

    def test_extra_collection_requires_a_valid_transfer_image_before_building(self):
        script = Path(__file__).with_name("test-apple-app.py")
        for target in ["preview", "transfer"]:
            result = subprocess.run([sys.executable, str(script), "--platform", "tvos", "--app", target,
                                     "--extra-image", "does-not-exist.hsqs"], capture_output=True, text=True)
            self.assertEqual(result.returncode, 2)
            self.assertIn("--extra-image requires", result.stderr)

    def test_core_requires_both_distinct_successful_checks(self):
        self.assertTrue(app.valid_report(self.report, "ios", "core", "this-run"))
        for checks in [[], self.report["checks"][:1], [self.report["checks"][0]] * 2,
                       [{"system": "nes", "passed": False, "error": ""}, self.report["checks"][1]], [None, None]]:
            bad = dict(self.report, checks=checks)
            self.assertFalse(app.valid_report(bad, "ios", "core", "this-run"))

    def test_wrong_run_platform_schema_or_error_is_not_runtime_success(self):
        for field, value in [("run_id", "old"), ("platform", "tvos"), ("schema", True),
                             ("passed", False), ("error", "failed")]:
            bad = copy.deepcopy(self.report)
            bad[field] = value
            self.assertFalse(app.valid_report(bad, "ios", "core", "this-run"))
        self.assertFalse(app.valid_report([], "ios", "core", "this-run"))

    def test_preview_requires_presented_frames_and_nonzero_u64_checksum(self):
        good = dict(self.report, frames=120, checksum="1234")
        self.assertTrue(app.valid_report(good, "ios", "preview", "this-run"))
        for value in [None, "0", "-1", "x", str(2**64), "１２", 42]:
            self.assertFalse(app.valid_report(dict(good, checksum=value), "ios", "preview", "this-run"))
        for value in [True, 0, 2, "120"]:
            self.assertFalse(app.valid_report(dict(good, frames=value), "ios", "preview", "this-run"))

    def test_newer_foldable_only_runtime_does_not_hide_the_conventional_screen(self):
        pro = {"name": "iPhone 18 Pro", "identifier": "pro"}
        duo = {"name": "iPhone Duo", "identifier": "duo"}
        runtimes = [{"identifier": "com.apple.CoreSimulator.SimRuntime.iOS-27-0", "version": "27.0",
                     "isAvailable": True, "supportedDeviceTypes": [pro]},
                    {"identifier": "com.apple.CoreSimulator.SimRuntime.iOS-27-1", "version": "27.1",
                     "isAvailable": True, "supportedDeviceTypes": [duo]}]
        with patch.object(app, "sim_json", side_effect=[{"runtimes": runtimes}, {"devicetypes": [pro, duo]}]):
            self.assertEqual(app.select_runtime("ios"), (runtimes[0]["identifier"], "pro"))

    def test_game_requires_video_audio_bounds_and_lifecycle(self):
        good = dict(self.report, frames=240, checksum="1234", lifecycle_probe_passed=True, game={
            "emulated_frames": 200, "video_changes": 6, "audio_samples": 320000,
            "audio_nonzero": True, "audio_started": True, "max_audio_queue_bytes": 28000,
            "audio_queue_bytes": 12000, "audio_limit_bytes": 38400, "audio_drained_bytes": 100000,
            "max_steps_per_callback": 3, "pauses": 1})
        self.assertTrue(app.valid_report(good, "ios", "game", "this-run"))
        for field, value in [("video_changes", 0), ("audio_samples", 0), ("audio_nonzero", False),
                             ("audio_started", False), ("max_audio_queue_bytes", 38401),
                             ("audio_drained_bytes", 0), ("max_steps_per_callback", 4), ("pauses", 0), ("emulated_frames", True)]:
            bad = copy.deepcopy(good)
            bad["game"][field] = value
            self.assertFalse(app.valid_report(bad, "ios", "game", "this-run"))
        self.assertFalse(app.valid_report(dict(good, lifecycle_probe_passed=False), "ios", "game", "this-run"))

    def test_input_requires_all_event_checks_and_core_output(self):
        good = dict(self.report, frames=200, checksum="1234", input_probe_passed=True, input_checks=12, game={
            "emulated_frames": 180, "video_changes": 6, "audio_samples": 300000, "buttons": 256,
            "audio_nonzero": True, "audio_started": True, "max_audio_queue_bytes": 28000,
            "audio_queue_bytes": 12000, "audio_limit_bytes": 38400, "audio_drained_bytes": 100000,
            "max_steps_per_callback": 3, "pauses": 1})
        self.assertTrue(app.valid_report(good, "ios", "input", "this-run"))
        for field, value in [("input_probe_passed", False), ("input_checks", 11), ("input_checks", True), ("run_id", "old")]:
            self.assertFalse(app.valid_report(dict(good, **{field: value}), "ios", "input", "this-run"))
        for field, value in [("buttons", 0), ("video_changes", 0), ("audio_started", False), ("max_audio_queue_bytes", 38401)]:
            bad = copy.deepcopy(good)
            bad["game"][field] = value
            self.assertFalse(app.valid_report(bad, "ios", "input", "this-run"))


class TransferTests(unittest.TestCase):
    def snapshot(self):
        return dict(schema=1, platform="tvos", run_id="run", instance_id="instance", sequence=0, active=False,
                    connection=dict(port=1234, code="012345"), game={}, transfer=dict(enabled=True, busy=False,
                    message="Ready", received=0, total=0, revision=1, profiles=[]))

    def test_transfer_snapshot_rejects_stale_or_malformed_launch_state(self):
        good = self.snapshot()
        self.assertTrue(app.valid_transfer_snapshot(good, "tvos", "run"))
        for field, value in [("schema", True), ("run_id", "old"), ("platform", "ios"), ("active", 1), ("sequence", True)]:
            self.assertFalse(app.valid_transfer_snapshot(dict(good, **{field:value}), "tvos", "run"))
        for group, field, value in [("connection", "port", True), ("connection", "code", "bad"),
                                    ("connection", "code", "１２３４５６"), ("connection", "code", 123456),
                                    ("connection", "token", "a" * 32),
                                    ("transfer", "busy", 1), ("transfer", "total", 1024**3 + 1),
                                    ("transfer", "received", 1), ("transfer", "profiles", [None])]:
            bad = copy.deepcopy(good); bad[group][field] = value
            self.assertFalse(app.valid_transfer_snapshot(bad, "tvos", "run"))


class TouchTests(unittest.TestCase):
    def test_touch_requires_all_checks_platform_capability_and_live_core(self):
        good = dict(schema=1, platform="ios", run_id="run", passed=True, error="", frames=240, checksum="1234",
                    touch_probe_passed=True, touch_checks=17, touch_ui_supported=True, game={
                        "emulated_frames":180, "video_changes":6, "audio_samples":300000, "buttons":1,
                        "audio_nonzero":True, "audio_started":True, "max_audio_queue_bytes":28000,
                        "audio_queue_bytes":12000, "audio_limit_bytes":38400, "audio_drained_bytes":100000,
                        "max_steps_per_callback":3, "pauses":1})
        self.assertTrue(app.valid_report(good, "ios", "touch", "run"))
        self.assertTrue(app.valid_report(dict(good, platform="tvos", touch_ui_supported=False), "tvos", "touch", "run"))
        for field, value in [("touch_probe_passed", False), ("touch_checks", 16), ("touch_checks", True),
                             ("touch_ui_supported", False), ("touch_ui_supported", 1), ("run_id", "old")]:
            self.assertFalse(app.valid_report(dict(good, **{field:value}), "ios", "touch", "run"))
        for field, value in [("buttons", 0), ("buttons", True), ("video_changes", 0), ("audio_drained_bytes", 0), ("max_audio_queue_bytes", 38401)]:
            bad = copy.deepcopy(good); bad["game"][field] = value
            self.assertFalse(app.valid_report(bad, "ios", "touch", "run"))


class LifecycleTests(unittest.TestCase):
    def snapshot(self, phase, frames=100, callbacks=120, paused=False, backgrounds=0, foregrounds=0, sequence=0):
        return {"schema": 1, "platform": "ios", "run_id": "run", "instance_id": "instance", "phase": phase,
                "callbacks": callbacks, "background_events": backgrounds, "foreground_events": foregrounds,
                "command_sequence": sequence, "resume_frame_delta": 0,
                "game": {"emulated_frames": frames, "paused": paused, "audio_started": not paused,
                         "audio_queue_bytes": 0 if paused else 1000, "buttons": 0, "generation": 7,
                         "presented_frames": 120, "audio_drained_bytes": frames * 100, "max_audio_queue_bytes": 20000,
                         "audio_limit_bytes": 38400, "max_steps_per_callback": 2}}

    def cycle(self):
        return dict(before=self.snapshot("ready"),
                    background=self.snapshot("background", paused=True, backgrounds=1),
                    foreground=self.snapshot("foreground", paused=True, backgrounds=1, foregrounds=1),
                    stable=self.snapshot("status", callbacks=150, paused=True, backgrounds=1, foregrounds=1),
                    resumed=self.snapshot("resumed", callbacks=151, backgrounds=1, foregrounds=1, sequence=1),
                    running=self.snapshot("status", frames=130, callbacks=181, backgrounds=1, foregrounds=1, sequence=1))

    def test_snapshot_rejects_wrong_run_platform_phase_and_invalid_types(self):
        good = self.snapshot("ready")
        self.assertTrue(app.valid_lifecycle_snapshot(good, "ios", "run", "ready"))
        for field, value in [("schema", True), ("run_id", "old"), ("platform", "tvos"),
                             ("phase", "foreground"), ("instance_id", ""), ("callbacks", True)]:
            self.assertFalse(app.valid_lifecycle_snapshot(dict(good, **{field: value}), "ios", "run", "ready"))
        for field, value in [("paused", 1), ("audio_started", 0), ("generation", 0), ("audio_queue_bytes", 38401)]:
            bad = copy.deepcopy(good)
            bad["game"][field] = value
            self.assertFalse(app.valid_lifecycle_snapshot(bad, "ios", "run", "ready"))

    def test_actual_pause_and_explicit_resume_cycle(self):
        app.verify_lifecycle_cycle(**self.cycle(), cycle=1)

    def test_restart_auto_resume_queued_audio_and_catchup_are_failures(self):
        mutations = [("foreground", "instance_id", "new"), ("stable", "callbacks", 149),
                     ("foreground", "foreground_events", 0), ("resumed", "resume_frame_delta", 4)]
        for phase, field, value in mutations:
            bad = self.cycle()
            bad[phase][field] = value
            with self.assertRaises(RuntimeError):
                app.verify_lifecycle_cycle(**bad, cycle=1)


        for phase, field, value in [("foreground", "emulated_frames", 101), ("stable", "paused", False),
                                    ("background", "audio_queue_bytes", 1), ("background", "buttons", 1),
                                    ("resumed", "emulated_frames", 104), ("running", "audio_started", False),
                                    ("running", "audio_drained_bytes", 10000)]:
            bad = self.cycle()
            bad[phase]["game"][field] = value
            with self.assertRaises(RuntimeError):
                app.verify_lifecycle_cycle(**bad, cycle=1)


class InterruptionTests(unittest.TestCase):
    snapshot = LifecycleTests.snapshot

    def interruption_cycle(self):
        before = self.snapshot("ready")
        before.update(interruption_began=0, interruption_ended=0)
        before["game"]["audio_interrupted"] = False
        evidence = {"before": before}
        for phase, sequence, callbacks, interrupted in [
                ("began", 1, 130, True), ("blocked", 2, 131, True),
                ("ended", 3, 132, False), ("stable", 3, 162, False),
                ("resumed", 4, 163, False), ("running", 4, 193, False)]:
            snapshot = self.snapshot("status" if phase in ("stable", "running") else phase,
                                     frames=130 if phase == "running" else 100,
                                     callbacks=callbacks, paused=phase not in ("resumed", "running"), sequence=sequence)
            snapshot.update(interruption_began=1, interruption_ended=1 if sequence >= 3 else 0)
            snapshot["game"]["audio_interrupted"] = interrupted
            evidence[phase] = snapshot
        return evidence

    def test_interruption_cycle_blocks_resume_until_end_then_requires_explicit_resume(self):
        evidence = self.interruption_cycle()
        for snapshot in evidence.values():
            self.assertTrue(app.valid_interruption_snapshot(snapshot, "ios", "run", snapshot["phase"]))
        app.verify_interruption_cycle(**evidence, sequence=1)

    def test_interruption_snapshot_requires_boolean_latch_and_real_counters(self):
        good = self.interruption_cycle()["began"]
        for key, value in [("interruption_began", True), ("interruption_ended", -1), ("run_id", "old")]:
            self.assertFalse(app.valid_interruption_snapshot(dict(good, **{key: value}), "ios", "run", "began"))
        bad = copy.deepcopy(good)
        bad["game"]["audio_interrupted"] = 1
        self.assertFalse(app.valid_interruption_snapshot(bad, "ios", "run", "began"))

    def test_interruption_auto_resume_stale_notifications_and_catchup_fail(self):
        for phase, key, value in [("began", "audio_interrupted", False), ("blocked", "paused", False),
                                  ("ended", "paused", False), ("stable", "audio_queue_bytes", 1),
                                  ("running", "audio_started", False), ("resumed", "emulated_frames", 104)]:
            bad = self.interruption_cycle()
            bad[phase]["game"][key] = value
            with self.assertRaises(RuntimeError):
                app.verify_interruption_cycle(**bad, sequence=1)


        for phase, key, value in [("began", "interruption_began", 0), ("ended", "interruption_ended", 0),
                                  ("resumed", "instance_id", "restart"), ("stable", "callbacks", 161)]:
            bad = self.interruption_cycle()
            bad[phase][key] = value
            with self.assertRaises(RuntimeError):
                app.verify_interruption_cycle(**bad, sequence=1)


class ImportTests(unittest.TestCase):
    def flow(self):
        base = {"schema": 1, "platform": "ios", "run_id": "run", "instance_id": "same-owner",
                "source_unchanged": True, "save_persisted": False, "command_sequence": 0, "callbacks": 120,
                "rom": "", "game": {}, "import": {"selected": 0, "cancelled": 0, "completed": 0,
                    "rejected": 0, "busy": False, "active": False, "error": "", "picker_supported": True,
                    "picker_open": False, "picker_presented": False}}
        evidence = {name: copy.deepcopy(base) for name in ["ready", "picker", "cancelled", "invalid", "loaded",
                                                          "playing", "stopped", "late_cancelled", "reloaded", "replaying"]}
        rom = "/owned/Library/Application Support/CloverRawNES/" + "a" * 64 + "/game.nes"
        for sequence, (name, snapshot) in enumerate(evidence.items()):
            snapshot["command_sequence"] = sequence
            snapshot["callbacks"] += sequence * 30
            if sequence >= 2:
                snapshot["import"]["cancelled"] = 1
            if sequence >= 3:
                snapshot["import"].update(rejected=1, selected=1)
            if sequence >= 4:
                snapshot["rom"] = rom
                snapshot["import"].update(completed=1, selected=2)
            if sequence >= 6:
                snapshot["save_persisted"] = True
            if sequence >= 7:
                snapshot["import"].update(cancelled=2, selected=3)
            if sequence >= 8:
                snapshot["import"].update(completed=2, selected=4)
            if name in ["loaded", "playing", "reloaded", "replaying"]:
                snapshot["import"]["active"] = True
                playing = name in ["playing", "replaying"]
                snapshot["game"] = {"paused": not playing, "emulated_frames": 70 if playing else 0,
                    "audio_samples": 100000 if playing else 0, "audio_started": playing, "audio_nonzero": playing,
                    "audio_drained_bytes": 10000 if playing else 0, "audio_queue_bytes": 10000 if playing else 0,
                    "max_audio_queue_bytes": 20000 if playing else 0, "audio_limit_bytes": 38400,
                    "max_steps_per_callback": 2 if playing else 0, "video_changes": 2 if playing else 0, "generation": 1}
        evidence["picker"]["import"].update(picker_open=True, picker_presented=True)
        return evidence

    def test_import_flow_rejects_invalid_and_stale_results_then_reuses_owned_identity(self):
        evidence = self.flow()
        for snapshot in evidence.values():
            self.assertTrue(app.valid_import_snapshot(snapshot, "ios", "run"))
        app.verify_import_flow(evidence, "ios")

    def test_import_snapshot_rejects_stale_run_source_mutation_and_malformed_state(self):
        good = self.flow()["playing"]
        for field, value in [("schema", True), ("run_id", "old"), ("source_unchanged", False),
                             ("save_persisted", 1), ("game", [])]:
            self.assertFalse(app.valid_import_snapshot(dict(good, **{field: value}), "ios", "run"))
        for field, value in [("audio_started", 1), ("audio_limit_bytes", 40000), ("emulated_frames", True)]:
            bad = copy.deepcopy(good)
            bad["game"][field] = value
            self.assertFalse(app.valid_import_snapshot(bad, "ios", "run"))

    def test_import_cancellation_no_autoplay_persistence_and_restart_faults(self):
        for phase, field, value in [("loaded", "instance_id", "restart"), ("reloaded", "rom", "/wrong/game.nes"),
                                    ("stopped", "save_persisted", False)]:
            bad = self.flow()
            bad[phase][field] = value
            with self.assertRaises(RuntimeError):
                app.verify_import_flow(bad, "ios")
        for phase, field, value in [("cancelled", "selected", 1), ("late_cancelled", "active", True),
                                    ("late_cancelled", "completed", 2)]:
            bad = self.flow()
            bad[phase]["import"][field] = value
            with self.assertRaises(RuntimeError):
                app.verify_import_flow(bad, "ios")
        bad = self.flow()
        bad["loaded"]["game"]["paused"] = False
        with self.assertRaises(RuntimeError):
            app.verify_import_flow(bad, "ios")


class LibraryTests(unittest.TestCase):
    def flow(self):
        imported = ImportTests().flow()
        digest = "a" * 64

        def library(snapshot, count=1, visible=False, cold=False, revision=3, opened=0):
            snapshot = copy.deepcopy(snapshot)
            snapshot["source_exists"] = not cold
            snapshot["source_unchanged"] = not cold
            snapshot["instance_id"] = "cold" if cold else "warm"
            snapshot["import"].update(library_count=count, library_revision=revision, selected_digest=digest if count else "",
                                      scanning=False, library_skipped=0, library_limited=False, opened=opened)
            if cold:
                snapshot["import"].update(selected=0, completed=0, cancelled=0, rejected=0)
            snapshot["library_scene"] = dict(revision=revision, presented_revision=revision,
                entries=[digest] if count else [], selected=digest if count else "", visible=visible,
                checksum="777" if visible and count else "888" if visible else "0")
            return snapshot

        evidence = {"loaded": library(imported["loaded"]), "playing": library(imported["playing"]),
                    "stored": library(imported["stopped"], visible=True, revision=4),
                    "cold": library(imported["stopped"], visible=True, cold=True),
                    "reopened": library(imported["reloaded"], cold=True, opened=1),
                    "replaying": library(imported["replaying"], cold=True, opened=1),
                    "restopped": library(imported["stopped"], visible=True, cold=True, opened=1),
                    "corrupt": library(imported["stopped"], count=0, visible=True, cold=True, opened=1, revision=6),
                    "restored": library(imported["stopped"], visible=True, cold=True, opened=1, revision=7),
                    "restored_open": library(imported["reloaded"], cold=True, opened=2, revision=7),
                    "restored_playing": library(imported["replaying"], cold=True, opened=2, revision=7)}
        evidence["corrupt"]["import"]["library_skipped"] = 1
        evidence["corrupt_open"] = copy.deepcopy(evidence["corrupt"])
        return evidence

    def test_library_cold_restart_without_source_and_corruption_recovery(self):
        evidence = self.flow()
        for snapshot in evidence.values():
            self.assertTrue(app.valid_library_snapshot(snapshot, "ios", "run"))
        app.verify_library_flow(evidence)

    def test_library_requires_real_presented_scene_and_absent_source(self):
        for phase, field, value in [("cold", "instance_id", "warm"), ("cold", "source_exists", True),
                                    ("reopened", "rom", "/wrong/game.nes"), ("reopened", "save_persisted", False)]:
            bad = self.flow(); bad[phase][field] = value
            with self.assertRaises(RuntimeError): app.verify_library_flow(bad)
        for phase, field, value in [("cold", "visible", False), ("cold", "presented_revision", 0),
                                    ("cold", "entries", []), ("cold", "checksum", "0")]:
            bad = self.flow(); bad[phase]["library_scene"][field] = value
            with self.assertRaises(RuntimeError): app.verify_library_flow(bad)
        bad = self.flow(); bad["corrupt_open"]["import"]["opened"] = 2
        with self.assertRaises(RuntimeError): app.verify_library_flow(bad)

    def test_library_snapshot_rejects_duplicates_bad_types_and_malformed_ids(self):
        good = self.flow()["cold"]
        for field, value in [("entries", ["../escape"]), ("revision", True), ("visible", 1),
                             ("entries", ["a" * 64] * 2), ("checksum", "x")]:
            bad = copy.deepcopy(good); bad["library_scene"][field] = value
            self.assertFalse(app.valid_library_snapshot(bad, "ios", "run"))


if __name__ == "__main__":
    unittest.main()
