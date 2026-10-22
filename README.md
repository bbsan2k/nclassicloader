# Classic Menu — Clover reconstruction

[![Build and test](https://github.com/bbsan2k/nclassicloader/actions/workflows/portable.yml/badge.svg?branch=main)](https://github.com/bbsan2k/nclassicloader/actions/workflows/portable.yml)
[![Desktop build targets](https://img.shields.io/badge/build_targets-macOS%20%7C%20Windows%20%7C%20Linux-2088ff)](docs/platform-builds.md)
[![iOS/tvOS library and core checks only](https://img.shields.io/badge/iOS%20%2F%20tvOS-library_%2B_core_checks_only-8e8e93?logo=apple)](docs/platform-builds.md)
[![WebAssembly library check only](https://img.shields.io/badge/WebAssembly-library_check_only-654ff0?logo=webassembly&logoColor=white)](docs/platform-builds.md)
[![GitHub issues](https://img.shields.io/github/issues/bbsan2k/nclassicloader)](https://github.com/bbsan2k/nclassicloader/issues)
[![GitHub stars](https://img.shields.io/github/stars/bbsan2k/nclassicloader?style=flat)](https://github.com/bbsan2k/nclassicloader/stargazers)

[![Rust](https://img.shields.io/badge/frontend-Rust-dea584?logo=rust)](rust/)
[![ares](https://img.shields.io/badge/emulation-ares-7057ff)](https://github.com/ares-emulator/ares)
[![SDL3](https://img.shields.io/badge/platform-SDL3-194b8d)](https://github.com/libsdl-org/SDL)
[![License: MIT](https://img.shields.io/badge/license-MIT-green)](LICENSE)
[![Vibe coded](https://img.shields.io/badge/vibe-coded-ff69b4)](#about-this-project)
[![REA assisted](https://img.shields.io/badge/reverse_engineering-REA-00897b)](docs/reverse-engineering.md)

Classic Menu is a Rust desktop frontend and read-only importer for locally
supplied SNES Classic, NES Classic, and Famicom images. The SNES/NES execution
cores remain ares C++ behind a narrow C ABI; the launcher, image/profile import,
menu, controls, settings, audio pacing, and saves are Rust. Python scripts handle
build orchestration and packaging.

## About this project

**This is a vibe-coded reconstruction**, developed with AI coding assistance.
It used **REA (Reverse Engineer Anything)** for static reverse engineering,
including Ghidra-backed inspection of the original native binaries, alongside
direct inspection of image resources and LuaJIT bytecode. REA informed the
reconstruction; it is **not a runtime or build dependency**. The
[investigation ledger](docs/reverse-engineering.md) distinguishes evidence from
inferences and remaining unknowns.

This is an experimental, unofficial project—not Nintendo's original menu engine,
an exact behavior/pixel-parity claim, or a Nintendo-affiliated product. AI-generated
code and passing tests are not substitutes for review or hardware validation.

## Quick start

Build on the target desktop OS (requires Rust stable, CMake 3.28+, a C/C++
toolchain, Python 3, Git, and Ninja for a fresh default build):

```sh
python3 scripts/build-rust-desktop.py
```

On macOS, launch the packaged app with:

```sh
open "dist/Classic Menu.app"
```

Windows and Linux executables are packaged in `dist/rust-desktop/windows/` and
`dist/rust-desktop/linux/`. Archives for all desktop platforms are written under
`dist/rust-desktop/`. The [GitHub Actions workflow](.github/workflows/portable.yml)
is configured to build/test/package all three; Windows/Linux runtime behavior
remains unverified.

The desktop packages contain application code and license notices, not game
images, firmware, ROMs, or extracted console assets.

## Import and play

On first launch, choose a local **`.hsqs` or `.tar.gz` filesystem** in the image
library. A boot `.img` can resolve a matching local filesystem companion; a boot
image by itself contains no games. The importer reads image data without
executing console programs or scripts and leaves the source unchanged. Imported
artwork, fonts, labels, supported games, and verified SMP boot firmware go into
a hash-checked profile cache. Settings and saves are kept separately in that
profile's data directory. Later launches can use the cache when the source image
is offline. Press **F2** or choose **Settings → Image Library** to switch/import.

All six supplied SNES Classic filesystems are supported: Europe 2.0.7/2.0.14,
USA 2.0.7/2.0.13/2.0.14, and Japan 2.0.12. Their 21 game images are converted
by the Rust importer, including verified signal-exact BRR restoration. This does
not claim byte-identical retail ROM recovery or full gameplay compatibility.
Super Mario Kart still requires user-supplied DSP1B firmware; firmware is never
downloaded or guessed.

The Rust launcher also imports NES Classic 1.0.2/1.0.3/1.0.7 and Famicom Classic
1.0.5 filesystems. It supports the stock iNES 1.0 boards documented in
[`docs/rust-migration.md`](docs/rust-migration.md), using the ares Famicom core.
NES 2.0, unsupported mappers, trainers, and board variants are rejected.
Famicom Disk System conversion/BIOS/disk controls are not implemented; four
Disk System titles remain unavailable. Famicom 1.0.8 and Shonen Jump images need
filesystem companions that are not present in the supplied set.

## Controls and features

- Keyboard and SDL gamepads; one active controller with hotplug support.
- **F11** toggles fullscreen; on macOS, **Control–Command–F** also works.
- **Up** focuses the top toolbar; Left/Right selects a page; Return opens it;
  Down returns to the game list. Down from games opens the four-slot suspend UI.
- **Escape/Guide** pauses to a screenshot-backed suspend picker instead of
  overwriting a slot. Suspend states are ares states, not compatible with
  Nintendo's original Kachikachi/Canoe states.
- Regional language tables, display modes, launcher filters, firmware checks,
  image/profile switching, and per-profile save data are supported.

The original idle/demo animations, decorative frame presets, and CRT shader are
not fully reproduced. Physical display/VSync/audio quality and controller
behavior still need hardware confirmation. See
[`docs/controller-support.md`](docs/controller-support.md) for mappings and
[`docs/platform-builds.md`](docs/platform-builds.md) for platform limits.

## Build and test

Build the default Rust app and its pinned native ares/SDL dependencies on the
target desktop OS:

```sh
python3 scripts/build-rust-desktop.py
```

`python3 scripts/build-portable.py` remains a compatibility alias. The build
needs internet access for its first dependency fetch. The build scripts do not
install system software; host prerequisites must already be installed. See the
[platform build guide](docs/portable-build.md) and
[CI prerequisites](.github/workflows/portable.yml), especially for Linux.
macOS packages are ad-hoc signed and not notarized; Windows packages are not
installers. Linux may require host graphics/audio libraries.

### How ares and SDL are included

They are **not Git submodules**. `scripts/bootstrap-portable.py` fetches separate,
ignored checkouts under `third_party/ares/` and `third_party/SDL/`, pinned by
[`dependencies.lock.json`](dependencies.lock.json). Existing checkouts must match
the pinned revisions and have no local modifications. CMake builds static
libraries; Rust links them through `rust/build.rs`:

- **ares:** SNES/Famicom emulation through `portable/core/ares_c_api.h` and the
  thread-affine Rust wrapper in `rust/src/ares.rs`; no external emulator process.
- **SDL3:** windowing, rendering, audio, gamepads, and file dialogs through Rust
  SDL bindings. A small Objective-C++ AppKit adapter supplies the macOS menu bar.

### Validation

After building the native dependencies, run (POSIX shell syntax):

```sh
cargo fmt --manifest-path rust/Cargo.toml --check
CLOVER_NATIVE_BUILD_DIR="$PWD/.build/native" cargo test --locked \
  --manifest-path rust/Cargo.toml --all-targets --features native-app
CLOVER_NATIVE_BUILD_DIR="$PWD/.build/native" cargo clippy --locked \
  --manifest-path rust/Cargo.toml --all-targets --features native-app -- -D warnings
python3 scripts/test_rust_importer.py
```

The last command requires local SNES images. Portable library tests can run
without those images or the native libraries:

```sh
cargo test --locked --manifest-path rust/Cargo.toml
```

Local macOS checks have passed 44 native-app Rust tests, Clippy, three native
CTest targets, six SNES profile imports, 126 SFROM round trips, and image-backed
NES/Famicom, UI, suspend, audio-buffering, and relocated-package checks. These
are scoped smoke/regression results, not full gameplay or original-console parity.

The full importer/UI/audio/controller regression scripts and their local-image
requirements are listed in [`docs/rust-migration.md`](docs/rust-migration.md).
Windows/Linux CI builds are configured, but runtime behavior on those platforms
and physical-device behavior remain unverified.

## Repository layout

| Path | Purpose |
|---|---|
| [`rust/`](rust/) | Frontend, importer, ares wrapper, tests, and redistributable launcher assets |
| [`portable/core/`](portable/core/) | C++ ares integration, C ABI, and generated-ROM smoke tests |
| [`portable/platform/`](portable/platform/) | Small macOS native-menu adapter |
| [`scripts/`](scripts/) | Dependency bootstrap, builds, packaging, and local-image audits |
| [`docs/`](docs/) | Architecture, controls, reverse-engineering notes, and validation limits |
| `third_party/` | Ignored pinned upstream checkouts fetched during bootstrap |

`.build/`, `dist/`, `analysis/`, local console images, caches, and saves are
deliberately excluded from Git. Do not publish generated console-content artifacts.

### Frontend portability work

The [architecture split](docs/frontend-architecture.md) and
[small-agent task queue](docs/frontend-tasks.md) separate shared behavior from
SDL rendering, platform services and emulation ownership. Controller input,
launcher filtering/selection, stick/repeat, suspend navigation, gameplay masks,
language navigation and timing policies now run without SDL. The remaining
work is staged, not a claim of working iOS, tvOS or browser applications. Luna
handles bounded implementation tasks; Sol reviews high-risk contracts and
verifies integration.

## Licensing and content

The repository's `LICENSE` covers original project contributions only. It does
not grant rights to Nintendo content, extracted assets, game data, or trademarks.
See [`THIRD-PARTY-NOTICES.md`](THIRD-PARTY-NOTICES.md); do not redistribute a
locally asset-enabled build unless you have rights to every included asset.
Repository tracking rules are in [`docs/repository-content.md`](docs/repository-content.md).

Further documentation: [image import](docs/image-import.md), [SFROM conversion](docs/sfrom-import.md),
[ares integration](docs/ares-core-integration.md), [regions](docs/regions.md),
[Clover visual analysis](docs/clover-visuals.md), and [reverse-engineering scope](docs/reverse-engineering.md).
