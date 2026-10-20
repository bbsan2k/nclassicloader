#!/usr/bin/env python3
"""Exercise the Rust importer on every supplied region/version image."""
import argparse
import hashlib
import json
import pathlib
import shutil
import subprocess
import tempfile

root = pathlib.Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--rust-executable", type=pathlib.Path,
                    default=root / "rust/target/release/clover-rust")
parser.add_argument("--images", type=pathlib.Path, default=root / "SNES-Mini-Kernels")
args = parser.parse_args()
if not args.rust_executable.is_file():
    parser.error(f"Rust importer not found: {args.rust_executable}; run `cargo build --release` in rust/")

names = [
    "dp-sneseur-release-v2.0.7-0-geb2b275.hsqs",
    "dp-sneseur-release-v2.0.14-0-gd8b65c6.hsqs",
    "dp-snesusa-release-v2.0.7-0-geb2b275.hsqs",
    "dp-snesusa-release-v2.0.13-0-g9dca6c5.hsqs",
    "dp-snesusa-release-v2.0.14-0-gd8b65c6.hsqs",
    "dp-shvc-release-v2.0.12-0-gbff4fb3.hsqs",
]
images = [args.images / name for name in names]
missing = [path for path in images if not path.is_file()]
if missing:
    parser.error("Missing supplied image(s): " + ", ".join(map(str, missing)))

with tempfile.TemporaryDirectory(prefix="rust-importer-audit-", dir=root / ".build") as temporary:
    cache = pathlib.Path(temporary) / "profiles"
    source_hashes = {path: hashlib.sha256(path.read_bytes()).hexdigest() for path in images}
    observed = []
    for path in images:
        run = subprocess.run([str(args.rust_executable.resolve()), "import-image", str(path.resolve()), str(cache)],
                             capture_output=True, text=True, timeout=300)
        if run.returncode:
            raise RuntimeError(f"Rust import failed for {path.name}:\n{run.stderr}")
        manifest = json.loads(run.stdout)
        expected_region = "Japan" if "shvc" in path.name else "USA" if "snesusa" in path.name else "Europe"
        if (manifest["region"] != expected_region or manifest["games"] != 21
                or manifest["playableGames"] != 21 or manifest["externalFirmwareGames"] != 1):
            raise RuntimeError(f"Unexpected imported profile summary for {path.name}: {manifest}")
        profile = cache / f"{manifest['sourceSHA256']}-v{manifest['importerVersion']}"
        catalog = json.loads((profile / "assets/catalog.json").read_text())
        if len(catalog["games"]) != 21:
            raise RuntimeError(f"Incomplete catalog for {path.name}")
        index = json.loads((profile / "assets/skin/languages.json").read_text())
        expected_locales = {"usa_en", "eur_de", "eur_es", "eur_fr", "eur_it", "eur_nl", "eur_pt", "eur_ru", "jpn_ja"}
        if set(index["locales"]) != expected_locales:
            raise RuntimeError(f"Incomplete language import for {path.name}: {index}")
        for locale in index["locales"]:
            relative = f"assets/skin/languages/{locale}.json"
            table = json.loads((profile / relative).read_text())
            if table["locale"] != locale or len(table["strings"]) != 95 or relative not in manifest["files"]:
                raise RuntimeError(f"Invalid or unhashed language table: {path.name}: {locale}")
        blocked = [game for game in catalog["games"] if game.get("firmwareRequired")]
        if len(blocked) != 1 or blocked[0]["id"] not in ("CLV-P-SAAFE", "CLV-P-VAAFJ", "CLV-P-VAAFE"):
            raise RuntimeError(f"Unexpected firmware-required game for {path.name}: {blocked}")
        observed.append((manifest["region"], manifest["version"], manifest["sourceSHA256"], profile))
        print(f"{path.name}: {manifest['region']} / {manifest['version']}; 21 games converted; {len(index['locales'])} languages")

    profiles = subprocess.run([str(args.rust_executable.resolve()), "list-profiles", str(cache)],
                              capture_output=True, text=True, check=True)
    listed = json.loads(profiles.stdout)
    if len(listed) != len(images):
        raise RuntimeError(f"Expected {len(images)} verified profiles, found {len(listed)}")
    first = observed[0][3] / "profile.json"
    before = first.stat().st_mtime_ns
    again = subprocess.run([str(args.rust_executable.resolve()), "import-image", str(images[0].resolve()), str(cache)],
                           capture_output=True, text=True, check=True, timeout=300)
    if first.stat().st_mtime_ns != before:
        raise RuntimeError("Verified cache was unnecessarily rewritten")
    if "Using verified cache" not in again.stderr:
        raise RuntimeError("Repeated import did not report verified cache reuse")
    migration_cache = pathlib.Path(temporary) / "migration"
    old = migration_cache / f"{observed[0][2]}-v4"
    shutil.copytree(observed[0][3], old)
    legacy = json.loads((old / "profile.json").read_text())
    legacy["importerVersion"] = 4
    for relative in list(legacy["files"]):
        if relative.startswith("assets/skin/languages/") or relative == "assets/skin/languages.json":
            (old / relative).unlink()
            del legacy["files"][relative]
    (old / "profile.json").write_text(json.dumps(legacy))
    (old / "data/preferences.json").write_text(json.dumps({"selected": "CLV-P-SAAAE", "language": "eur_de"}))
    (old / "data/saves").mkdir(exist_ok=True)
    (old / "data/saves/migration-test.state").write_bytes(b"preserve user state")
    subprocess.run([str(args.rust_executable.resolve()), "import-image", str(images[0].resolve()), str(migration_cache)],
                   capture_output=True, text=True, check=True, timeout=300)
    new = migration_cache / observed[0][3].name
    if ((new / "data/saves/migration-test.state").read_bytes() != b"preserve user state"
            or json.loads((new / "data/preferences.json").read_text())["language"] != "eur_de"
            or not (old / "data/saves/migration-test.state").is_file()):
        raise RuntimeError("v4 to v5 migration lost settings, saves, or original data")
    upgraded = subprocess.run([str(args.rust_executable.resolve()), "list-profiles", str(migration_cache)],
                              capture_output=True, text=True, check=True)
    if len(json.loads(upgraded.stdout)) != 1:
        raise RuntimeError("Launcher did not prefer v5 over duplicate v4 profile")
    print("v4 to v5 settings/save preservation and profile upgrade selection passed")
    for path, old_hash in source_hashes.items():
        if hashlib.sha256(path.read_bytes()).hexdigest() != old_hash:
            raise RuntimeError(f"Rust import modified source image {path}")
    print(f"Cache integrity/reuse and source immutability passed for {len(images)} profiles")
