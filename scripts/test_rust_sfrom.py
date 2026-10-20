#!/usr/bin/env python3
"""Check that standalone Rust SFROM conversion matches Rust profile imports."""
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
parser.add_argument("--rust-executable", type=pathlib.Path,
                    default=root / "rust/target/release/clover-rust")
parser.add_argument("--images", type=pathlib.Path, default=root / "SNES-Mini-Kernels")
parser.add_argument("--jobs", type=int, default=4)
args = parser.parse_args()
if not args.rust_executable.is_file():
    parser.error(f"Rust converter not found: {args.rust_executable}; build the native-app feature first")

work = pathlib.Path(tempfile.mkdtemp(prefix="rust-sfrom-roundtrip-", dir=root / ".build"))
archives = sorted(args.images.glob("*.tar.gz"))
assert len(archives) == 6, f"Expected six supplied archives, found {len(archives)}"
items = []
for archive in archives:
    cache = work / (archive.name[:-7] + "-cache")
    imported = subprocess.run(
        [str(args.rust_executable.resolve()), "import-image", str(archive.resolve()), str(cache)],
        capture_output=True, text=True, timeout=300)
    if imported.returncode:
        raise RuntimeError(f"Rust profile import failed for {archive.name}:\n{imported.stderr}")
    manifest = json.loads(imported.stdout)
    profile = cache / f"{manifest['sourceSHA256']}-v{manifest['importerVersion']}"
    catalog = json.loads((profile / "assets/catalog.json").read_text())
    expected_roms = {game["id"]: profile / "assets" / game["rom"]
                     for game in catalog["games"] if "rom" in game}
    with tarfile.open(archive) as image:
        for member in image.getmembers():
            if not (member.isfile() and member.name.startswith("usr/share/games/")
                    and member.name.endswith(".sfrom")):
                continue
            code = pathlib.PurePosixPath(member.name).stem
            stream = image.extractfile(member)
            if stream is None or code not in expected_roms:
                raise RuntimeError(f"Missing SFROM/catalog entry {code} in {archive.name}")
            items.append((archive.name, code, stream.read(), expected_roms[code]))


def convert(item):
    archive, code, payload, expected_path = item
    stem = f"{archive}-{code}"
    source = work / f"{stem}.sfrom"
    output = work / f"{stem}.sfc"
    source.write_bytes(payload)
    result = subprocess.run(
        [str(args.rust_executable.resolve()), "convert-sfrom", str(source), str(output)],
        capture_output=True, text=True, timeout=90)
    if result.returncode:
        return {"image": archive, "game": code, "passed": False, "error": result.stderr.strip()}
    actual = output.read_bytes()
    expected = expected_path.read_bytes()
    metadata = json.loads(result.stdout)
    return {"image": archive, "game": code, "passed": actual == expected,
            "conversion": metadata["conversion"],
            "rom_sha256": hashlib.sha256(actual).hexdigest()}


with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as executor:
    results = list(executor.map(convert, items))
report = {"converter": "Rust", "oracle": "Rust imported profile output",
          "supplied_archives": len(archives), "roms": len(results),
          "byte_identical": sum(row["passed"] for row in results),
          "failures": [row for row in results if not row["passed"]], "artifacts": str(work)}
destination = root / "analysis/rust-sfrom-roundtrip.json"
destination.parent.mkdir(parents=True, exist_ok=True)
destination.write_text(json.dumps(report, indent=2))
print(json.dumps(report, indent=2))
if not results or report["byte_identical"] != report["roms"]:
    raise SystemExit(f"Standalone SFROM conversion differs from imported profile output; see {destination}")
