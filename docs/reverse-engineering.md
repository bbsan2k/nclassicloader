# Investigation ledger

**Update:** the second pass restores the actual Clover art, scene measurements,
bitmap fonts, and selected bytecode-derived motion. See
[Clover visual reconstruction](clover-visuals.md). The initial-pass observations
below are retained for provenance; its SwiftUI presentation has been replaced by
an AppKit scene renderer in the SwiftUI app shell.

## Scope and status

| Question | Status |
| --- | --- |
| Prepare REA/Ghidra | Answered: live MCP reports Ghidra available; native analysis succeeded |
| Retrieve supplied images | Answered: cloned `https://github.com/mentalblank/SNES-Mini-Kernels` into `SNES-Mini-Kernels/` |
| Identify game-selection menu | Answered: shell wrapper → `ReedPlayer-Clover` → compiled LuaJIT UI resources |
| Disassemble native loader | Partial: ARM `main` disassembled and decompiled; not every engine function |
| Reconstruct macOS app | Partial: buildable native catalog/browser/launcher preview, not behavioral parity |
| Exact menu state machine / save / replay | Unresolved: compiled LuaJIT control flow and console runtime need further work |

## Artifact identity

- Original supplied boot image:
  `kernel-dp-sneseur-release-v2.0.14-0-gd8b65c6.img`.
  SHA-256 `b13b93a44ff89dfec0d81127a3dd9a62f06dcd3aa24c257dfd82442d52c698c0`.
- Matching filesystem archive:
  `SNES-Mini-Kernels/dp-sneseur-release-v2.0.14-0-gd8b65c6.tar.gz`.
  Its digest and extracted regular-file digests are in
  `analysis/extraction-manifest.json`.
- Native menu runtime: `analysis/eur/usr/bin/ReedPlayer-Clover`, ARM ELF,
  SHA-256 `ce587d8ad98282501d794a6aa4a1db1637f061981c9f56df7905487f25056af6`.

REA uses Ghidra **12.1.3**, the installed native macOS arm64 decompiler, and the
installed Homebrew Java **21.0.12.1**. REA notes its bridge was verified against
12.1.4 and accepts other 12.1.x releases. The executable was statically analyzed,
not executed.

## REA evidence

Complete returned records, including provider limitations and analysis profile,
are retained in `analysis/rea-evidence.json`:

Readable native output is also saved as `analysis/ReedPlayer-Clover-main.asm`
and `analysis/ReedPlayer-Clover-main.c`. The owned Ghidra session was closed after
exporting the evidence.

| Evidence | Observation |
| --- | --- |
| `ev_558866ecb6ba3821913309b2553e6a2a02978366601cc1bd8c8abe0bc8034a65` | Procedure search identifies `main` at `0xbffac` |
| `ev_aa5bdf2f692923249106114c0eb3ad246c557553e58a9363901f9a0385437f83` | Decompilation of `main`: locale/argument handling, Reed resource/save roots, SDL subsystem initialization, timing/render loop, and shutdown |
| `ev_f979878492300c8ac3934c60529baafba3c3131d3727efd7d1efaf473024cc89` | ARM assembly for `main` |
| `ev_d780c8eeef7fa46c98f3ff50ab60de37addfb85418cd780b22f55cbfc16c34bc` | Defined string search finds bootapp option/help text; does **not** establish catalog or launch behavior |

Ghidra warns about unreachable-block removal and decompiler dead-code handling in
`main`. Pseudocode is recovered analysis output, not original source. Searches
cover provider-defined objects, not all possible embedded data. The bundle's lack
of recorded unknown entries does not mean this investigation has no unknowns.

## Direct artifact observations (ordinary file inspection, not REA Evidence)

- The original file has an Android boot header, a Linux kernel, and an
  LZO-compressed ramdisk. Its command line references `/dev/nandb` as root.
  Header-derived payload copies are `analysis/kernel.bin` and `analysis/ramdisk.bin`.
- `usr/bin/clover-ui` is a shell script, not the native menu executable. It
  requires UI/language/region arguments, checks an override before the stock UI,
  and invokes `ReedPlayer-Clover --bootapp=...` through `taskset`.
- It supplies version, decorative-frame, and suspension-point paths through
  additional Lua script variables. This wrapper was read, never run.
- `usr/share/ui/snes-eur/init.json` lists the scene and UI script resources.
  The `.lua` files begin with `1b 4c 4a 01`, consistent with LuaJIT v1 bytecode;
  they are not readable Lua source.
- Printable constants in `data_gamelist.lua` include title, players, recent,
  play-count, release, and publisher sort labels. Constants in `mcp_commands.lua`
  include start-game, resume-game, record/replay, transition event descriptors,
  and atomic command-file terminology. **These are leads, not proven control flow.**
- `usr/share/games/CLV-*/CLV-*.desktop` holds catalog names, console `Exec`, art
  paths, codes, player counts, dates, sort keys, and copyrights. There are 21
  catalog entries. The macOS reconstruction uses metadata, never console `Exec`.

## Extraction limitations

The archive contains both `etc/clover/VERSION` and the directory
`etc/clover/version`, which collide on this host's case-insensitive filesystem.
The initial partial extraction is left at `analysis/rootfs-eur/`; it is not used
for analysis or building. The usable extraction at `analysis/eur/` intentionally
contains only regular files under `usr/`. Links and special files were not
published, and no extracted program or script was executed. This is not an exact
bootable root filesystem. Use a case-sensitive filesystem for a future full dump.

## Reconstruction choices and remaining gaps

`Sources/MenuCore/GameLibrary.swift` implements strict desktop-entry loading,
local artwork paths, deterministic sorts, and wraparound navigation.
`Sources/ClassicMenu/ClassicMenuApp.swift` supplies a new SwiftUI renderer,
selection persistence, local launch history, and explicit external emulator
document opening. UI proportions, navigation animation, and comparator directions
are reconstruction choices, not proven copies of the original LuaJIT logic.

No attempt is made to execute ARM Linux code as a macOS app, interpret LuaJIT
bytecode as native instructions, claim SFROM files are standard SNES ROMs, or
fabricate working console save/replay behavior. Stock game artwork is copied only
into a local build; ROM files remain outside the app.

Tests cover parsing, path/code validation, navigation, sorting, a temporary
library, and the actual extracted 21-game catalog. Tests do not prove original
console parity. Emulator launching and visual fidelity still require manual
validation. Further work needs static LuaJIT bytecode decoding and, for runtime
parity, a suitable console/runtime observation setup.

## Local verification

- `swift test`: six tests passed, including the extracted 21-entry catalog.
- `python3 scripts/build-app.py`: release build succeeded on Apple Silicon;
  local ad-hoc signing succeeded.
- `codesign --verify --deep --strict`: passed for `dist/Classic Menu.app`.
- Package inspection: exactly 21 desktop entries; no `.sfrom`, `.sfc`, `.smc`,
  `.img`, or `.hsqs` payloads inside the app.
- Launch smoke test: the built executable remained alive for four seconds with
  no reported stderr, then the test terminated its owned process. This is a
  startup test, not a screenshot comparison or proof of interactive behavior.
