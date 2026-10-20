# Rust migration status

This is a staged rewrite of the current SDL3/ares app, not a claim that the
existing C++ frontend and image importer have already been replaced.

## Boundary

- The SNES execution core remains upstream ares C++. `portable/core/ares_c_api.h`
  is the intended narrow boundary: UTF-8 paths, buttons, borrowed ARGB frames and
  stereo float samples, state operations, and error text. The Rust wrapper is
  thread-affine and prevents stepping the machine while a borrowed frame/audio
  view is alive.
- SDL3 remains the cross-platform window/audio/render backend. The Rust app will
  call SDL's public C ABI; platform-specific macOS menu integration may remain a
  tiny Objective-C++ adapter where AppKit requires it.
- Image reading, font/skin preparation, profile/cache/migration, SFROM conversion,
  menu/application logic, and tests are intended to move to Rust. Nintendo host
  executables or scripts remain data and are never run.

## Milestones

1. **Converter/core boundary (complete):** Rust SFROM parsing/audio restoration;
   safe ares FFI wrapper; transition parity and synthetic tests.
2. **Image importer (complete for supplied formats):** bounded SquashFS and
   gzip/ustar readers, static skin decoding, cache verification, v1–v3 data
   migration, and local profile discovery. All six regional/version images
   import as v4 profiles; outputs match the existing importer semantically.
3. **Rust SDL application (in progress):** the imported-profile menu now renders
   Clover's atlas, bitmap fonts, covers, carousel, HUD, settings, and toolbar.
   Keyboard/gamepad input, ROM and firmware pickers, profile import/switching,
   suspend slots, menu/game audio, native macOS menus, and the ares launch path
   are wired. A fixed-animation menu screenshot matched the C++ reference
   pixel-for-pixel before adding the required missing-firmware lock badge. Input,
   audio, and physical-display comparisons are still pending.
4. **Build/package cutover:** compile the Rust frontend and importer with the
   C++ ares C ABI into the cross-platform package; run the full existing test
   suite and verify relocation before changing the default product.

## Implemented Rust portion

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

Native-app tests now pass **18/18**. The real-core integration check also covers
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

v5 imports preserve v4 settings and saves alongside the unchanged old profile.
The startup launcher upgrades a selected v4 profile when its source is online,
and prefers the new profile thereafter. Offline v4 profiles remain launchable
with their existing default table; re-import the source to enable all languages.
The six-image audit verifies all 54 language tables, source immutability, cache
reuse, v4-to-v5 save/settings preservation, and upgrade deduplication. The UI
snapshot test additionally renders each locale's menu and language selector and
checks preference persistence across restart. Rust native-app tests: **16/16**.

`rust/src/sfrom.rs`, `image_reader.rs`, `skin.rs`, and `profile.rs` implement the
converter and image/profile import pipeline. The SFROM port retains firmware
policy, supported-format limits, checksum normalization, metadata, and fail-closed
behavior. The image reader consumes selected members in memory and never extracts
or runs the console's executable/script files. Cache output is versioned as v5;
upgrades preserve settings and saves without replacing old profiles. The CLI supports:

```sh
cd rust
cargo test
cargo build --release
../scripts/test_rust_sfrom.py --legacy-executable ../.build/portable/ClassicMenu.app/Contents/MacOS/ClassicMenu
python3 ../scripts/test_rust_importer.py
```

The parity test imports each supplied image with the current importer, converts
its local SFROM members with Rust, and requires byte-identical ROM output to that
transition oracle and requires byte-identical ROM output for all 126 SFROM files.
`test_rust_importer.py` exercises the six full profile imports, cache reuse, and
source immutability. Rust-generated asset JSON is semantically equal to the
existing importer; formatting is not byte-identical. The C ABI is covered by the
existing native ares smoke test; Rust core bindings compile with
`cargo test --features native-ares`.

The opt-in frontend can be built against the same native CMake artifacts and
run directly (the packaged/default app remains C++):

```sh
CLOVER_NATIVE_BUILD_DIR="$PWD/.build/portable" cargo build \
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

The Rust menu's deterministic static render was compared against the C++ menu
using the same image profile, game, and disabled animation; the screenshots were
pixel-identical. The Rust frontend also passes its SDL dummy-driver window and
real-profile screenshot smoke runs. This does not yet cover input automation,
full game-launch/audio parity, native file-dialog interaction on every OS, or
Windows/Linux builds.

Latest local validation: Rust native-app tests **11/11**, Clippy with warnings
denied, C++ CTest **8/8**, six-profile importer/cache/source-immutability audit,
and SFROM parity **126/126 byte-identical ROMs** all pass. Firmware gating also
has a Rust test covering missing, partial, and complete required-file sets.

### Graphics review and macOS preview bundle

The Rust tooltip left end-cap is now mirrored correctly. Rendering goes through
a fixed 1280x720 target before window/Retina scaling, like the C++ reference.
Modal panels now dim with alpha blending, clear underlying mouse targets, and
fit/scroll their rows within the dialog. Up navigates backwards in panels.

The launcher uses a modern dark/teal library layout, embedded OFL-licensed
Barlow typography, friendly region/version cards, a selected-source details
panel, keyboard focus/hover states, and responsive import/error/empty states.
It needs neither Nintendo art nor system fonts. The original menu is unchanged.

`scripts/test-rust-ui.py --assets PATH` captures all five toolbar bubbles,
settings/resume/info/help/profile panels, and the launcher (including empty,
importing, and error states) using SDL's dummy
driver. Its tooltip-border regression requires mirrored caps (zero differing
border pixels). It uses ImageMagick for this development-only assertion.

`python3 scripts/build-rust-macos.py` builds `dist/Classic Menu Rust.app`, with a
native executable, Info.plist, third-party/Rust license notices, and local ad-hoc
signature. Existing Rust bundles are backed up; the C++ product is untouched.
This is a host-architecture preview, not a universal/notarized distribution.
The built bundle's signature was verified, and its launcher and real-profile
rendering passed after relocation to a temporary directory containing spaces.
`otool -L` shows only macOS system libraries/frameworks, not checkout/Homebrew
runtime dependencies. The 15-snapshot UI regression, Rust tests/Clippy, and
existing 8-test CTest suite pass. Physical-display/Retina interaction still
needs manual verification.

Until milestones 3–4 and end-to-end rendering/input/audio tests land, the packaged
application remains the existing C++/SDL build. The Rust crate is not yet the
default executable.
