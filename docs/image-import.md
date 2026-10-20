# Image-driven launcher (0.4.0)

## Current 0.5.0 behavior (supersedes the 0.4.x limitations below)

All six supplied Europe/USA/Japan filesystems import all 21 ROMs, including the
previously blocked PCM titles. Signal-exact BRR restoration is verified against
the supplied PCM and recorded loop histories; byte-identical retail ROM recovery
and full gameplay compatibility are not asserted. See [the restoration audit](sfrom-import.md).
Super Mario Kart additionally needs user-supplied DSP1B firmware for ares. Only
SMP IPL is recovered automatically from the supplied images; no unverified DSP
firmware extraction or download is substituted.

Image changes now drive actual regional artwork/wallpaper, fonts, catalog,
portrait/landscape covers, original strings, and recovered title/HUD colors.
Japanese `.fnt` containers and source Japanese labels are supported. Custom
emulator/import panels and macOS menus remain English. Details and source
fingerprints: [regional menu verification](regions.md).

Importer revision 3 creates a new profile beside v1/v2 and copies user data
without modifying the old cache. The last-used old profile upgrades automatically
when its source still exists and matches its digest. Otherwise choose its image
again. Choosing a coprocessor firmware folder no longer displaces imported IPL.

Menu music now refills before the current clip drains, avoiding a frame-sized
gap at loop boundaries. The native game audit records finite/nonzero PCM, RMS,
peak and video activity rather than treating a nonempty audio buffer as sound.
Tests and limits: [game audio audit](game-audio-audit.md).

## 0.4.1 rendering and macOS menus

- Native macOS menus now include Library → Image Library / Switch Version…
  (Command-Shift-I), Import Image… (Command-O), ROM/firmware pickers, settings,
  sound, sorting, fullscreen, help, and standard application/window actions.
- F2 (or fn-F2 on keyboards with media keys) and Settings still open the image
  library. European images are listed first, and known other-region files are
  labeled not enabled. The active profile/version appears in the window title.
- Cover textures were 228×160 but incorrectly stretched to 228×204. Covers now
  fit within that aperture without distortion; 40×28 thumbnails likewise retain
  their ratio instead of being forced square. Card/frame geometry is unchanged.
- The visible macOS frontend prefers SDL's Metal renderer with VSync and draws
  into a fixed 1280×720 target, linearly scaled once to the window. Nearest atlas
  sprites resolve to stage pixels; all cover textures preload before animation,
  and suspend-slot filesystem checks are cached rather than repeated each frame.
  These changes address likely jitter/shimmer causes; headless screenshots do
  not establish that physical-display flicker is eliminated.
- Seven local CTest targets pass, including artwork geometry and native menu
  label/command-dispatch tests. `--scroll-test --frames 240` exercises repeated
  carousel moves; renderer/VSync information is printed on finite-frame exit.
  The packaged macOS app completed this test on the real `metal` renderer with
  VSync enabled; log: `analysis/metal-scroll-test.log`, scene capture:
  `analysis/metal-scroll-preview.png`. Perceptual display flicker still needs
  user confirmation.

## Use

Quit any older running Classic Menu process, then open:

```sh
open "dist/portable/Classic Menu.app"
```

The first-run image library needs no Nintendo assets to render. Select a local
European filesystem and press Return/double-click; I opens a native file picker.
Drop an image onto this launcher to import it. Progress is displayed while a
background worker imports. Closing the window during import waits for that work
to finish before exiting. The menu's F2 shortcut or Settings opens the library
again. The last verified profile opens automatically on subsequent launches.

Images beside the installed package are discovered. This workspace's supplied
`SNES-Mini-Kernels/` directory is also recognized when running from its build
layout. No image is fetched from the internet.

Supported input:

- European 2.0.14 and 2.0.7 `.hsqs` SquashFS files, including trimmed variants.
- Their matching gzip/tar filesystem archives.
- A named `kernel-*.img` with its matching filesystem beside it, or in the
  workspace's `SNES-Mini-Kernels/` child directory. **Boot images alone have no
  game filesystem.** Matching here is filename-based discovery, not a claim of
  cryptographic binding between a boot image and a root filesystem.

USA/Japanese files can be read by the archive layer, but profile import rejects
them until their layouts/fonts/catalogs are validated. Full language selection
and localization of reconstructed panels remain future work.

## What is imported

The native C++ importer uses a bundled, statically linked zlib. There is no
runtime Python, `tar`, `unsquashfs`, Canoe process, or extracted shell/Lua code.
It decodes stock bitmap fonts and scene measurements and retains selected
sprites, wallpaper, sounds, game artwork, and sanitized catalog metadata.
Desktop `Exec` fields are never invoked. Archive links/devices are not followed.

The standard 64-byte SMP IPL is recovered from the supplied Canoe executable
as **data only**, using a full SHA-256 match:
`c95f88b299030d5afa55b1031e2b5ef2dff650c4b4e6bb6f8b1359436521278f`.
Only those 64 bytes are retained, not the ARM executable. On the supplied
European 2.0.14 carrier, direct byte inspection located one match at `0x19b888`.
This is ordinary artifact inspection, not REA decompilation evidence.

This also fixes a prior bridge omission: the ares SMP was previously running
without initialized IPL bytes. Synthetic color tests did not exercise games'
sound-driver upload handshake. The bridge now explicitly requires `ipl.rom`,
and save-state identity includes its bytes. Core tests supply a self-authored
idle SPC700 firmware instead of Nintendo firmware. Existing old bridge states
are intentionally incompatible. When choosing a manual firmware folder, that
folder must include `ipl.rom` as well as any required coprocessor firmware.

## ROM support: three converted, eighteen blocked

**This is not yet a full 21-game import.** All 21 titles have menu artwork and
metadata, but only Star Fox, Yoshi's Island, and Star Fox 2 have supported ROM
conversion. These containers have no separately encoded PCM; their unchanged
ROM payloads pass header, mapping, size, and computed SNES checksum validation.
All three ran for 600 startup frames with visible intro output after IPL was
supplied. This is not full gameplay, audible playback, or controller validation.

The other 18 stock containers replace BRR audio blocks with PCMF records and
external PCM. Removing a fixed header would leave damaged audio data. The
importer rejects these conversions and records actionable errors rather than
publishing them as playable. Verified restoration, loop/control handling,
S-DD1 extensions where relevant, and game-specific patch assessment remain open.
See [the per-game SFROM audit](sfrom-import.md) for exact ranges and findings.

You can still map your own standard ROM through Settings. Additional
coprocessor firmware is never downloaded. No external ROM/patch payload is
substituted for unsupported image content.

## Cache and safety

SDL's `ClassicMenu/ClassicMenu` preference directory contains:

```text
last-profile.json
profiles/<filesystem-sha256>-v2/
  profile.json              source identity, conversion results, output hashes
  assets/catalog.json
  assets/skin/...
  assets/games/...
  assets/roms/...           only supported converted ROMs
  assets/firmware/ipl.rom
  data/preferences.json
  data/saves/...
```

`--data-dir` overrides the base directory. Each exact filesystem image digest
and importer version gets an isolated profile. Different archive encodings of
the same filesystem can therefore produce separate profiles. Saves are not
automatically merged. SRAM is further separated by ROM digest within the core.

Images are opened read-only, hashed before/after parsing, and never modified.
Output is prepared in a unique staging folder and published by directory
rename only after success. Failed staging folders are retained (and hidden
from the library) for diagnostics. Existing damaged caches are not overwritten;
preserve/rename them before retrying. Every published asset is hash-checked
before reuse; unknown importer revisions and corrupt profiles are not launched.
An intact cache can launch with its original source offline.

The default app package includes code and licenses only. Firmware, games, and
artwork remain user-imported content; this does not establish redistribution
rights. Source formats/parser limits are documented in [image reader](image-reader.md).

## Commands and verification

```sh
python3 scripts/build-portable.py
ctest --test-dir .build/portable --output-on-failure
python3 scripts/test_image_import.py --run-games
python3 scripts/validate-portable.py --image SNES-Mini-Kernels/dp-sneseur-release-v2.0.14-0-gd8b65c6.hsqs
```

The integration script's `--run-games` explicitly runs the three converted
stock ROMs through ares. Without it, tests cover import/render/cache/blocking.
Tests preserve unique `.build/` artifacts. The native core test still uses only
self-authored ROM/firmware. CTest covers hash/companion handling, hostile archive
fixtures, SFROM checks, launcher rendering, and the core (5 targets).

CLI options include `--image`, `--import-image` (no menu), `--list-profiles`,
`--launcher`, `--render-launcher`, `--game CODE --start`, `--smoke-test`, and
`--assets` for the older pre-extracted catalog path. Windows/Linux builds have
not been run locally; the build matrix remains pending remote execution.
