"""Collect redistribution notices for desktop/Apple packages, never game data."""
import argparse
import json
from pathlib import Path
import shutil
import subprocess


def collect(root: Path, destination: Path, env=None, features="native-app"):
    destination.mkdir(parents=True, exist_ok=True)
    metadata = json.loads(subprocess.check_output([
        "cargo", "metadata", "--manifest-path", str(root / "rust/Cargo.toml"),
        "--format-version", "1", "--features", features, "--locked",
    ], env=env))
    inventory = []
    for package in metadata["packages"]:
        if not package.get("source"):
            continue
        source = Path(package["manifest_path"]).parent
        target = destination / "rust" / f'{package["name"]}-{package["version"]}'
        target.mkdir(parents=True, exist_ok=True)
        notices = []
        for notice in sorted(source.iterdir()):
            if notice.is_file() and notice.name.upper().startswith(("LICENSE", "LICENCE", "COPYING", "NOTICE")):
                shutil.copy2(notice, target / notice.name)
                notices.append(notice.name)
        if not notices:
            raise RuntimeError(f"No redistribution notice found for {package['name']} {package['version']}")
        if package["name"] == "lzma-sys":
            for notice in sorted((source / "xz-5.2").glob("COPYING*")):
                shutil.copy2(notice, target / f"liblzma-{notice.name}")
                notices.append(f"liblzma-{notice.name}")
        entry = {key: package.get(key) for key in ("name", "version", "license", "repository", "source")}
        entry["notices"] = notices
        inventory.append(entry)
    (destination / "rust-packages.json").write_text(json.dumps(inventory, indent=2) + "\n")
    shutil.copy2(root / "rust/assets/fonts/OFL.txt", destination / "Barlow-OFL.txt")
    shutil.copy2(root / "rust/assets/flags/LICENSE.txt", destination / "flag-icons-MIT.txt")
    shutil.copy2(root / "rust/assets/flags/README.md", destination / "flag-icons-attribution.md")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path)
    parser.add_argument("--features", default="native-app")
    args = parser.parse_args()
    collect(Path(__file__).resolve().parents[1], args.destination, features=args.features)


if __name__ == "__main__":
    main()
