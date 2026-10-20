#!/usr/bin/env python3
"""Audit local NES/Famicom images without executing extracted console code.

Optional --runtime boots cartridges through ares and checks deterministic state
replay. All output stays under --output; no assets/ROMs are shipped by this test.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess


def main():
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--images", type=Path, default=root / "NES-Mini-Kernels")
    parser.add_argument("--executable", type=Path, default=root / "rust/target/release/clover-rust")
    parser.add_argument("--output", type=Path, default=root / ".build/rust-nes-regression")
    parser.add_argument("--runtime", action="store_true")
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    executable = str(args.executable.resolve())
    cache = args.output.resolve() / "profiles"
    stems = ["dp-nes-release-v1.0.2-0-g99e37e1", "dp-nes-release-v1.0.3-0-gc4c703b",
             "dp-nes-release-v1.0.7-0-g4ea4041", "dp-hvc-release-v1.0.5-0-g2f04d11"]
    env = dict(os.environ, SDL_VIDEODRIVER="dummy", SDL_AUDIODRIVER="dummy")
    profiles = []
    hashes = {}

    def import_image(image):
        hashes[image] = hashlib.sha256(image.read_bytes()).hexdigest()
        run = subprocess.run([executable, "import-image", str(image.resolve()), str(cache)],
                             capture_output=True, text=True, timeout=300, check=True)
        manifest = json.loads(run.stdout)
        profile = cache / f"{manifest['sourceSHA256']}-v{manifest['importerVersion']}"
        assert all(hashlib.sha256((profile / path).read_bytes()).hexdigest() == digest
                   for path, digest in manifest["files"].items())
        return manifest, profile

    for stem in stems:
        image = args.images / f"{stem}.hsqs"
        manifest, profile = import_image(image)
        japanese = "dp-hvc-" in stem
        assert manifest["system"] == "nes" and manifest["games"] == 30
        assert manifest["playableGames"] == (26 if japanese else 30)
        assert manifest["externalFirmwareGames"] == 0 and manifest["smpIPL"] is None
        catalog = json.loads((profile / "assets/catalog.json").read_text())
        blocked = [game for game in catalog["games"] if not game.get("rom")]
        assert len(blocked) == (4 if japanese else 0)
        assert all("Disk System" in game["importError"] for game in blocked)
        assert sum(item.get("status") == "restored-ines-wrapper" for item in manifest["conversions"]) == 1
        languages = json.loads((profile / "assets/skin/languages.json").read_text())
        assert len(languages["locales"]) == (1 if japanese else 8)
        assert languages["default"] == ("jpn_ja" if japanese else "usa_en")
        colors = json.loads((profile / "assets/skin/theme.json").read_text())["colors"]
        assert colors["background"] == ([235, 226, 213, 255] if japanese else [0, 21, 61, 255])
        assert colors["hud"] == ([167, 0, 0, 255] if japanese else [219, 43, 0, 255])
        assert colors["caption"] == [0, 0, 0, 255]
        assert colors["resume_hud"] == ([0, 0, 0, 255] if japanese else [219, 43, 0, 255])
        # Older releases supply the same content in two container formats.
        archive = args.images / f"{stem}.tar.gz"
        if archive.is_file():
            archived, _ = import_image(archive)
            assert manifest["files"] == archived["files"], "SquashFS/tar asset mismatch"
        kernel = args.images / f"kernel-{stem}.img"
        hashes[kernel] = hashlib.sha256(kernel.read_bytes()).hexdigest()
        resolved, _ = import_image(kernel)
        assert resolved["sourceSHA256"] == manifest["sourceSHA256"]
        before = (profile / "profile.json").stat().st_mtime_ns
        import_image(image)
        assert (profile / "profile.json").stat().st_mtime_ns == before, "Cache was rewritten"
        profiles.append((stem, profile))
        print(f"{stem}: {manifest['playableGames']}/30 cartridges; {len(languages['locales'])} languages")

    backup = args.images / "dp-nes-release-v1.0.7-0-g4ea4041.tar"
    hashes[backup] = hashlib.sha256(backup.read_bytes()).hexdigest()
    rejected = subprocess.run([executable, "import-image", str(backup.resolve()), str(cache)],
                              capture_output=True, text=True, timeout=60)
    assert rejected.returncode and "unsupported image" in rejected.stderr
    for stem in ["dp-hvc-release-v1.0.8-0-g32708cb", "dp-hvcj-release-v3.0.1-0-gad315e1"]:
        image = args.images / f"kernel-{stem}.img"
        hashes[image] = hashlib.sha256(image.read_bytes()).hexdigest()
        rejected = subprocess.run([executable, "import-image", str(image.resolve()), str(cache)],
                                  capture_output=True, text=True, timeout=60)
        assert rejected.returncode and "matching" in rejected.stderr

    for stem, profile in profiles:
        data = args.output.resolve() / "ui-data" / stem
        command = [executable, "run", "--assets", str(profile / "assets"), "--data-dir", str(data)]
        for panel in [None, "toolbar:0", "toolbar:2", "resume"]:
            suffix = panel.replace(":", "-") if panel else "home"
            subprocess.run(command + ["--frames", "2", "--screenshot", str(args.output.resolve() / f"{stem}-{suffix}.png")]
                           + (["--preview-panel", panel] if panel else []), env=env, check=True, timeout=60)
        if shutil.which("magick"):
            home = args.output.resolve() / f"{stem}-home.png"
            def pixel(x, y):
                return subprocess.check_output(["magick", str(home), "-crop", f"1x1+{x}+{y}",
                                                "+repage", "-depth", "8", "RGB:-"])
            # The boot/active-menu background is solid, not an always-on dot
            # lattice. This sample is outside the title/cards and HUD.
            assert pixel(100, 160) == bytes([235, 226, 213] if "dp-hvc-" in stem else [0, 21, 61])
            if "dp-hvc-" not in stem:
                assert pixel(500, 40) == bytes([153, 153, 153]), "Missing NES center header layer"
        if args.runtime and ("1.0.7" in stem or "dp-hvc-" in stem):
            subprocess.run(command + ["--audit-games", str(args.output.resolve() / f"{stem}-runtime")],
                           env=env, check=True, timeout=300)
    assert all(hashlib.sha256(path.read_bytes()).hexdigest() == digest for path, digest in hashes.items())
    print("NES imports, hashes, container parity, cache reuse, unsupported images, UI and source immutability passed")


if __name__ == "__main__":
    main()
