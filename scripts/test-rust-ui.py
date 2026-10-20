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
    parser.add_argument("--executable", type=Path, default=root / "rust/target/debug/clover-rust")
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
