#!/usr/bin/env python3
"""Compare the Rust SFROM port byte-for-byte with imported stock-ROM fixtures."""
import argparse
import concurrent.futures
import hashlib
import json
import pathlib
import subprocess
import tarfile
import tempfile

root = pathlib.Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--legacy-executable", type=pathlib.Path,
                    default=root / ".build/portable/ClassicMenu.app/Contents/MacOS/ClassicMenu",
                    help="Current C++ importer used as the transition oracle")
parser.add_argument("--rust-executable", type=pathlib.Path,
                    default=root / "rust/target/release/clover-rust")
parser.add_argument("--images", type=pathlib.Path, default=root / "SNES-Mini-Kernels")
parser.add_argument("--jobs", type=int, default=4)
args = parser.parse_args()
if not args.rust_executable.is_file():
    parser.error(f"Rust converter not found: {args.rust_executable}; run `cargo build --release` in rust/")
work = pathlib.Path(tempfile.mkdtemp(prefix="rust-sfrom-parity-", dir=root / ".build"))
archives = sorted(args.images.glob("*.tar.gz"))
assert len(archives) == 6, f"Expected six supplied archives, found {len(archives)}"
items = []
for archive in archives:
    data = work / (archive.name[:-7] + "-data")
    imported = subprocess.run([str(args.legacy_executable.resolve()), "--import-image", str(archive.resolve()),
                               "--data-dir", str(data)], capture_output=True, text=True, timeout=300)
    if imported.returncode:
        raise RuntimeError(f"Legacy image import failed for {archive.name}: {imported.stdout}\n{imported.stderr}")
    profile_directories = list((data / "profiles").glob("*/profile.json"))
    if len(profile_directories) != 1:
        raise RuntimeError(f"Expected one imported profile for {archive.name}, got {len(profile_directories)}")
    manifest = json.loads(profile_directories[0].read_text())
    profile = data / "profiles" / f"{manifest['sourceSHA256']}-v{manifest['importerVersion']}"
    catalog = json.loads((profile / "assets/catalog.json").read_text())
    expected_roms = {game["id"]: profile / "assets" / game["rom"] for game in catalog["games"] if "rom" in game}
    with tarfile.open(archive) as image:
        for member in image.getmembers():
            if not (member.isfile() and member.name.startswith("usr/share/games/") and member.name.endswith(".sfrom")):
                continue
            code = pathlib.PurePosixPath(member.name).stem
            items.append((archive.name, code, image.extractfile(member).read(), expected_roms[code]))


def convert(item):
    archive, code, payload, expected_path = item
    source = work / f"{archive}-{code}.sfrom"
    output = work / f"{archive}-{code}.sfc"
    source.write_bytes(payload)
    result = subprocess.run([str(args.rust_executable.resolve()), "convert-sfrom", str(source), str(output)],
                            capture_output=True, text=True, timeout=90)
    if result.returncode:
        return {"image": archive, "game": code, "passed": False, "error": result.stderr.strip()}
    actual = output.read_bytes()
    expected = expected_path.read_bytes()
    metadata = json.loads(result.stdout)
    return {"image": archive, "game": code, "passed": actual == expected,
            "status": metadata["conversion"], "rust_rom_sha256": hashlib.sha256(actual).hexdigest(),
            "legacy_rom_sha256": hashlib.sha256(expected).hexdigest()}


with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as executor:
    results = list(executor.map(convert, items))
report = {"converter": "Rust", "oracle": "current local C++ image importer", "supplied_archives": len(archives),
          "roms": len(results), "byte_identical": sum(row["passed"] for row in results),
          "failures": [row for row in results if not row["passed"]], "artifacts": str(work)}
destination = root / "analysis/rust-sfrom-parity.json"
destination.write_text(json.dumps(report, indent=2))
print(json.dumps(report, indent=2))
if report["byte_identical"] != report["roms"]:
    raise SystemExit(f"Rust SFROM port differs from the transition oracle; see {destination}")
