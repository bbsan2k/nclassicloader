# Portable Clover frontend

**0.5.0 update:** all six supplied regional images and all 21 SFROM conversions
per image are supported, including signal-exact BRR restoration. Super Mario Kart
still requires user DSP1B firmware. GUI resources, fonts, labels and text colors
follow the selected image. See [regions](regions.md) and [audio](sfrom-import.md).
The historical 0.3/0.4 build details and limitations below are retained for context.

**0.4.0 update:** the default package is now an image-driven launcher with no
Nintendo content bundled. See [image import](image-import.md) for the current
workflow, three supported SFROM conversions, verified SMP IPL extraction, and
profile isolation. The pre-extracted asset-bundling path described below now
requires `--legacy-assets`. Core loading also requires a supplied `ipl.rom`;
supported image imports provide it automatically.

## Architecture and packaging

`portable/frontend/` renders a shared 1280×720 SDL3 stage with the extracted
Clover atlas, bitmap fonts, scene measurements, card artwork, and optional WAV
audio. `portable/core/` embeds only the ares SNES core and required components;
the upstream ares desktop application is not built or launched.

`dependencies.lock.json` pins ares, SDL3, nlohmann/json, and stb. Bootstrap
verifies revisions/file hashes and refuses modified dependency repositories.
The application uses static ares/SDL libraries; on Windows the build requests
the static MSVC runtime. OS services are still required. “Self-contained” does
not mean a single executable runs on all three operating systems, nor that
Linux graphics/audio libraries and native file-dialog services are bundled.

Native builds produce a macOS `.app` or a Windows/Linux directory. The macOS
build targets macOS 13+ on the host architecture, is ad-hoc signed, and is not
notarized or universal. CI configuration covers macOS, Windows, and Linux,
without Nintendo assets; those remote jobs have not yet been executed.

## Build and use

```sh
python3 scripts/build-portable.py
ctest --test-dir .build/portable --output-on-failure
python3 scripts/test_portable_assets.py
```

The build prepares `assets/` from `analysis/eur` by default. It includes art,
fonts, music/effects, and a sanitized JSON catalog, not ROMs, console binaries,
Lua modules, or metadata launch commands. Asset preparation uses a fresh
staging directory so a later default build cannot accidentally carry ROMs
from a previous opt-in build. Previous outputs are retained as timestamped
backups. These backups may contain previously opted-in ROMs.

Only redistribute Nintendo material if you have the necessary rights.
No firmware is downloaded. Games needing coprocessor firmware require a
user-selected firmware folder; see the core integration document for names.
SFROM import/conversion remains unsupported.

For code-only distribution:

```sh
python3 scripts/build-portable.py --without-assets
```

Prepare art separately with `scripts/prepare-portable-assets.py`, then launch
the executable with `--assets /path/to/assets`. A fully populated local package
finds its assets relative to itself and needs no development checkout.

Settings, ROM mappings, SRAM, and four suspend slots per catalog entry live in
SDL's OS-specific preference directory (`ClassicMenu/ClassicMenu`). Override
with `--data-dir /path/to/data` for removable-folder use. Paths supplied to
that option are relative to the launch working directory unless absolute.
Mapped ROM paths remain external; optional `--roms` copies compatible files
named by catalog code into the package. Nintendo save states are incompatible.

## Implemented and remaining work

Implemented: circular menu navigation, original sprites/bitmap text, thumbnails,
sorting/history, mouse/keyboard, one SDL gamepad, embedded SNES video/audio,
SRAM persistence, four ares suspend slots, and basic display/settings/help panels.
Suspend files check ROM identity and pinned core revision. Gameplay returns to
the existing core session when selecting the same mapped game.

Not complete: rewind/replay, SFROM conversion, Canoe save compatibility, original
language selection/unlock logic, multiple controllers, configurable bindings,
original panel parity, and all-game coprocessor validation. Slot preview PNGs
are saved but not displayed by the current suspend panel. Some animations are
approximations; physical-console pixel/timing parity is unverified. Sorting
history counts session starts/resumes rather than measuring gameplay duration.
The Swift frontend remains the visual reference and is not replaced in place.

## Local verification

- CTest: menu rendering and ares bridge tests passed (2/2).
- Asset import tests: catalog/no-implicit-ROM and clean rebuild tests passed (2/2).
- Core test uses a self-authored SNES program: video, audio buffers, button input,
  SRAM, state roundtrip, and invalid input/state rejection. Commercial games
  were not used to establish compatibility.
- `scripts/validate-portable.py` relocated the signed package and rendered the
  menu plus 12 homebrew gameplay frames without source-tree asset paths.
- macOS dependency audit found only `/usr/lib` and `/System/Library` dependencies,
  no Homebrew, SDL dylib, external ares, Python, or Swift runtime dependency.

Audit: `analysis/portable-package-audit.json`. Headless smoke tests do not prove
audible playback, physical controller operation, or macOS native dialogs.
Windows/Linux runtime portability is pending actual builds and testing.
