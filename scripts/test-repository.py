#!/usr/bin/env python3
"""Content-policy checks must cover ignored, staged and historical payloads."""
import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("repository_check", ROOT / "scripts/check-repository.py")
audit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(audit)


class ContentTests(unittest.TestCase):
    def test_private_paths_and_case_insensitive_content(self):
        for name in ["ROM.NES", "firmware.bin", "capture.png", "game.sav", "secret.p12",
                     ".build/log.txt", "dist/app/info.json", "third_party/SDL/source.c",
                     "private.env.pfx", ".env", ".env.local", "data/Game.app/Info.plist",
                     "App.ipa", "Archive.xcarchive/Info.plist", "Test.xcresult/Info.plist"]:
            with self.subTest(name=name):
                self.assertTrue(audit.content_problems(name, b"private"))
        self.assertFalse(audit.content_problems("src/main.rs", b"fn main() {}\n"))
        self.assertFalse(audit.content_problems(".env.example", b"SETTING=replace-me\n"))

    def test_only_named_licensed_source_fonts_are_allowed(self):
        self.assertFalse(audit.content_problems("rust/assets/fonts/Barlow-Regular.ttf", b"\x00\x01\x00\x00font"))
        self.assertTrue(audit.content_problems("rust/assets/fonts/Barlow-Regular.ttf", b"wrong format"))
        self.assertTrue(audit.content_problems("assets/console.ttf", b"\x00\x01\x00\x00font"))

    def test_credentials_are_reported_without_echoing_values(self):
        tokens = ["ghp_" + "a" * 36, "github_pat_" + "b" * 82,
                  "AKIA" + "C" * 16, "-----BEGIN " + "OPENSSH PRIVATE KEY-----"]
        for token in tokens:
            issues = audit.content_problems("config.txt", token.encode())
            self.assertTrue(issues)
            self.assertNotIn(token, "\n".join(issues))

    def test_binaries_large_files_and_merge_markers(self):
        self.assertTrue(audit.content_problems("data.txt", b"a\x00b"))
        self.assertTrue(audit.content_problems("data.txt", b"\xff"))
        self.assertTrue(audit.content_problems("big.txt", b"x" * (audit.MAX_BYTES + 1)))
        self.assertTrue(audit.content_problems("code.rs", b"<<<<<<< HEAD\n"))

    def test_relative_links_only_resolve_to_publishable_content(self):
        files = {"README.md", "docs/guide.md", "scripts/check.py"}
        good = b"[guide](docs/guide.md#start) [scripts](scripts/) [web](https://example.org) [top](#top)\n"
        self.assertFalse(audit.link_problems("README.md", good, files))
        self.assertFalse(audit.link_problems("docs/guide.md", b"[home](../README.md)", files))
        self.assertTrue(audit.link_problems("README.md", b"[local](.build/result.md)", files))
        self.assertTrue(audit.link_problems("README.md", b"[outside](../README.md)", files))
        self.assertFalse(audit.link_problems("README.md", b"```md\n[example](missing.md)\n```", files))
        token = "ghp_" + "a" * 36
        issues = audit.link_problems("README.md", f"[private]({token})".encode(), files)
        self.assertTrue(issues)
        self.assertNotIn(token, "\n".join(issues))

    def test_external_workflow_actions_require_immutable_references(self):
        path = ".github/workflows/check.yml"
        self.assertTrue(audit.content_problems(path, b"  - uses: actions/checkout@v4\n"))
        self.assertFalse(audit.content_problems(path, ("  - uses: actions/checkout@" + "a" * 40 + " # v4\n").encode()))
        self.assertFalse(audit.content_problems(path, b"  - uses: ./.github/actions/local\n"))
        self.assertFalse(audit.content_problems(path, b"  - uses: actions/checkout@v4\n", check_workflow_pins=False))


class GitTests(unittest.TestCase):
    def setUp(self):
        (ROOT / ".build").mkdir(exist_ok=True)
        self.temporary = tempfile.TemporaryDirectory(prefix="repository-check-", dir=ROOT / ".build")
        self.root = Path(self.temporary.name)
        self.git("init", "-q")
        self.git("config", "user.name", "Repository check fixture")
        self.git("config", "user.email", "fixture@example.invalid")

    def tearDown(self):
        self.temporary.cleanup()

    def git(self, *args):
        return subprocess.check_output(["git", "-C", str(self.root), *args])

    def test_ignored_payload_is_not_read_but_force_added_payload_is_rejected(self):
        (self.root / ".gitignore").write_text("*.hsqs\n")
        (self.root / "private.hsqs").write_bytes(b"not a real console image")
        self.assertFalse(audit.inspect_files(self.root)[1])
        self.git("add", "-f", "private.hsqs")
        self.assertTrue(audit.inspect_files(self.root)[1])

    def test_staged_check_reads_index_not_cleaner_worktree(self):
        path = self.root / "config.txt"
        path.write_text("ghp_" + "a" * 36)
        self.git("add", "config.txt")
        path.write_text("redacted\n")
        self.assertFalse(audit.inspect_files(self.root)[1])
        self.assertTrue(audit.inspect_files(self.root, staged=True)[1])

    def test_historical_removed_payload_is_still_rejected(self):
        path = self.root / "sample.rom"
        path.write_bytes(b"self-authored test payload")
        self.git("add", "sample.rom")
        self.git("commit", "-qm", "fixture")
        self.git("rm", "-q", "sample.rom")
        self.git("commit", "-qm", "remove fixture")
        self.assertFalse(audit.inspect_files(self.root)[1])
        self.assertTrue(audit.inspect_history(self.root)[1])

    def test_shallow_history_does_not_claim_a_complete_audit(self):
        (self.root / "README.md").write_text("fixture\n")
        self.git("add", "README.md")
        self.git("commit", "-qm", "fixture")
        (self.root / ".git/shallow").write_bytes(self.git("rev-parse", "HEAD"))
        count, problems = audit.inspect_history(self.root)
        self.assertEqual(count, 0)
        self.assertTrue(any("shallow" in issue for _, issue in problems))


if __name__ == "__main__":
    unittest.main()
