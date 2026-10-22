# In-process ares SNES / Famicom bridge

**0.4.0 correction:** the bridge now requires a 64-byte `ipl.rom` in the firmware
directory and includes it in state identity (bridge revision 2). The original
color test did not expose missing SMP boot firmware. The image importer supplies
the verified bytes from the user's own image; tests use a self-authored SPC700
idle program. See [image import](image-import.md). No Nintendo IPL bytes are
compiled into the application.

`portable/core/ares_core.h` implements the internal `clover::AresCore` API using
ares commit `9408cb43d4948fc3ea6e152a307a34348df3fe04`. Emulation is linked into
the application: no external emulator, desktop UI, SDK, firmware download,
SDL, Qt, GTK, or runtime resource directory is required by this target.

The Rust application uses `portable/core/ares_c_api.h`, implemented by
`ares_c_api.cpp`, through the safe, thread-affine wrapper in `rust/src/ares.rs`.
It does not call the C++ API directly. ares and SDL are separate ignored
checkouts, not Git submodules; `scripts/bootstrap-portable.py` fetches and
verifies their pinned revisions from `dependencies.lock.json`.

## Build integration

```cmake
add_subdirectory(portable/core)
target_link_libraries(your_frontend PRIVATE clover_ares)
```

The static target exports its include directory and C++20 requirement. Its
upstream includes, definitions and compatibility flags remain private. CMake
must have C and C++ compilers; `enable_language(C)` also accommodates C++-only
parents. Threads and platform system libraries are propagated to static-library
consumers (CoreFoundation on macOS; Windows networking, COM, shell and system
libraries; dl where needed).
`CLOVER_ARES_SOURCE_DIR` can override the default `third_party/ares` source path;
it must refer to the pinned revision, not an arbitrary newer checkout.

Only the SFC and FC cores, their required processor/audio/cartridge components,
shared ares runtime, nall utility implementation, libco, and FC's required ymfm
dependencies are compiled. Other emulation cores,
JIT implementation, desktop UI, mia database/archive importers and mia firmware
resources are not linked. At configure time, CMake extracts upstream's exact
SFC heuristic analyzer implementation (with a small standalone declaration
adapter), embeds `ares/System/Super Famicom/boards.bml`, and embeds the
SFC/FC crosshair PNGs needed by the controller translation units. Generated files
stay in the build tree; upstream is not modified. No build tool is downloaded.

## Calling and data contract

```cpp
#include <ares_core.h>
clover::AresCore core;
if (!core.load(romPath, saveDirectory, firmwareDirectory)) {
  // Display core.error().
}
if (core.runFrame(buttonMask)) {
  // Upload core.frame().pixels and queue core.audio().
}
core.unload(); // Check error() for a final SRAM write failure.
```

All operations, including unloading/destruction, must be performed on the thread
that loaded the ROM. This is required by libco's coroutine context. Concurrent
operations on an instance are unsupported. The global ares platform/machine is
reserved at load and released on unload; a second loaded instance is rejected.

`runFrame` drives the loaded system's scheduler; SNES rendering waits for the
video-worker callback.
Pixels are packed, opaque `0xFFRRGGBB`, and dimensions come from ares's processed
viewport; do not assume 256x224 (the smoke test produces 564x242). Audio is
interleaved stereo float, clipped to [-1,1], resampled by ares to 48000 Hz, and
cleared each `runFrame`. Buffers remain valid until the next mutating operation.
The frontend owns pacing, audio buffering, presentation and event processing.
The worker callback has a five-second timeout; the synchronous emulation call
itself is not sandboxed or subject to a hard execution deadline.

Controller port 1 is explicitly allocated and connected as `Gamepad`. Input
buttons are set by node name: bits 0–11 are B, Y, Select, Start, Up, Down, Left,
Right, A, X, L, R. Port 2 is left disconnected. Pixel accuracy and deterministic
entropy are enabled.

## ROMs and firmware

SNES loading accepts raw `.sfc` and `.smc` files (case-insensitive); the optional
512-byte copier header is stripped by the upstream analyzer. `.sfrom` returns
an explicit conversion-needed error. There is no automatic container conversion,
archive reading, patching, interleave conversion or local manifest override.
Headers are scored, generated board IDs must match the embedded upstream board
table, and missing ROM/firmware parts fail rather than silently booting zeros.
Maximum input ROM size is 16 MiB.

NES/Famicom loading accepts supported iNES 1.0 `.nes` cartridges. Supported
mappers are 0, 1, 2, 3, 4, 9, and 184, with additional board/size checks;
NES 2.0, trainers, unsupported board variants, and Famicom Disk System are
rejected. See `portable/core/nes_cartridge.h`, `rust/src/nes.rs`, and
[the NES/Famicom migration notes](rust-migration.md) for the supported scope.
NES cartridges do not require the SNES SMP IPL.

Most cartridges require no external firmware. For firmware-dependent boards,
provide your own legally obtained dumps in the explicit firmware directory.
Files are named `<lowercase identifier>.<lowercase architecture>.<content>.rom`,
with the exact size required by the generated manifest. Examples:

| Firmware | Files / sizes |
| --- | --- |
| DSP1B | `dsp1b.upd7725.program.rom` (0x1800), `dsp1b.upd7725.data.rom` (0x800) |
| DSP1–4 | same naming convention with `dsp1`, `dsp2`, `dsp3`, `dsp4` identifier |
| Cx4 | `cx4.hg51bs169.data.rom` (0xc00) |
| ST010/ST011 | `st010.upd96050.program.rom` (0xc000), `st010.upd96050.data.rom` (0x1000), or `st011` equivalents |
| ST018 | `st018.arm6.program.rom` (0x20000), `st018.arm6.data.rom` (0x8000) |

Properly appended firmware recognized by upstream's analyzer is also accepted.
Firmware is size-checked, not verified against a known-good copyright-sensitive
database. Nothing is downloaded, and bundled mia firmware is deliberately not
used. Super Game Boy, Satellaview and Sufami Turbo cartridge add-ons are rejected;
MSU-1 sidecars and other controller peripherals are not exposed by this API.
Ordinary cartridge coprocessors are compiled but have not been game-tested here.

## Persistence and state compatibility

The save directory is mandatory. Persistent memories live under
`<saveDirectory>/<SHA256(normalized ROM)>/` with upstream VFS names such as
`save.ram`, `upd96050.data.ram` and `time.rtc`. Existing memory sizes must match.
Directory creation failures are reported at load; write failures are reported
when persistence is attempted. SRAM/RTC are copied out of the core
on unload and before applying a valid state, protecting current progress before
rewind. Restored SRAM is subsequently persisted on unload. Files are written to
a sibling temporary and renamed, with a backup/restore replacement on Windows.
`unload()` cannot return a bool under the agreed API, so callers should inspect
`error()` before destruction if they need to report a failed final flush.

States are intentionally strict and not interchangeable with desktop ares states.
They carry bridge format, exact core revision, core options, architecture, OS,
compiler identity, pointer width, normalized-ROM SHA256, loaded ROM+firmware
SHA256 and payload SHA256. The exact payload size is checked against a fresh
snapshot before ares is allowed to mutate the machine, and a rollback snapshot
is retained for core rejection. Corrupt, truncated and different-ROM states fail.
No promise is made of cross-compiler, cross-OS, cross-architecture, or future
bridge compatibility. State checksums detect corruption, not maliciously crafted
states; as with the upstream emulator, only load trusted state files.

## Standalone smoke tests

For the separate iOS/tvOS simulator **compile/link-only** checks and build-tree
compatibility adaptations, see [feasibility evidence](frontend-feasibility.md).
They do not establish simulator/device runtime or frontend app support.

```sh
python3 scripts/bootstrap-portable.py
cmake -S portable/core -B .build/ares-bridge -DCMAKE_BUILD_TYPE=Release -DCLOVER_BUILD_TESTS=ON
cmake --build .build/ares-bridge -j 6
ctest --test-dir .build/ares-bridge --output-on-failure
```

The test creates its own original 32 KiB LoROM program in a unique build-directory
subfolder. It checks real red/green video, B-button serial input, nonempty finite
stereo audio (the program does not upload an SPC music program), SRAM contents,
save/load state roundtrip, corrupt and wrong-ROM state rejection, copier-header
normalization, unsupported SFROM and one-session enforcement. Test artifacts are
retained for inspection. No extracted Nintendo game/firmware is executed.

The separate generated NES test checks video/audio, controller input, state
replay, SRAM persistence, PAL startup, mapper initialization, invalid cartridge
rejection, and single-session enforcement without commercial ROM fixtures.

Validated on macOS arm64 with AppleClang 21, Release build. Windows and Linux
paths are implemented but not compiled or runtime-tested in this environment.

## Licensing

Retain `portable/core/LICENSE.ares`, `third_party/ares/LICENSE` and upstream
component notices in source/binary redistributions. The ares core and the
generated analyzer/board/sprite material are copyright 2004–2025 ares team,
Near et al, under ISC. libco/nall and included third-party headers retain their
upstream notices. No copyrighted cartridge firmware is added by this bridge.
