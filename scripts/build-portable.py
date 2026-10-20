#!/usr/bin/env python3
"""Compatibility entry point for building the Rust Classic Menu desktop app."""
from pathlib import Path
import subprocess
import sys

root = Path(__file__).resolve().parents[1]
subprocess.run([sys.executable, str(root / "scripts/build-rust-desktop.py"), *sys.argv[1:]],
               cwd=root, check=True)
