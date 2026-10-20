#!/usr/bin/env python3
"""Rasterize the original source geometry into a PNG-compressed macOS ICNS."""
import argparse
import json
import math
from pathlib import Path
import struct
import zlib


def distance(kind, geometry, x, y):
    """Signed interior distance, matching the SDL runtime icon renderer."""
    if kind == "circle":
        cx, cy, radius = geometry
        return radius - math.hypot(x - cx, y - cy)
    if kind == "rounded":
        left, top, width, height, radius = geometry
        dx = abs(x - left - width / 2) - width / 2 + radius
        dy = abs(y - top - height / 2) - height / 2 + radius
        return radius - math.hypot(max(dx, 0), max(dy, 0)) - min(max(dx, dy), 0)
    if kind == "triangle":
        vertices = list(zip(geometry[::2], geometry[1::2]))
        return min(((bx - ax) * (y - ay) - (by - ay) * (x - ax)) /
                   math.hypot(bx - ax, by - ay)
                   for (ax, ay), (bx, by) in zip(vertices, vertices[1:] + vertices[:1]))
    raise ValueError(f"Unknown icon primitive: {kind}")


def rasterize(design, size):
    pixels = bytearray(size * size * 4)
    # Analytic one-pixel coverage gives smooth edges even in the smallest sizes.
    for shape in design["shapes"]:
        kind, geometry, color = shape["kind"], shape["geometry"], shape["color"]
        for row in range(size):
            y = (row + 0.5) / size
            for column in range(size):
                x = (column + 0.5) / size
                alpha = max(0, min(1, distance(kind, geometry, x, y) * size + 0.5))
                if not alpha:
                    continue
                offset = (row * size + column) * 4
                previous = pixels[offset + 3] / 255
                result = alpha + previous * (1 - alpha)
                for channel in range(3):
                    pixels[offset + channel] = round((color[channel] * alpha +
                        pixels[offset + channel] * previous * (1 - alpha)) / result)
                pixels[offset + 3] = round(result * 255)
    return pixels


def png(size, pixels):
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))
    rows = b"".join(b"\0" + pixels[row * size * 4:(row + 1) * size * 4] for row in range(size))
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)) +
            chunk(b"IDAT", zlib.compress(rows, 9)) + chunk(b"IEND", b""))


def main():
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=root / ".build/icon/ClassicMenu.icns")
    args = parser.parse_args()
    design = json.loads((root / "rust/assets/icon/design.json").read_text())
    entries = []
    for kind, size in [(b"icp4", 16), (b"icp5", 32), (b"icp6", 64),
                       (b"ic07", 128), (b"ic08", 256), (b"ic09", 512), (b"ic10", 1024)]:
        data = png(size, rasterize(design, size))
        entries.append(kind + struct.pack(">I", len(data) + 8) + data)
    body = b"".join(entries)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_bytes(b"icns" + struct.pack(">I", len(body) + 8) + body)
    print(args.output)


if __name__ == "__main__":
    main()
