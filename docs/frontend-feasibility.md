# Frontend portability: feasibility evidence

These are development probes, not supported app builds. They use only the
project bridge and self-authored core smoke tests; no console content is used.
See [architecture](frontend-architecture.md), [task queue](frontend-tasks.md), and
[platform status](platform-builds.md) for acceptance gates.

## Apple core compilation probe — 2026-10-21

Environment: macOS arm64, Xcode 27.1 (`27A9275`), AppleClang 21.0.0,
iPhoneSimulator 27.1 SDK, AppleTVSimulator 27.0 SDK. ares revision:
`9408cb43d4948fc3ea6e152a307a34348df3fe04`.

### Reproduction

After the normal dependency bootstrap, configure separate cross-target build
directories. Do not reuse macOS libraries or run these binaries as macOS tests.

```sh
cmake -S portable/core -B .build/portability/ios-simulator -G Ninja \
  -DCMAKE_SYSTEM_NAME=iOS -DCMAKE_OSX_SYSROOT=iphonesimulator \
  -DCMAKE_OSX_ARCHITECTURES=arm64 -DCMAKE_OSX_DEPLOYMENT_TARGET=15.0 \
  -DCLOVER_BUILD_TESTS=ON -DCMAKE_BUILD_TYPE=Release
cmake --build .build/portability/ios-simulator --parallel 3

cmake -S portable/core -B .build/portability/tvos-simulator -G Ninja \
  -DCMAKE_SYSTEM_NAME=tvOS -DCMAKE_OSX_SYSROOT=appletvsimulator \
  -DCMAKE_OSX_ARCHITECTURES=arm64 -DCMAKE_OSX_DEPLOYMENT_TARGET=15.0 \
  -DCLOVER_BUILD_TESTS=ON -DCMAKE_BUILD_TYPE=Release
cmake --build .build/portability/tvos-simulator --parallel 3
```

### Initial result: blocked before linking/runtime

At baseline `08072c8`, both configurations succeeded, including pthread discovery.
Both builds failed while compiling the then-current native bridge composition:

- **iOS and tvOS:** `nall/nall/memory.hpp:204` references
  `pthread_jit_write_protect_np`, explicitly unavailable on these targets.
  `nall/nall/intrinsics.hpp` defines `PLATFORM_MACOS` for `__APPLE__`, which also
  matches iOS/tvOS; the utility header's macOS-only guard is therefore too broad.
- **tvOS additionally:** the monolithic `nall/nall/nall.cpp` includes `run.cpp`
  and `run.hpp`, which reference unavailable `fork`, `execvp` and `execlp` APIs.
  These are process-launch utility compilation failures, not proof that the
  SNES/FC scheduler itself launches external processes.

The successful compilation of `ares_c_api.cpp` alone did not establish that the
core linked or ran.

### Follow-up: simulator and device compile/link checks now pass

The current build uses `portable/core/apple_compat.cmake` only when
`CMAKE_SYSTEM_NAME` is `iOS` or `tvOS`. It generates two build-tree files:

- A `nall/memory.hpp` overlay includes Apple's `TargetConditionals.h` and narrows
  the existing macOS JIT write-protection guard to `TARGET_OS_OSX`. It does not
  enable JIT or pretend mobile is macOS; the selected SNES/FC cores are
  interpreter-based and retain the helper's non-macOS path.
- A nall runtime composition omits the unused `run.cpp` process-launch unit.
  There are no fake successful process implementations. Any accidental caller
  of omitted functions would produce an unresolved symbol during linking.

Each source substitution must match exactly once or configuration fails. The
pinned upstream checkout remains unchanged. Generated files carry upstream
attribution; the original ares/nall licenses still apply. Desktop native builds
use the original composition and no Apple overlay.

Both simulator commands above now compile `libclover_ares.a` and link the NES
and SNES smoke executables. `xcrun vtool -show-build` reports **IOSSIMULATOR**
(SDK 27.1) and **TVOSSIMULATOR** (SDK 27.0), respectively—not macOS executables.
The same commands also succeed for **device** SDKs with these substitutions:

| Build directory | System | SDK | Binary platform |
|---|---|---|---|
| `.build/portability/ios-device` | `iOS` | `iphoneos` | `IOS` |
| `.build/portability/tvos-device` | `tvOS` | `appletvos` | `TVOS` |

Keep `arm64`, deployment target 15.0, Release and smoke tests enabled. The device
builds link both NES/SNES smoke executables, and `vtool` confirms the expected
platform for all eight simulator/device executables. These are link checks, not
device installation, provisioning or execution tests.

The GitHub Actions workflow includes all four **compile/link-only** targets;
their first remote run is still pending. `scripts/test-apple-compat.py` tests
synthetic inputs, unchanged source fixtures, paths with spaces and missing/
duplicate-pattern rejection; the normal native build runs these tests too.

### Remaining runtime gate

No simulator runtime is installed locally (`simctl list runtimes --json` returns
an empty list), so the executables have **not run** on either target. No runtime
installation, simulator device creation, signing setup or device test was
performed. There is no frontend app, controller/audio test or libco runtime
proof from successful linking alone. Device targets also have not been executed
or provisioned for installation; no signing/profile setup was performed.

Next: use simulator app hosts to run generated NES and SNES tests and verify
coroutine ownership, video-worker completion, finite frame/audio output, SRAM,
and deterministic state replay. Device architectures, executable-memory/signing
constraints, lifecycle and physical runtime remain separate gates. S02 is
therefore **partially completed**, not passed.

Do not hide these failures with warning suppressions, platform misidentification,
stubbed success responses or a claim that a Cargo library check covers the core.

## Browser core probe

Not run: `emcc` and `emcmake` are not installed in this environment. The existing
`wasm32-unknown-unknown` library check does not include ares/libco, SDL or XZ.
Toolchain selection, coroutine/threading viability, host memory/storage and
actual browser runtime checks remain open S01/S03a/S05 work.
