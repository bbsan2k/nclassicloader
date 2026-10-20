"""Collect redistribution notices for the Rust native-app package, not game data."""
import json
from pathlib import Path
import shutil
import subprocess


def collect(root: Path, destination: Path, env=None):
    destination.mkdir(parents=True, exist_ok=True)
    metadata = json.loads(subprocess.check_output([
        "cargo", "metadata", "--manifest-path", str(root / "rust/Cargo.toml"),
        "--format-version", "1", "--features", "native-app", "--locked",
    ], env=env))
    inventory = []
    for package in metadata["packages"]:
        if not package.get("source"):
            continue
        source = Path(package["manifest_path"]).parent
        target = destination / "rust" / f'{package["name"]}-{package["version"]}'
        target.mkdir(parents=True, exist_ok=True)
        for notice in source.iterdir():
            if notice.is_file() and notice.name.upper().startswith(("LICENSE", "LICENCE", "COPYING", "NOTICE")):
                shutil.copy2(notice, target / notice.name)
        if package["name"] == "lzma-sys":
            for notice in (source / "xz-5.2").glob("COPYING*"):
                shutil.copy2(notice, target / f"liblzma-{notice.name}")
        inventory.append({key: package.get(key) for key in ("name", "version", "license", "repository", "source")})
    (destination / "rust-packages.json").write_text(json.dumps(inventory, indent=2) + "\n")
    shutil.copy2(root / "rust/assets/fonts/OFL.txt", destination / "Barlow-OFL.txt")
    shutil.copy2(root / "rust/assets/flags/LICENSE.txt", destination / "flag-icons-MIT.txt")
    shutil.copy2(root / "rust/assets/flags/README.md", destination / "flag-icons-attribution.md")
