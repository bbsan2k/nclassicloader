# Rust migration status

The language migration described here is distinct from the ongoing
[frontend architecture split](frontend-architecture.md). See the
[agent-sized task queue](frontend-tasks.md) for the incremental host/service
separation; moving code to Rust did not itself implement mobile or web apps.

The non-emulation application has been ported to Rust. The shipped frontend,
image/profile importer, menu logic, and settings are Rust; the ares execution
cores remain C++ behind the C ABI. The former C++ frontend and importer have
been removed from the product/build.

## Boundary

- The NES/SNES execution cores remain upstream ares C++. `portable/core/ares_c_api.h`
  is the intended narrow boundary: UTF-8 paths, buttons, borrowed ARGB frames and
  stereo float samples, state operations, and error text. The Rust wrapper is
  thread-affine and prevents stepping the machine while a borrowed frame/audio
  view is alive.
- SDL3 remains the cross-platform window/audio/render backend. The Rust app uses
  SDL's public API; a small Objective-C++ adapter only bridges AppKit's macOS
  menu bar to SDL events.
- Image reading, font/skin preparation, profile/cache/migration, SFROM
  conversion, menu/application logic, and their product tests are Rust.
  Nintendo host executables and scripts remain data and are never run.

## Milestones

1. **Converter/core boundary (complete):** Rust SFROM parsing/audio restoration;
   safe ares FFI wrapper; synthetic and historical transition-parity tests.
2. **Image importer (complete for supported formats):** bounded SquashFS and
   gzip/ustar readers, static skin decoding, cache verification, profile-data
   migration, and local profile discovery. All supported SNES/NES/Famicom
   images import to versioned profiles.
3. **Rust SDL application (complete):** the imported-profile menu renders
   Clover's atlas, bitmap fonts, covers, carousel, HUD, settings, and toolbar.
   Keyboard/gamepad input, ROM and firmware pickers, profile import/switching,
   suspend slots, menu/game audio, native macOS menu events, and the ares launch
   path are wired. A fixed-animation screenshot matched the former C++ reference
   pixel-for-pixel before the required missing-firmware lock badge was added.
4. **Build/package cutover (complete):** Rust is the default/only application
   frontend and importer. Native builds retain only ares C++, SDL, and the
   small macOS AppKit adapter. Physical-device and cross-platform runtime checks
   remain outstanding.

## Implemented Rust portion

### Launcher system identity

The launcher now has persistent **All / NES / SNES** filter tabs (mouse,
keyboard 1/2/3 or controller shoulders), with selection retained by entry
identity. NES includes Famicom; SNES includes Super Famicom. Empty filtered
lists still expose image import and prepared-folder selection. Region chips
use sourced, MIT-licensed Japan/US/EU SVG flag assets with semantic region labels
in details; unknown regions use a neutral mark. The original Clover/play icon is generated from
source JSON for the macOS bundle and installed as an SDL window icon.

If a cached playable profile matches a discovered image's source or system,
region, and release version, the launcher shows only the **Ready to play**
profile for that version. The source image row remains available when there is no
matching cached profile.

The library groups NES/Famicom entries together, followed by SNES/Super Famicom,
then unidentified images, with an explicit system badge on each row. Ordering
within a family and last-profile selection are retained. The details panel uses
distinct original vector illustrations: rectangular/two-red-button NES, red/
gold Famicom, and rounded/four-button SNES. No console artwork is bundled.
Cached entries use system/UI metadata (including legacy SNES UI identifiers),
not their hash-addressed directory names. Discovered image names provide only
pre-import identity hints; unknown images use a neutral illustration/title.

Native-app tests: **44/44**. Launcher snapshots cover NES, Famicom, SNES, filters and
empty/busy/error states when the relevant local sources are available.
See `docs/controller-support.md` for controller mapping, hotplug and fullscreen
controls, and `docs/platform-builds.md` for desktop CI and mobile/web limits.

### Presentation and game-audio pacing

Both interactive Rust windows explicitly request SDL renderer VSync. When it is
unavailable, a deadline-based 60 Hz fallback subtracts rendering time; no fixed
sleep is added after a VSync-blocked presentation. Snapshot paths avoid VSync.

Gameplay audio now clocks emulation independently of presentation refresh. The
frontend fills a 60 ms stereo-float input reservoir, with at most six catch-up
core frames per presentation, and starts/resumes playback only after priming.
Pause/state-load clears pause and reset this reservoir. Continuous playback is
not corrected by dropping samples, clearing queues, or resetting the core.
This also avoids tying NES's near-60.10 Hz sample production to a fixed 60 Hz
render loop. If no audio device opens, a bounded nominal-60 Hz timer prevents
high-refresh displays from accelerating emulation.

`scripts/test-rust-audio.py --assets DIR` exercises an actual SDL dummy playback
device through simulated 50/60/120 Hz presentation with brief timing stalls.
NES and SNES checks observed no empty input queues after priming. Dummy-device
wall-clock rate is scheduler-dependent; this is a buffering regression test,
not an exact hardware clock, audible-output test, or proof of tear-free physical
presentation. Physical display/speaker confirmation is still required.

Rust native-app tests: **44/44**; Clippy passes with warnings denied. The existing
NES/SNES suspend and language/UI snapshot regressions also pass.

### NES / Famicom cartridges (introduced in v6; current importer v7)

The Rust launcher discovers `NES-Mini-Kernels/` alongside the SNES image folder.
NES 1.0.2, 1.0.3 and 1.0.7 filesystems import 30 cartridges each. Famicom 1.0.5
imports 26 cartridges and retains four unavailable Disk System titles in the
catalog with explicit errors. The 1.0.7 `.hsqs` uses XZ: block decoding now bounds
liblzma memory to 64 MiB and output to the SquashFS block size, and rejects
truncated/trailing streams. liblzma is statically linked through `xz2`.

The importer recognizes `usr/share/clover-ui/`, regional NES/HVC atlases and
fonts, shared title/copyright fonts, and older `strings.json` language tables
(eight NES languages, Japanese for Famicom). It validates old font whitespace
sentinels without allowing drawable glyphs outside their atlases. The v7 menu
uses the statically recovered original regional palette and solid active-menu
background, not v6's incorrect tinted dot grid. Idle/demo Mario/Luigi animations
are not reproduced. The common carousel, toolbar and screenshot
suspend interfaces reuse locally imported NES artwork; this is not a claim of
complete visual/runtime parity.

Standard iNES 1.0 ROMs are validated for supported stock boards, exact sizes,
header flags and mirroring. Two hash-identified Excitebike carriers contain a
Wii U VC wrapper: the importer copies its bounded cartridge payload, restores
the iNES magic terminator, validates it, and records source/output hashes. It
does not heuristically strip unknown wrappers or execute console code.

The shared C ABI now dispatches `.nes` files to ares Famicom. Enabled mappers:
0, 1, 2, 3, 4, 9 and 184. Standard cartridges need no BIOS. Gamepad/keyboard
inputs use the existing mask (B bit 0, A bit 8); states have a distinct NES build/
ROM identity, while the existing SNES state prefix is retained. Save data stays
profile/game-local. The NES Pixel Perfect presentation uses 256:240 geometry.
Unsupported trainers, NES 2.0, other mappers and board/memory variants fail
closed; this is not a general-purpose all-NES-ROM loader. Imported console
Kachikachi suspend states are not compatible with ares states.

Disk System `.qd` conversion, BIOS loading, disk-side controls and disk writes
remain unimplemented. The 1.0.7 `.tar` in the supplied folder is a save backup,
not a game filesystem. Famicom 1.0.8 and Shonen Jump boot images have no matching
filesystem companions here and are rejected with a matching-filesystem error.
Offline v4/v5/v6 profiles remain usable; reimport upgrades to v7 and preserves
settings and saves in a separate new profile directory.

`docs/nes-menu-analysis.md` records Ghidra/bytecode evidence and the v7 palette,
header backing, card/player-indicator, tooltip, HUD and Display-page corrections.

Validation commands (local image data required):

```sh
python3 scripts/test-rust-nes.py --runtime
ctest --test-dir .build/native -R '^clover_ares(_nes)?_smoke$' --output-on-failure
```

The generated-ROM native smoke tests cover NES frames/audio, A/B/controller
input, state replay, SRAM persistence, PAL startup, mapper initialization and
invalid-cartridge rejection without tracking commercial ROM data. The optional
`run --assets DIR --data-dir DIR --audit-games OUTPUT` audit boots each available
cartridge for 600 frames, sends inputs, checks picture/finite audio samples and
30-frame state replay, using only `OUTPUT/runtime/` for its saves. It is a smoke
audit, not exhaustive gameplay or a subjective audio-fidelity comparison.

Local validation passed for all four supplied full NES/Famicom filesystems,
three gzip/tar companions, source immutability/cache integrity, and all **56**
available cartridges' video/audio/state-replay smoke audits. NES/Famicom/SNES
suspend and toolbar checks pass; all six supplied SNES image imports and v4/v5/v6
data-migration checks still pass. macOS is the validated host; Windows/Linux
builds and physical-controller/display behavior need platform testing.

### Top-menu pages

The five top icons now use the stock full-screen option frame (outer edges at
x70/1210, y42/678; header divider y108), flipped/rotated imported border sprites,
header icon/title, and right-aligned input hints. Display uses the original
three preview images/radio controls; Language uses the two-column radio layout
with a ninth Japanese choice. Legal Notices has scrollable IP/Open Source tabs,
and Manuals displays the image's QR code and manual URL without fetching it.

4:3 and Pixel Perfect display settings persist and apply to gameplay. CRT uses
a basic scanline approximation, not the original console shader. Decorative
frame presets are not imported. Original demo and burn-in controls are visibly
disabled because those console features are not implemented. Emulator/library
settings remain accessible through a separately labeled button in Options;
no destructive factory-reset substitute is offered. H from Manuals opens
emulator control help. This is based on static scene geometry, not a claim of
pixel-perfect runtime equivalence.

Native-app tests pass **44/44**. The CMake test suite covers generated SNES/NES
core smoke tests and, on macOS, native menu event dispatch. The real-core
integration check also covers
display selection, Options/custom-settings back navigation and legal-tab scroll.

### Suspend points and original-style navigation

The Rust menu now has separate game-row, toolbar, and suspend-picker focus.
Keyboard and gamepad share the same navigation handler. Up selects the toolbar,
Left/Right traverses its five icons, Enter opens the icon, and Down returns to
the game row. Down from games opens the suspend picker.

Escape/Guide pauses rather than unloading/auto-overwriting a numbered slot.
A temporary `pending.state` and `pending.png` remain in that game's data folder
until placed into one of four slots. The picker uses Clover's imported shelf,
numbered cards, cursor and lock art. Each numbered ares state has a matching PNG
captured from the real emulator frame. Existing older states without pictures
remain usable and show “Preview unavailable” until replaced.

The picker supports selection, load, save, overwrite/delete confirmation and
per-slot locks. Down focuses its action row; S saves, R loads, X locks, Delete
deletes after confirmation, and C continues the floating point. Paused cores do
not run behind menus, and pending points survive restarting the application.
This is a closer presentation/interaction reconstruction, not native Canoe
state compatibility; original rewind history and move-point mode are not yet
implemented.

The visual follow-up uses the European source scene's `resumemenu_position`
nodes: title Y 182→234 and card row Y 0→69 (52/69 pixels upward on the stage).
The saved-card TextureComponent is 160×120, centered 12 pixels above the card;
the header icon, empty-number badges, lock sizing, and grouped icon/label HUD
now follow that scene rather than the previous generic button row. These are
static layout matches, not a claim of pixel-perfect console runtime parity.
Gamepad Y saves, Start loads, X locks, and Select requests deletion in the picker.

`scripts/test-rust-suspend.py --assets PATH` exercises a real imported ROM in
disposable profile data. It verifies keyboard/gamepad toolbar navigation,
pending screenshots, numbered-slot selection, lock protection, overwrite
confirmation/cancel, deterministic load/replay, isolated deletion, loading after
unloading the core, and picker screenshots across a process restart.

### Language import (v5)

The Rust importer now saves every shipped `strings/<locale>/strings.lng` table
under `skin/languages/`, with `skin/languages.json` as its index. All six supplied
images contain nine tables (95 strings each). `skin/strings.json` remains the
regional default for compatibility. Every language file is covered by the
profile hash manifest. Identifiers, table counts, sizes and string types are
validated before publishing.

The globe/Language panel switches labels immediately and saves the locale in
the profile preferences. Missing keys fall back to the regional default. This
does not translate the launcher, custom emulator controls, game titles, or ROMs.
The original regional artwork/palette remains selected by the source image.

v7 imports preserve v4/v5/v6 settings and saves alongside the unchanged old profile.
The startup launcher upgrades a selected v4 profile when its source is online,
and prefers the new profile thereafter. Offline v4 profiles remain launchable
with their existing default table; re-import the source to enable all languages.
The six-image audit verifies all 54 language tables, source immutability, cache
reuse, v4/v5/v6-to-current save/settings preservation, and upgrade deduplication. The UI
snapshot test additionally renders each locale's menu and language selector and
checks preference persistence across restart. Rust native-app tests: **44/44**.

`rust/src/sfrom.rs`, `image_reader.rs`, `skin.rs`, and `profile.rs` implement the
converter and image/profile import pipeline. The SFROM port retains firmware
policy, supported-format limits, checksum normalization, metadata, and fail-closed
behavior. The image reader consumes selected members in memory and never extracts
or runs the console's executable/script files. Cache output is versioned as v7;
upgrades preserve settings and saves without replacing old profiles. The CLI supports:

```sh
python3 scripts/build-rust-desktop.py
CLOVER_NATIVE_BUILD_DIR="$PWD/.build/native" cargo test --locked \
  --manifest-path rust/Cargo.toml --all-targets --features native-app
python3 scripts/test_rust_sfrom.py
python3 scripts/test_rust_importer.py
```

The SFROM round-trip audit compares standalone conversion with the corresponding
ROM bytes written by Rust profile imports. `test_rust_importer.py` exercises the
six full SNES profile imports, cache reuse, migration, and source immutability.
Native ares smoke tests cover the C ABI and generated SNES/NES inputs.

The Rust frontend is the packaged/default app and can be run directly after
building its native libraries:

```sh
CLOVER_NATIVE_BUILD_DIR="$PWD/.build/native" cargo build \
  --manifest-path rust/Cargo.toml --features native-app
./rust/target/debug/clover-rust run --image /path/to/filesystem.hsqs
```

`run` (or double-clicking the macOS bundle) always opens the Game Data Library
before the actual menu. It lists verified cached profiles and nearby images,
and supports browsing for `.hsqs`/archives or a prepared game-data folder.
Imports run on a worker thread with progress/error text and leave the launcher
responsive. `--image FILE` bypasses the launcher for automation;
`--assets DIR --data-dir DIR` is useful for testing an existing
profile. Settings and saves stay in the profile's data directory; the source
image remains read-only. Games requiring absent DSP firmware are marked with a
lock and cannot start until the user selects a firmware folder.

The Rust menu's deterministic static render was compared against the former C++
menu using the same image profile, game, and disabled animation; the screenshots
were pixel-identical. The Rust frontend also passes its SDL dummy-driver window,
launcher, and real-profile screenshot smoke runs. This does not establish
physical-device behavior or Windows/Linux runtime compatibility.

Current macOS validation: Rust native-app tests **44/44**, Clippy with warnings
denied, CTest **3/3** (ares SNES/NES smoke and macOS menu bridge), six-profile
import/cache/migration audit, SFROM round-trip **126/126**, NES/Famicom runtime
smokes, menu-audio playback, UI snapshots, and relocated-package launch all pass.
Firmware gating has Rust tests for missing, partial, and complete required-file
sets. The former C++ importer/frontend are not compiled; native C++ is limited
to ares and the macOS menu bridge.

### Graphics review and macOS application bundle

The Rust tooltip left end-cap is now mirrored correctly. Rendering goes through
a fixed 1280x720 target before window/Retina scaling, like the C++ reference.
Modal panels now dim with alpha blending, clear underlying mouse targets, and
fit/scroll their rows within the dialog. Up navigates backwards in panels.

The launcher uses a modern dark/teal library layout, embedded OFL-licensed
Barlow typography, friendly region/version cards, a selected-source details
panel, keyboard focus/hover states, and responsive import/error/empty states.
It needs neither Nintendo art nor system fonts; the game menu is rendered by
Rust as well.

`scripts/test-rust-ui.py --assets PATH` captures all five toolbar bubbles,
settings/resume/info/help/profile panels, and the launcher (including empty,
importing, and error states) using SDL's dummy
driver. Its tooltip-border regression requires mirrored caps (zero differing
border pixels). It uses ImageMagick for this development-only assertion.

`python3 scripts/build-rust-macos.py` builds `dist/Classic Menu.app`, with a
native executable, Info.plist, third-party/Rust license notices, and local ad-hoc
signature. Existing bundles are preserved under a timestamped backup. This is a
host-architecture app, not a universal/notarized distribution.
The built bundle's signature was verified, and its launcher and real-profile
rendering passed after relocation to a temporary directory containing spaces.
`otool -L` shows only macOS system libraries/frameworks, not checkout/Homebrew
runtime dependencies. The 15-snapshot UI regression, Rust tests/Clippy, and
ares/native-menu tests pass. Physical-display/Retina interaction still needs
manual verification.
