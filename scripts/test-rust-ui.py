#!/usr/bin/env python3
"""Headless UI snapshots and tooltip-cap regression (requires ImageMagick)."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile


def main():
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--assets", type=Path, required=True, help="Prepared European profile assets")
    parser.add_argument("--executable", type=Path, default=root / "rust/target/release/clover-rust")
    parser.add_argument("--output", type=Path, default=root / ".build/rust-ui-regression")
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ, SDL_VIDEODRIVER="dummy", SDL_AUDIODRIVER="dummy")
    with tempfile.TemporaryDirectory(prefix="clover-ui-", dir=args.output) as temporary:
        data = Path(temporary)
        (data / "preferences.json").write_text(json.dumps({"motion": False, "sound": False}))
        base = [str(args.executable.resolve()), "run", "--assets", str(args.assets.resolve()),
                "--data-dir", str(data), "--frames", "2"]
        for index in range(5):
            subprocess.run(base + ["--preview-hover", f"toolbar:{index}", "--screenshot",
                                  str(args.output / f"tooltip-{index}.png")], env=env, check=True)
        for panel in ("toolbar:0", "settings", "emulator-settings", "help", "resume", "profiles", "toolbar:2", "toolbar:3", "toolbar:4"):
            subprocess.run(base + ["--preview-panel", panel, "--screenshot",
                                  str(args.output / f"panel-{panel.replace(':', '-')}.png")], env=env, check=True)
        subprocess.run([str(args.executable.resolve()), "run", "--data-dir", str(data),
                        "--launcher-preview", str(args.output / "launcher.png")], env=env, check=True)
        for system in ("nes", "famicom", "snes"):
            subprocess.run([str(args.executable.resolve()), "run", "--data-dir", str(data),
                            "--launcher-preview", str(args.output / f"launcher-{system}.png")],
                           env=dict(env, CLOVER_LAUNCHER_PREVIEW_SYSTEM=system,
                                    CLOVER_LAUNCHER_PREVIEW_FILTER="SNES" if system == "snes" else "NES"), check=True)
        # With supplied source folders, assert the embedded flag SVGs render on
        # SDL's software renderer (rather than only validating their parser).
        if (root / "SNES-Mini-Kernels/dp-sneseur-release-v2.0.14-0-gd8b65c6.hsqs").is_file():
            flag = subprocess.check_output(["magick", str(args.output / "launcher-snes.png"),
                                            "-crop", "32x24+286+286", "+repage", "-depth", "8", "RGB:-"])
            gold_pixels = sum(flag[i] > 180 and flag[i + 1] > 120 and flag[i + 2] < 80
                              for i in range(0, len(flag), 3))
            if gold_pixels < 12:
                raise RuntimeError("EU flag asset did not render its star circle on the software renderer")
        if (root / "NES-Mini-Kernels/dp-nes-release-v1.0.2-0-g99e37e1.hsqs").is_file():
            canton = subprocess.check_output(["magick", str(args.output / "launcher-nes.png"),
                                              "-crop", "32x24+286+286", "+repage", "-depth", "8", "RGB:-"])
            white_pixels = sum(min(canton[i:i+3]) > 245 for i in range(0, len(canton), 3))
            if white_pixels < 12:
                raise RuntimeError("US flag asset did not render its stripes/stars on the software renderer")
        for system in ("ALL", "NES", "SNES"):
            subprocess.run([str(args.executable.resolve()), "run", "--data-dir", str(data),
                            "--launcher-preview", str(args.output / f"launcher-filter-{system.lower()}.png")],
                           env=dict(env, CLOVER_LAUNCHER_PREVIEW_FILTER=system), check=True)
        subprocess.run([str(args.executable.resolve()), "run", "--data-dir", str(data),
                        "--launcher-preview", str(args.output / "launcher-controller.png")],
                       env=dict(env, CLOVER_LAUNCHER_PREVIEW_CONTROLLER="1"), check=True)
        for state in ("empty", "busy", "error"):
            subprocess.run([str(args.executable.resolve()), "run", "--data-dir", str(data),
                            "--launcher-preview", str(args.output / f"launcher-{state}.png")],
                           env=dict(env, CLOVER_LAUNCHER_PREVIEW_STATE=state), check=True)
        # Options bubble uses the stock 144px body. Compare just its white border,
        # not the moving wallpaper visible through transparent pixels around it.
        for name, x, flop in [("left", 460, []), ("right", 616, ["-flop"])]:
            subprocess.run(["magick", str(args.output / "tooltip-1.png"), "-crop", f"12x66+{x}+89",
                            "+repage", *flop, "-colorspace", "Gray", "-threshold", "90%",
                            str(data / f"{name}.png")], check=True)
        subprocess.run(["compare", "-metric", "AE", str(data / "left.png"),
                        str(data / "right.png"), "null:"], check=True)
        theme = json.loads((args.assets / "skin/theme.json").read_text())
        if theme.get("system") == "nes":
            caption = subprocess.check_output(["magick", str(args.output / "tooltip-1.png"),
                                              "-crop", "112x28+488+108", "+repage", "-depth", "8", "RGB:-"])
            if not any(caption[i:i+3] == bytes([0, 0, 0]) for i in range(0, len(caption), 3)):
                raise RuntimeError("NES tooltip has no dark text on its white caption")
        index_path = args.assets / "skin/languages.json"
        if index_path.is_file():
            locales = json.loads(index_path.read_text())["locales"]
            for locale in locales:
                for panel in ([], ["--preview-panel", "toolbar:2"]):
                    suffix = "-selector" if panel else ""
                    subprocess.run(base + ["--language", locale, *panel, "--screenshot",
                                           str(args.output / f"language-{locale}{suffix}.png")], env=env, check=True)
                if json.loads((data / "preferences.json").read_text())["language"] != locale:
                    raise RuntimeError(f"Language preference not saved: {locale}")
            subprocess.run(base + ["--screenshot", str(args.output / "language-restart.png")], env=env, check=True)
            if json.loads((data / "preferences.json").read_text())["language"] != locales[-1]:
                raise RuntimeError("Language preference not restored on restart")
            print(f"All {len(locales)} languages render and survive profile restart")
    print(f"Tooltip caps mirror correctly; toolbar, menu and launcher snapshots saved to {args.output}")


if __name__ == "__main__":
    main()
