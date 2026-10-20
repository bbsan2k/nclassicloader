#!/usr/bin/env python3
"""Prepare locally supplied Clover art/catalog; no ROMs are included by default."""
import argparse
import configparser
import json
import pathlib
import shutil
import uuid
from datetime import datetime
from clover_assets import prepare_skin

root = pathlib.Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--filesystem", type=pathlib.Path, default=root / "analysis/eur")
parser.add_argument("--output", type=pathlib.Path, default=root / ".build/portable-assets")
parser.add_argument("--roms", type=pathlib.Path, help="Optional local compatible ROM directory; filenames must match game codes (.sfc/.smc)")
args = parser.parse_args()
output = args.output.resolve()
output.parent.mkdir(parents=True, exist_ok=True)
assets = output.with_name("." + output.name + ".staging-" + uuid.uuid4().hex)
assets.mkdir(exist_ok=False)
prepare_skin(args.filesystem / "usr/share/ui/snes-eur/resources", assets / "skin")
games = []
for folder in sorted((args.filesystem / "usr/share/games").glob("CLV-*")):
    desktop = folder / (folder.name + ".desktop")
    if not folder.is_dir() or folder.is_symlink() or not desktop.is_file() or desktop.is_symlink():
        continue
    ini = configparser.ConfigParser(interpolation=None)
    ini.read(desktop, encoding="utf-8")
    entry, meta = ini["Desktop Entry"], ini["X-CLOVER Game"]
    code = meta["Code"]
    if code != folder.name or not all(c.isalnum() or c == "-" for c in code):
        raise ValueError(f"Unsafe or mismatched catalog code in {desktop}")
    dest = assets / "games" / code
    dest.mkdir(parents=True, exist_ok=True)
    for suffix in [".png", "_small.png"]:
        file = folder / (code + suffix)
        if file.is_file() and not file.is_symlink():
            shutil.copy2(file, dest / file.name)
    game = dict(id=code, title=entry["Name"], sortTitle=meta.get("SortRawTitle", entry["Name"].lower()),
                publisher=meta.get("SortRawPublisher", ""), releaseDate=meta.get("ReleaseDate", ""),
                players=int(meta.get("Players", 1)), simultaneous=meta.get("Simultaneous", "0") == "1",
                copyright=meta.get("Copyright", ""))
    if args.roms:
        for suffix in [".sfc", ".smc"]:
            rom = args.roms / (code + suffix)
            if rom.is_file() and not rom.is_symlink():
                (assets / "roms").mkdir(exist_ok=True)
                shutil.copy2(rom, assets / "roms" / rom.name)
                game["rom"] = "roms/" + rom.name
                break
    games.append(game)
if not games:
    raise SystemExit("No catalog found; supply the extracted filesystem with --filesystem")
(assets / "catalog.json").write_text(json.dumps({"schema": 1, "games": games}, ensure_ascii=False, indent=2), encoding="utf-8")
if output.exists():
    backup = output.with_name(output.name + ".previous-" + datetime.now().strftime("%Y%m%d-%H%M%S-%f"))
    output.rename(backup)
assets.rename(output)
print(f"Prepared {len(games)} games at {output}; ROM bundling {'explicitly requested' if args.roms else 'disabled'}")
