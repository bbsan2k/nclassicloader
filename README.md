# Classic Menu — Clover reconstruction

## Image-driven SDL3 + ares build (0.5.0)

The new C++ frontend embeds ares's SNES core and statically links SDL3. It no
longer needs a separate emulator, Python, Swift, or downloaded libraries at
runtime. The macOS package has been built and tested; Windows/Linux build jobs
are provided but have not yet been run. Each OS needs its own native build.

```sh
open "dist/portable/Classic Menu.app"
```

On first launch, choose a supplied **`.hsqs` or `.tar.gz` filesystem**
in the image library. A boot `.img` can resolve a matching local companion;
the boot image itself has no games. Artwork, fonts, music, metadata, verified
SMP boot firmware, and supported ROMs are imported into a hash-checked cache.
Subsequent launches reuse it even when the source image is disconnected.
Press **F2**, or use Settings, to import/switch profiles.

On macOS, **Library → Image Library / Switch Version…** (⌘⇧I) opens the same
selector; **Library → Import Image…** (⌘O) opens the picker. The active version
appears in the window title. 0.4.1 restores these native menus, preserves cover
and thumbnail proportions, and adds a VSync-paced fixed-resolution rendering path.

**All six supplied images are supported:** Europe 2.0.7/2.0.14, USA
2.0.7/2.0.13/2.0.14, and Japan 2.0.12. Switching images changes the actual
console artwork, wallpaper, catalog, box-art proportions, bitmap fonts, labels,
and regional text colors. Japan uses its white theme and Japanese title/HUD text.
Custom emulator/import controls and native macOS menus remain English.

**All 21 ROMs per image now convert.** BRR restoration reproduces the supplied
PCM and recorded loop histories with zero sample differences; this is not a
claim of byte-identical retail ROM recovery or complete gameplay compatibility.
**Super Mario Kart needs separately supplied DSP1B firmware** for ares; it is
not downloaded or guessed from the image. The other **20 games per image passed
startup audio/video checks** (600 frames, or 1800 for quiet intros).
See [audio restoration](docs/sfrom-import.md),
[regional menus](docs/regions.md), and [image import](docs/image-import.md).
Older caches are upgraded alongside the originals, preserving settings and saves.

You can still map your own compatible `.sfc`/`.smc` ROM in Settings or drag one
onto the menu. Keyboard gameplay: Z=B, X=A,
A=Y, S=X, Q=L, W=R, arrows=D-pad, Return=Start, Backspace=Select. Escape saves
the current suspend slot and returns to the menu; F5 saves and F8 loads.
SDL gamepads are supported (one controller; Guide returns to the menu).

Build the self-contained importer and frontend (no Nintendo content required):

```sh
python3 scripts/build-portable.py
```

Requires CMake 3.28+, a C/C++20 toolchain, Git, Python 3, and internet access for
the first pinned-source bootstrap. No system software is installed by the
script. Existing packages are backed up before replacement. The default package
contains code and licenses only; image content is imported locally at runtime.
`--legacy-assets` optionally bundles the old pre-extracted catalog/art using
`--filesystem`; `--roms` is only for that legacy bundler.

See [portable build details](docs/portable-build.md) and
[ares integration](docs/ares-core-integration.md) for verification and limits.
The in-progress Rust migration and its verified importer/core boundary are tracked
in [Rust migration status](docs/rust-migration.md); the packaged UI remains the
existing build until the Rust app passes end-to-end rendering, input, audio, and
packaging validation.
The original macOS implementation remains available, as documented below.

### Opt-in Rust frontend preview

The Rust SDL frontend can be built against the same native artifacts without
changing the packaged default:

```sh
CLOVER_NATIVE_BUILD_DIR="$PWD/.build/portable" cargo build \
  --manifest-path rust/Cargo.toml --features native-app
./rust/target/debug/clover-rust run
```

Every normal start opens the Game Data Library first: select a cached profile,
a nearby supplied image, **Browse image**, or **Data folder** (a prepared
profile's `assets` directory). You can also pass `--image FILE`, or test a prepared profile with
`--assets DIR --data-dir DIR`. Saves and settings remain separate from the
source image. See the migration notes for current parity and platform limits.

On macOS, build/open the separate Rust app bundle:

```sh
python3 scripts/build-rust-macos.py
open "dist/Classic Menu Rust.app"
```

The bundle contains code and license notices, not console game data. It is
locally ad-hoc signed, not notarized, and does not replace the C++ app.

The Rust menu now imports all nine shipped language tables. Click the **globe**
in the top toolbar to select English, German, French, Spanish, Italian, Dutch,
Portuguese, Russian, or Japanese. The choice is saved per profile. Older cached
profiles upgrade on launch when their source image is available; otherwise use
**Import image** again. Game titles/artwork and custom launcher/emulator text
are not translated by these menu language files.

Rust menu controls: **Up** focuses the top toolbar; **Left/Right** selects its
icons; **Return** opens one; **Down** returns to the games. From the games,
**Down** opens the four-slot suspend-point picker. **Escape/Guide** during a
game pauses into that picker with a screenshot instead of overwriting a slot.
Select a slot with Left/Right and press Return to save the floating point, or
load the slot when no floating point is present. **Down** focuses the picker’s
Save / Start Game / Lock / Delete / Back buttons. **S** saves, **R** loads,
**X** locks/unlocks, **Delete** requests deletion, and **C** continues the current
floating point. Replacing or deleting a point asks for confirmation. Temporary
paused points survive quitting before choosing a slot. F5/F8 remain quick save/load.

The Rust toolbar pages now follow the original full-screen layout. **Options →
Open emulator settings** contains the custom ROM, firmware and image-library
controls. **Display** supports 4:3, Pixel Perfect and an approximate CRT scanline
mode. Decorative frames and console demo/burn-in features are not implemented.
**Legal Notices** supports tabs/scrolling; **Manuals** shows the original QR/URL,
with emulator help available using H.

## Original Swift/AppKit preview (0.2.1)

A native AppKit scene renderer in a SwiftUI app, based on the European SNES Classic 2.0.14
filesystem, investigated with REA and Ghidra. This is **not a complete port of
Nintendo's menu engine or emulator**. See [the findings](docs/reverse-engineering.md)
for evidence and reconstruction boundaries. The second visual pass restores
Clover's shipped art rather than substituting macOS widgets:

![Locally rendered Clover menu](analysis/clover-menu-preview.png)

## Run

The locally built app is `dist/Classic Menu.app`:

```sh
open "dist/Classic Menu.app"
```

Browse with the mouse or left/right arrow keys. Return starts the selected game;
Tab cycles sorting, Up opens settings, Down explains suspend-point support, and
Escape closes a panel. Settings panels support Up/Down and Return. Press M or use
Settings to toggle the original menu music and effects (off initially).
The Library menu lets you choose a macOS SNES emulator and map each game to a
compatible `.sfc`/`.smc` ROM. Nothing is executed from the console metadata.

**No ROMs or emulator binaries are bundled in the app.** The downloaded console
ROM format (`.sfrom`) is not converted or launched. Choose a compatible ROM and
emulator yourself. Successful macOS document-open requests are counted as
launches, not verified gameplay sessions.

## Build and test

Requires macOS 13+, Xcode command-line tools, Swift 5.9+, and Python 3.

```sh
python3 scripts/clover_assets.py
swift test
python3 scripts/test_clover_assets.py
python3 scripts/build-app.py --backup-existing
```

The build script expects extracted metadata and artwork at
`analysis/eur/usr/share/games`; override with `--games /path/to/usr/share/games`.
The original UI resources default to `analysis/eur/usr/share/ui/snes-eur/resources`;
override with `--ui`. The build copies metadata, box art, and the selected visual/audio
assets, and applies a local ad-hoc signature. No console executables or scripts are
bundled. `--backup-existing` preserves the previous app under a timestamped name;
without it the build refuses to overwrite an existing app. The result targets the build host's
architecture; it is not notarized or a universal binary.

## Implemented

- All 21 European catalog entries and local box art.
- Carousel selection, thumbnail navigation, and keyboard shortcuts.
- Original neon-controller wallpaper, grey console bars, blue/cyan card art,
  controller prompts, toolbar icons, and decoded stock bitmap fonts.
- A letterboxed 1280×720 stage, circular carousel motion, eased cursor movement,
  and a recovered cursor tint pulse. Optional original music/sound effects.
- Title, publisher, release date, players, recent-launch, and launch-count sorting.
- Persistent selected game, sort order, ROM mappings, and local launch history.
- Explicit external macOS emulator integration.

## Not implemented / not verified

- Pixel-identical rendering, full original animation/state behavior, controller input,
  language selection, and exact original sort comparator semantics.
- Console MCP protocol, SFROM conversion, Canoe emulation, suspension points,
  rewind/replay, SRAM interoperability, and game-unlock conditions. The preview
  displays all catalog entries, including Star Fox 2, without original gating.
- Console runtime parity and visual comparison against physical hardware.

Settings are stored in the app's macOS UserDefaults domain. Original images and
the cloned repository are not modified. Extracted assets remain third-party
material; a locally buildable app does not establish redistribution rights.

## Visual analysis and previews

See [Clover visual reconstruction](docs/clover-visuals.md) for decoded asset
formats, recovered scene measurements, bytecode findings, and approximations.

Render the app's own stage to a PNG (no screen-capture permission needed):

```sh
"dist/Classic Menu.app/Contents/MacOS/ClassicMenu" \
  --render-preview analysis/clover-menu-preview.png --game CLV-P-SAAAE
```

The text-layout follow-up fixes HUD font/spacing, tooltip offsets, button-label
centering, and fallback glyph baselines. The emulator assessment recommends an
existing core rather than a Canoe binary port; see
[Canoe versus a native core](docs/canoe-port-feasibility.md). That assessment
predates the portable ares integration; this older Swift app still uses an external emulator.

Rendering tests additionally use decoded assets at `analysis/clover-skin/`; they
skip when that local investigation directory has not been generated. Decoder
tests validate source font lengths, glyph bounds, and scene coordinates without
needing any third-party Python packages.
