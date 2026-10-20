# Rust desktop build and packaging

The desktop product is the Rust SDL3 frontend/importer plus the upstream ares
SNES/Famicom cores. C++ is limited to ares and a small Objective-C++ AppKit
adapter for the macOS menu bar. There is no C++ frontend or image importer in
the build.

## Build and package

```sh
python3 scripts/build-rust-desktop.py
```

This bootstraps the pinned ares/SDL sources, builds those native libraries
with CMake, runs their smoke tests, builds the Rust app, and packages native and
Rust license notices. `scripts/build-portable.py` is retained as a compatibility
alias. Build on the target operating system; packages are not cross-platform.

| Platform | Output | Status |
|---|---|---|
| macOS | `dist/Classic Menu.app` and `dist/rust-desktop/ClassicMenu-macOS.zip` | Locally validated; ad-hoc signed, not notarized or universal |
| Windows | `dist/rust-desktop/windows/` and ZIP | CI build configured; runtime behavior not yet verified |
| Linux | `dist/rust-desktop/linux/` and tar.gz | CI build configured; host graphics/audio libraries may be required |

Requires CMake 3.28+, Rust stable, Python 3, Git, and internet access for the
first pinned-source bootstrap. The scripts do not install system software.
Windows uses a static MSVC runtime. Linux packages do not bundle host graphics,
audio, or file-dialog services. No game image, firmware, ROM, or imported asset
is included in the default packages.

## Validation

The build runs CTest for the ares bridge and (on macOS) the native-menu adapter.
Rust CI additionally runs formatting, Clippy with warnings denied, and all
native-app tests. The desktop packaging script smoke-tests the Rust window and
launcher using SDL's dummy video/audio drivers. For full image-backed checks,
see `docs/rust-migration.md`, `docs/image-import.md`, and
`docs/controller-support.md`.

Builds do not establish audible quality, physical display/VSync behavior,
controller behavior on every device, or runtime support on Windows/Linux.
Physical-device testing remains outstanding.
