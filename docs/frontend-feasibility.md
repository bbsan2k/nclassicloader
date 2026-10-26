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

### Remaining runtime gate at the initial probe

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

## App-hosted runtime follow-up — 2026-10-25

The first Apple app/runtime milestone now passes locally, beyond the earlier
compile/link-only results. See [the implementation plan](apple-host-plan.md) and
[reproduction instructions](../portable/apple/README.md).

Tooling installed with authorization for this work: arm64 iOS/tvOS simulator and
device Rust standard-library targets; iOS 27.0/27.1 and tvOS 27.0 simulator
runtimes. Tests below use **iOS 27.0 on iPhone 18 Pro** and **tvOS 27.0 on Apple TV
4K**, built with Xcode 27.1's iOS 27.1 / tvOS 27.0 SDKs and deployment target 15.0.
The available tools support headless `simctl` execution; a GUI `Simulator.app`
was not found in this Xcode installation. Screenshot/compositor checks do not
depend on that GUI application.

| Probe | iOS simulator | tvOS simulator |
|---|---|---|
| UIKit app-hosted generated NES/SNES tests | Pass, two distinct successful checks | Pass, two distinct successful checks |
| Rust/SDL callback app, shared synthetic scene | Pass, 120 presented callbacks | Pass, 120 presented callbacks |
| Pitch-aware 1280×720 stage checksum | `15617674036659297062` | `15617674036659297062` |
| Captured visible-window validation | Pass | Pass |

`CloverCoreSmoke.app` runs existing generated-program tests on one dedicated OS
thread. Checks include video, input, finite stereo output, SRAM, deterministic
state replay/rejection and the NES wrong-thread rejection. Exceptions are caught
inside app-callable C boundaries; error buffers and invalid artifact roots have
desktop tests. The app creates only its own sandbox artifacts, copied out before
the runner deletes its own temporary simulator.

`CloverApplePreview.app` links a Rust static library and unmodified pinned SDL.
Actual shared card/title/selection/arrow/thumbnail/HUD/bar/toolbar builders feed
the same SDL leaf executor now used by the desktop. Data and bitmap glyphs are
self-authored. A fixed stage is letterboxed to the window; generation checks,
resource destruction and a single-thread opaque-handle ABI are explicit. Native
event callbacks queue normalized flags; main-thread ownership gates prevent
off-thread/reentrant mutable access. Immediate lifecycle delivery is serviced
without waiting for a display callback. Background wins over a coalesced resume.
The probe binds no scene actions and runs no continuous gameplay or audio device.

Review found and corrected two integration failures rather than treating a link
as success: this SDK requires UIScene lifecycle adoption; and the newest iOS
runtime supported only a multi-display phone whose default screenshot could be
blank while the offscreen stage rendered. The runner now selects a conventional
phone/runtime and rejects blank captures independently of the stage checksum.
Four runner unit tests and a blank-image rejection check pass.

Desktop regression evidence after extracting SDL leaf execution: **178 default /
177 portable / 291 native+preview unit tests and 137 integration tests each**;
formatting and warnings-denied Clippy pass. Ten explicitly selected dummy/
software SDL tests pass (nine retained-resource tests plus the new preview ABI/
pause/resume probe). Both native core smoke executables and callable-entry error
tests pass. All 132 UI/language snapshots match outside the launcher busy strip;
three-system suspend/navigation and release SNES/NES 50/60/120 Hz dummy-audio
checks pass. Artifacts/logs are under `.build/apple-app`, `.build/apple-validation`
and `.build/apple-desktop-*`.

Remaining at that first milestone: connect generated-ROM video/audio to the callback host; prove bounded
pacing, actual audio interruptions, input/touch/remote navigation, real-profile
import, durable mobile storage/tvOS eviction recovery and full frontend target
passes. No physical device app, signing account, provisioning, TestFlight/App
Store or remote CI runtime verification was performed. S02 therefore has actual
simulator runtime evidence but is not a complete physical-device/lifecycle gate.

### Generated NES callback follow-up — 2026-10-25

The next vertical slice is implemented as `--app game` in the same SDL callback
app. Both iOS and tvOS simulator runs pass visible-window validation plus
changing generated NES video, finite/non-silent stereo samples, SDL stream
startup, observed queue drainage and bounded buffering/work. The existing NES
smoke generator is shared rather than duplicating ROM bytes. Its program changes
solid backdrop colors and plays a pulse tone; it is not a commercial/full game.

One verified pair of local receipts showed 211 emulated frames on iOS / 225 on tvOS, six video
changes each, zero dropped audio samples and peak input queues of 27,032 / 30,472
bytes respectively (below 38,400). Both pass the injected lifecycle check and
independent visible-window checks. Counts are run-dependent; receipts are in
`.build/apple-app/{ios,tvos}/game-runtime-result.json`, alongside screenshots
and visible-check JSON. The synthetic preview still passes separately on both.

The Rust `AresSession` is created, stepped and destroyed on the host callback
thread; borrowed video is copied before mutating the core again. The elapsed
clock targets 60.0988 Hz independently of display callback rate, caps at three
steps and drops excess debt. The audio queue is capped at 38,400 input bytes
(100 ms, 48 kHz float stereo), and stalls drop new audio instead of growing the
queue. There is no emulator sleep/poll loop or session migration to a dispatch
queue. Pause clears audio/held input; resume discards elapsed debt.

Automated probes inject input and lifecycle events through the same owner-thread
ABI and verify frozen emulation plus an empty queue while paused, no automatic
foreground resume, and subsequent progress. These do **not** certify real OS
background/interruption behavior. A dummy/software desktop test compares actual
uploaded stage checksums after A/B transitions, enforces queue/work bounds and
reopens a core after teardown. Pure timing tests cover 50/60/120 Hz and long
stalls. Current default/portable/native+game suites pass **178/177/292 unit tests**
and **137 integration tests each**. Five runner tests reject stale/malformed
receipts, missing gameplay evidence, unbounded queues and false lifecycle passes.

Physical-device audible output, underrun-free playback, OS audio interruptions,
complete touch/controller/remote navigation, imported real profiles, durable
saves/tvOS recovery and full frontend target passes remain open. No commit,
push, signing-account change or upstream SDL modification was made.

### OS background/foreground follow-up — 2026-10-25

`--app lifecycle` now passes **two OS-driven background/foreground cycles on each
simulator**. The runner launches system Settings, waits in the background, then
returns to the same running app. Observations require unchanged instance/core
generation, frozen emulation and presentation, neutral input, an empty audio
queue and no automatic foreground restart. Explicit resume is injected through
a test-only sandbox command channel; subsequent snapshots require renewed
emulation and audio queue drainage, with at most three frames in the first
resumed callback. Independent final visible-window checks pass.

This closed a host-side lifecycle gap: SDL dispatches lifecycle events immediately,
whereas our original pause waited for an iterate/display callback. The host now
serializes Rust entry through a main-thread ownership gate and services lifecycle
pause without relying on another display tick. Reentrant callbacks wait until
the current Rust boundary returns; off-thread events only queue/schedule work on
the main thread. No handler renders, transfers a live session between threads or
changes SDL. Duplicate platform notifications are tolerated.

Runner validation also rejected a launch-time foreground snapshot masquerading
as a new foreground transition; each tested cycle now requires fresh event
counters. Eight Python tests cover malformed/stale snapshots, restart identity,
automatic resume, nonempty audio, stale input and catch-up regressions. Native
Rust pacing/video/audio/lifecycle/teardown tests continue to pass.

Receipts, complete cycle observations and screenshots are under
`.build/apple-app/{ios,tvos}/lifecycle-*`. These are simulator OS lifecycle tests,
not audio-session interruption (calls/Siri/routes), process-kill recovery,
background durable saves, physical-device suspension or audible-output proof.

### Audio interruption policy follow-up — 2026-10-25

The generated-game host now registers a lifetime-bound
`AVAudioSessionInterruptionNotification` observer. It does not change SDL or take
over SDL's audio-session configuration. Begin notifications enter the existing
owner-thread gate, pause/clear audio and held input, and latch a Rust interruption
blocker. Resume attempts are ignored while that blocker is active. End removes
the blocker but preserves the pause, irrespective of `ShouldResume`; a subsequent
explicit resume resets the clock, primes the stream and starts audio again.

`--app interruptions` injects Foundation begin/end notifications into the real
observer path. Both iOS and tvOS simulator runs pass two cycles and independent
visible-window checks. Run-tagged snapshots check blocked resume during an
interruption, no automatic resume after end, unchanged host/core identity, frozen
frames and empty queues, then bounded explicit resume and renewed SDL drainage.
The existing desktop SDL test also covers duplicate begin/end events and the
resume blocker. Eleven Python tests validate all receipt modes and reject false
interruption/recovery evidence.

In these runs, all four resume attempts during interruption stayed blocked and
all four first callbacks after authorized resume advanced zero emulated frames
(within the three-frame cap). Every resumed stream subsequently drained input
audio again. Receipts are explicitly scoped as injected-notification evidence at
`.build/apple-app/{ios,tvos}/interruptions-runtime-result.json`.

These are **injected-notification integration tests**, not simulator OS-issued
call/Siri interruptions or physical-device audio-session recovery. Audio-route
changes, media-services resets, missing-end delivery, reactivation failures and
audible hardware output remain open. The conservative behavior while an
interruption remains latched is to stay paused, not guess that playback is safe.

### Input ownership follow-up — 2026-10-25

The Apple callback host now opens/tracks/closes SDL gamepads on its owner thread
and reduces input through bounded, synchronized per-source state: keyboard,
eight gamepads, and ten `(touch device, finger)` identities. Releases no longer
cancel another source's held button. Disconnect/remap neutralizes only that
device; focus loss/background/interruption clear held state. Independent input
blockers prevent foreground transitions from overriding interruption/focus state.
Key repeat does not resume gameplay or restore stale input. The left stick has
hysteresis, and simultaneous opposite directions neutralize after merging.

The new standalone C++ test links the pinned SDL archive and passes with actual
SDL virtual-gamepad button/axis/detach/reconnect events, plus reducer tests for
source isolation, capacities, touch cancellation and pause blockers. No SDL
source was changed. The host test also passes with the new wrapper/reducer code
compiled under AddressSanitizer/UndefinedBehaviorSanitizer (the prebuilt SDL
archive itself is not sanitizer-instrumented). `--app input` passes **12 event-to-core checkpoints on both
iOS and tvOS simulators**, with independently validated colored screenshots.
Each app creates virtual gamepads and injects keyboard/touch/focus events through
the normal SDL app callback. Receipts require all checkpoints, changing ares
video and bounded/draining audio; files are under
`.build/apple-app/{ios,tvos}/input-*`. Twelve runner unit tests reject incomplete
or stale evidence across all probe modes.

This is virtual-controller and synthetic-event evidence, not physical MFi/
Bluetooth controllers, real touch delivery or Siri Remote navigation. All pads
currently merge into the single player-one port; this legacy probe maps touch to B.
Player assignment, safe-area-aware on-screen D-pad/buttons and complete non-game
remote navigation remain open. See the probe README for mappings and test commands.

### iOS-first import/play follow-up — 2026-10-25

The iOS SDL host now exposes a small manual raw-NES alpha: **Import NES → Play /
Resume → Stop**. The actual UIKit Files picker feeds coordinated, security-scoped
read-only copying on one worker. Copies are limited to 1 MiB; the shared Rust NES
validator rejects unsupported cartridges before publication. This adapter admits
only NTSC iNES 1.0 because its pacing policy is currently NTSC. Source bytes are
not modified. No worker holds or transfers an ares session.

The private `CloverRawNES/<sha256>/game.nes` store verifies existing content before
reuse and keeps saves separate. iOS uses Application Support; tvOS tests use
evictable Caches and explicitly report no Files picker. This is a raw-cartridge
alpha store, **not** a new Classic profile schema or a completed Classic-image
importer. Loads remain paused until explicit Play. Stop persists SRAM and
releases the core before another selection. Reimport retains the save identity.
Cancellation rejects stale picker identities and worker results; a blocked
provider is kept single-flight until it returns. A late cancellation can leave
validated immutable bytes in the library, but never activates gameplay.

The full **seven-mode matrix passes on both simulators**: core, preview, game,
lifecycle, interruptions, input and import. Import probes present/cancel the actual
iOS picker, inject a delayed selection from that retired picker (ignored), reject
invalid bytes, load/play/stop, verify generated SRAM bytes, suppress cancelled
worker activation, then reimport/play using the same immutable ROM/save identity.
They verify source immutability and independently validate final screenshots.
tvOS runs the same staging/publication/gameplay flow with picker capability false.
Sources and selection callbacks are injected from the test sandbox: external
Files/iCloud/third-party-provider selection, hydration, permission denial and
physical-device behavior remain unverified.

The interactive iOS launch mode was also launched and captured with visible
Import/Play controls, then its owned simulator was shut down/deleted. This does
not count as a manual external-provider selection test. All test-owned simulators
were cleaned; no signing account or physical-device setup was changed.

Six new Rust store tests cover validation-before-write, bounded sizes, NTSC
restriction, immutable reuse/save preservation, symlink rejection and collisions.
Current Rust suites pass **178 default / 177 portable / 298 native+game unit tests**
and **137 integration tests each**. Sixteen runner tests cover all receipt modes
and interactive argument guards. Warnings-denied Clippy and 18 representative
desktop snapshots also pass. Logs, receipts, picker/game screenshots and complete
import observations are under `.build/apple-app/{ios,tvos}/import-*` and
`ios-first-matrix.log`; host checks are under `.build/apple-validation`.

Still open after that import slice (superseded by the follow-up below): a real
shared library/launcher, full on-screen controls, imported
SNES/Classic image profiles, periodic save/suspend/restart recovery, filesystem
fault/power-loss certification and tvOS off-device recovery. SRAM is currently
verified across explicit Stop/reimport, not OS kill/background termination.
Directory syncing is best-effort. No physical-device install, audible playback,
hardware controls or real call/Siri/route recovery is certified. All Apple work
remains uncommitted/unpushed; SDL remains unmodified.

### Foreground browser transfer and profile launcher — 2026-10-26

**Latest iOS/UI policy:** iOS is now Files-first: a native system-styled collection
screen, direct `.hsqs` import, progress/cancel and no LAN listener, URL, PIN or QR
panel. tvOS keeps browser acquisition. Its responsive two-step page now shows
connection state, selected-file size/type validation, upload/import progress,
cancellation and reconnect guidance. Node checks cover these states; WebKit
snapshots at 390/1100 px have no horizontal overflow. Fresh iOS Files/SNES and
tvOS browser NES/SNES end-to-end checks pass. All 18 iOS checkpoints confirm no
LAN listener. Seven Rust transfer tests and 24 runner tests pass. The historical
iOS LAN/pairing flow below is superseded; external Files-provider interaction is
still a manual/device gate.

**Current flow correction:** collection selection now opens the actual imported
Classic skin through the existing Rust `ui::App` renderer/navigation, not a native
game list. The embedded policy cannot create a core or run the desktop pacer;
game IDs are returned to the existing callback-thread Apple host. The flow is
collections → Classic → game → Classic → collections. iOS adds a persistent
Collections/Classic-menu back action; tvOS switches from native collection focus
to SDL navigation inside Classic. LAN transfer stays disabled until returning to
collections. Menu audio, suspend slots and desktop dialogs are not connected.
NES/SNES desktop-versus-embedded exact-pixel/no-core and pointer/key checks pass.
Fresh NES and SNES workflow validation passes on both simulators, including
collection entry, root return/reopen, gameplay, foreground freeze/explicit resume,
return to the same Classic selection, re-pairing and cold collection-screen
restart. Evidence is archived in `.build/apple-app/{ios,tvos}/classic-{nes,snes}`.
Both nine-mode regression reruns also pass (touch, library, import, input,
lifecycle, interruptions, game, preview, core); see platform `classic-regressions.log`.
Current host suites pass 313 native+game unit tests (21 ignored), 137 integration
tests, 24 runner tests, browser pairing checks, formatting and warnings-denied
Clippy. Default/portable suites pass 178/177 unit tests; three native CTest checks pass.
The earlier native-game/explicit-Play results below describe the superseded flow.

The subsequent tvOS GUI revision adds separate library/transfer panes, native
focused game cards, a useful empty state and couch-readable code/instructions.
Pairing now uses a six-digit random ten-minute PIN, exchanged through `POST /pair`
for the independent 128-bit bearer credential kept inside the browser. Five wrong
attempts lock pairing globally for a minute. A native New code action invalidates
both the PIN and existing sessions without altering profiles, and is blocked
during import. QR fragments contain only the six digits. Leading zeros and the
TV's optional grouping space are supported. Existing LAN/origin/foreground rules
remain unchanged; HTTP still requires a trusted network.

Both ordinary Apple launches now show a native profile/game launcher with a
local-IP URL and paired QR upload screen. An authenticated, same-origin-only HTTP
service accepts streamed `.hsqs` request bodies (1 GiB limit), cancellation and
status, with one job/four clients, bounded parsing/timeouts and no filesystem or
remote-play API. It calls the existing validated/cancellable profile importer,
atomically publishes prepared NES/SNES catalogs and removes owned upload sources.
The Apple build now includes static XZ decoding. Opening freshly revalidates the
profile on a worker; callback-thread ares creation uses its verified SMP firmware
and bounded NES/SNES video geometry. External coprocessor firmware is not imported.

The listener closes and cancels work on background and during gameplay. Returning
to the launcher/foreground rotates pairing, and startup scans valid profiles before
showing game titles. A late cancellation past the commit boundary may leave a
valid published profile but never auto-starts a game. Local HTTP is unencrypted;
the UI requires a trusted LAN and warns about tvOS eviction and absent save backup.
This is an initial native launcher, not yet full extracted Classic menu rendering.
iOS supports this browser path plus `.hsqs` Files selection through a coordinated,
security-scoped, read-only descriptor and an independent single-use worker lease.
It shares bounded staging/import and cancellation without transferring a UI/core
handle to a worker. Local selection and existing-profile launch work offline.
The earlier raw-NES Files service remains a separate probe path. Profile metadata
is limited to 64 scanned entries/192 KiB of projected rows with a limit warning.
Public SDL hints enable tvOS native focus in the launcher and Menu/Escape return
from gameplay, without modifying SDL. Physical providers/networking, remote focus, full SNES
touch controls and durable off-device saves remain open.

`--app transfer --image /path/to/image.hsqs` exercises real local HTTP uploads,
authorization/origin rejection, malformed-image preservation, profile identity,
injected native game selection, explicit Play, changing video/bounded audio,
background re-pairing, cold launcher restart, source immutability and source-stage
cleanup. `--app preview --interactive` now supports both platforms and opens
Device Hub on Xcode 27 (Simulator on older Xcode). No content is bundled/downloaded,
no signing accounts or SDL sources changed, and work remains uncommitted/unpushed.
NES and SNES transfer flows pass on both simulators; iOS also verifies a sandbox-
injected source through its coordinated Files worker. Both earlier nine-mode
regression matrices pass. Six transfer tests, all 312 native+game unit tests
(20 ignored), 137 integration tests, 24 runner tests, warnings-denied Clippy and
normal/ASan/UBSan input tests pass. QR codes decode from actual compositor images.
See platform `transfer-matrix.log`, `transfer-verified.log` and archived
`transfer-nes` / `transfer-snes` artifacts. None of this certifies physical-device
network privacy prompts, real provider hydration or actual remote/controller input.

### Owned library and iOS control follow-up — 2026-10-25

The alpha now renders a real owned raw-NES library through the shared Rust
scene/SDL path on both platforms. Scans bound directory entries (256), returned
cartridges (64) and ROM reads (16 MiB), validate digest/NTSC/content identity and
skip corrupt/symlink entries without changing them. Picks require the current
presented revision, owner and window; opening revalidates selected bytes on a
bounded worker before main-thread core creation. The library probe removes its
generated source, cold-restarts the process, compares scene checksums, reopens
stored content with SRAM and excludes/restores a deliberately corrupt owned ROM.

iOS gameplay now shows a safe-area UIKit D-pad/A/B/Start/Select overlay with
independent contacts, diagonal geometry, moves/releases/cancellation and resize,
focus and interruption cleanup. Seventeen injected-contact checkpoints pass on
both simulators with changing video, bounded/draining audio and compositor
screenshots; iOS shows the actual overlay, while tvOS reports no touch UI and
tests the shared contact model. This does not prove real UIKit finger delivery.
Host input/geometry tests pass under ASan/UBSan, runner tests pass 21 cases, and
Rust native+game passes 306 unit tests (20 ignored), plus warnings-denied Clippy.
Artifacts: `.build/apple-app/{ios,tvos}/{library,touch}-*` and `touch-matrix.log`.
The complete nine-mode matrix passes on both platforms: touch, library, import,
input, lifecycle, interruptions, game, preview and core.

Complete Classic frontend/profiles, physical devices/controllers/touches/audio,
real provider selection, periodic saves, kill/background recovery and tvOS
off-device durability remain open. Interactive launch retains only its newly
created simulator and warns when Simulator.app is unavailable; a new invocation
does not reuse another simulator's stored library. No commits/pushes or upstream
SDL changes were made.

## Browser core probe

Not run: `emcc` and `emcmake` are not installed in this environment. The existing
`wasm32-unknown-unknown` library check does not include ares/libco, SDL or XZ.
Toolchain selection, coroutine/threading viability, host memory/storage and
actual browser runtime checks remain open S01/S03a/S05 work.
