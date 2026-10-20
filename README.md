# Classic Menu — Clover reconstruction

Classic Menu is a Rust desktop frontend and read-only importer for locally
supplied SNES Classic, NES Classic, and Famicom images. The SNES/NES execution
cores remain ares C++ behind a narrow C ABI; the launcher, image/profile import,
menu, controls, settings, audio pacing, saves, and packaging are Rust.

On macOS, launch the packaged app with:

```sh
open "dist/Classic Menu.app"
```

Windows and Linux builds are packaged separately for their native platforms.
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
requires CMake 3.28+, a Rust stable toolchain, Python 3, Git, and internet access
for the first pinned-source bootstrap. It does not install system software.
macOS packages are ad-hoc signed and not notarized; Windows packages are not
installers. Linux may require host graphics/audio libraries.

Useful validation commands:

```sh
cargo fmt --manifest-path rust/Cargo.toml --check
CLOVER_NATIVE_BUILD_DIR="$PWD/.build/native" cargo test --locked \
  --manifest-path rust/Cargo.toml --all-targets --features native-app
CLOVER_NATIVE_BUILD_DIR="$PWD/.build/native" cargo clippy --locked \
  --manifest-path rust/Cargo.toml --all-targets --features native-app -- -D warnings
python3 scripts/test_rust_importer.py
```

The full importer/UI/audio/controller regression scripts and their local-image
requirements are listed in [`docs/rust-migration.md`](docs/rust-migration.md).
Windows/Linux CI builds are configured, but runtime behavior on those platforms
and physical-device behavior remain unverified.

## Licensing and content

The repository's `LICENSE` covers original project contributions only. It does
not grant rights to Nintendo content, extracted assets, game data, or trademarks.
See [`THIRD-PARTY-NOTICES.md`](THIRD-PARTY-NOTICES.md); do not redistribute a
locally asset-enabled build unless you have rights to every included asset.
Repository tracking rules are in [`docs/repository-content.md`](docs/repository-content.md).

Further documentation: [image import](docs/image-import.md), [SFROM conversion](docs/sfrom-import.md),
[ares integration](docs/ares-core-integration.md), [regions](docs/regions.md),
[Clover visual analysis](docs/clover-visuals.md), and [reverse-engineering scope](docs/reverse-engineering.md).
