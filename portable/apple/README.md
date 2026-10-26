# iOS and tvOS simulator previews

These are development probes, not complete iOS/tvOS products. They bundle no
console content and require no JIT, signing account or device provisioning.
Dependencies stay pinned and unmodified, including SDL.

## Interactive launcher and browser image transfer

Ordinary launches start in a **native collection/import screen**, not a game
browser or generated ROM. Selecting a collection opens its actual imported Classic
skin through the existing Rust renderer/navigation. Confirming a game there starts
gameplay; stopping returns to Classic, and root Back returns to collections.
iOS uses a native Files-first collection screen with **Import .hsqs** and no LAN
listener, pairing code or QR panel. tvOS offers the browser upload address,
pairing code and QR code. The raw-NES alpha and synthetic scenes below remain
probe modes.

```sh
python3 scripts/test-apple-app.py --platform tvos --app preview --interactive
# Or: --platform ios
```

The runner opens Device Hub on Xcode 27 (Simulator.app on older Xcode) and retains
only the device it creates. Allow local-network access if prompted. In a browser
on the same trusted network as Apple TV, scan the QR or open the shown address and enter the
six-digit code (shown as `123 456`). Select your own complete `.hsqs` filesystem image, then Upload and
import. Upload/import progress appears in the browser and launcher. Collections
appear only after verified atomic profile publication. Select one, then select a
game inside Classic. On iOS, **Classic menu** stops the core and writes SRAM;
**Collections** leaves Classic and returns to the Files-first screen. On tvOS,
root Back returns to the collection/transfer screen. Foregrounding paused
gameplay still requires an explicit resume input.
No content is downloaded or included in the application.

On iOS, **Import .hsqs** opens Files directly. A single coordinated, security-scoped
worker supplies a read-only, no-follow regular-file descriptor to the same bounded
staging/import pipeline. The worker owns an independent job lease, not a live core
or UI handle, so cancellation/shutdown cannot leave a dangling service pointer.
Late picker results are ignored; provider coordination may still block, and
cancellation does not promise an instant provider abort. Existing profiles and
local Files import work offline; iOS does not start a LAN listener. Browser
transfer needs Clover to remain foregrounded on a **different device** from the
browser. The initial network service uses private/link-local IPv4 addresses.

Transfers are raw request-body streams bounded to 1 GiB, one job at a time, with
four bounded client handlers, header/inactivity/total timeouts and exclusive
temporary files. The existing Rust image reader/importer validates SquashFS,
prepares NES/SNES catalogs/assets/ROMs and verifies available SMP firmware. The
Apple build now includes its statically linked XZ support. Invalid, interrupted
or cancelled uploads delete only their owned staging source and preserve existing
profiles. No original uploaded image is retained after import. Missing/corrupt
profiles are excluded on restart and freshly revalidated before game loading.
Profile metadata is bounded to 64 scanned entries and a 192 KiB projected list;
the launcher reports when limits omit additional profiles.

The web interface exposes only the upload page, pairing, authenticated status,
upload and cancel—not directory browsing, arbitrary paths or remote gameplay
commands. Entering a six-digit random PIN exchanges it for a separate 128-bit
browser session credential; the PIN is never an upload authorization token.
The PIN expires after ten minutes; five incorrect attempts lock pairing globally
for one minute (changing client addresses does not bypass this). **New code** on
the launcher rotates both the PIN and session credential and invalidates existing
browser sessions while keeping the browser URL unchanged; it is atomically
blocked during an import. QR codes carry only the short
PIN in a fragment, removed by the page. Strict local-IP Host/same-origin checks,
no CORS, no-store responses and a restrictive content policy remain in place.
**HTTP is unencrypted: use a trusted LAN.**
The listener closes and the active job is cancelled on background; returning
rotates the port/pairing code. A commit already in progress may finish publishing,
but never launches a game. Transfers stay disabled throughout Classic and gameplay.

iOS profiles use Application Support; tvOS profiles use evictable Caches. Keep the
source image elsewhere. Saves are separate from imported assets but still lack
off-device backup; **this does not solve tvOS eviction or kill/power-loss recovery**.
SNES games needing external coprocessor firmware are shown unavailable. NES touch
controls remain available on iOS; use a controller for complete SNES controls.
Keyboard: Z/X = B/A, A/S = Y/X, Q/W = L/R; arrows, Return and right Shift retain
their previous mappings. Physical-device networking, remote focus and finger
delivery still require manual testing.
tvOS enables UIKit controller focus in the launcher through public SDL hints and
returns gameplay to SDL input; use Siri Remote Menu/back or keyboard Escape to
return from gameplay to Classic, then back to collections. A controller is
recommended for gameplay.

The tvOS launcher uses a two-pane layout: collection counts and large, highlighted
collection cards on the left; transfer instructions, a large grouped PIN, expiry status,
QR code and New code action on the right. An explicit empty-library panel explains
how to start; inactive progress/cancel controls stay hidden. Native focus animates
the selected collection card. Classic uses SDL input, not UIKit game rows.

The embedded menu never creates a core or starts a second SDL event loop. It emits
game IDs to the existing owner-thread Apple game host. Menu audio, suspend slots,
desktop file dialogs and full SNES touch controls are not connected yet; unsupported
menu actions report that limitation rather than starting desktop services.

Pairing-page checks: `node scripts/test-apple-transfer-page.js` exercises grouped
and leading-zero PIN entry, QR auto-pair, long-session exchange and error/revocation
handling, file size/type validation, progress, cancellation and reconnect handling
without a browser or external dependencies. The responsive page separates pairing
from image selection, shows connection/file/progress states and includes local
network/storage warnings. Rust transfer tests also
cover expiry, global lockout and blocking credential refresh during an upload.

Real-upload simulator checks (never mutate the source):

```sh
python3 scripts/test-apple-app.py --platform tvos --app transfer --image /path/to/image.hsqs --timeout 300
python3 scripts/test-apple-app.py --platform ios --app transfer --image /path/to/image.hsqs --timeout 300
# Two different imports: focus/movement, correct collection, and return focus.
python3 scripts/test-apple-app.py --platform tvos --app transfer \
  --image /path/to/snes.hsqs --extra-image /path/to/nes.hsqs --timeout 420
```

On tvOS they check an empty launcher, real HTTP streaming, authentication/origin rejection,
invalid-image preservation, prepared profile identity, collection entry without a
core, real Classic rendering/navigation, nested game/Classic/collection returns,
bounded/draining audio, changing video, explicit foreground recovery, OS
background/re-pair, cold collection-screen restart and staging cleanup.
Private probe commands exist only with
the explicit run-tagged transfer flag; they are not HTTP endpoints. Evidence lives
under `.build/apple-app/{ios,tvos}/transfer-*`. JSON receipts omit pairing tokens;
screenshots necessarily show the short-lived code displayed by the launcher.
The optional `--extra-image` check imports two distinct images, opens both real
Classic menus by identity and verifies focus survives returns and credential/status
updates without unnecessary table reloads. tvOS movement/confirmation uses queued
SDL key events, not a direct prepare-by-ID call. This is still not physical remote
or controller delivery proof.
The iOS probe instead imports/reimports an owned source URL through its coordinated
Files worker, verifies source immutability/profile reuse/invalid-file preservation,
and requires no LAN listener on initial entry, return or cold restart. Actual external
Files/iCloud provider selection remains a manual/device gate.

Current Classic-flow evidence (2026-10-26): NES and SNES pass on both iOS and
tvOS simulators, including nested returns, foreground pause/explicit resume and
cold restart. Archived receipts and captures are under
`.build/apple-app/{ios,tvos}/classic-{nes,snes}`. These use injected selection/input;
they do not certify physical remote, controller or finger delivery.
Both nine-mode regression matrices also pass; logs are in each platform's
`classic-regressions.log`.
The Files-first UI follow-up passes the full iOS SNES flow with no LAN listener
at all 18 checkpoints, plus the tvOS NES/SNES browser flows. See
`ios/files-first-run.log`, `ios/files-first-snes`, and `tvos/web-refresh*-run.log`.

## Prerequisites

- Xcode with the relevant iOS/tvOS SDK and simulator runtime, CMake and Rust.
- Bootstrap the repository's pinned native sources as for the desktop build.
- For the Rust preview, install `aarch64-apple-ios-sim` and/or
  `aarch64-apple-tvos-sim` for the selected Rust toolchain.

Runtime installation is deliberately not part of the test runner. Xcode's
platform download/install can be large. The runner reports a missing runtime
rather than downloading one or changing signing accounts.

## Run

From the repository root:

```sh
python3 scripts/test-apple-app.py --platform ios
python3 scripts/test-apple-app.py --platform tvos
python3 scripts/test-apple-app.py --platform ios --app preview
python3 scripts/test-apple-app.py --platform tvos --app preview
python3 scripts/test-apple-app.py --platform ios --app game
python3 scripts/test-apple-app.py --platform tvos --app game
python3 scripts/test-apple-app.py --platform ios --app lifecycle
python3 scripts/test-apple-app.py --platform tvos --app lifecycle
python3 scripts/test-apple-app.py --platform ios --app interruptions
python3 scripts/test-apple-app.py --platform tvos --app interruptions
python3 scripts/test-apple-app.py --platform ios --app input
python3 scripts/test-apple-app.py --platform tvos --app input
python3 scripts/test-apple-app.py --platform ios --app import
python3 scripts/test-apple-app.py --platform tvos --app import
python3 scripts/test-apple-app.py --platform ios --app library
python3 scripts/test-apple-app.py --platform tvos --app library
python3 scripts/test-apple-app.py --platform ios --app touch
python3 scripts/test-apple-app.py --platform tvos --app touch
```

Use `--build-only` for compile/link evidence **only**, `--timeout SECONDS` to
bound the receipt wait, and `--keep-device` to inspect the runner's own simulator
after completion. Otherwise it shuts down/deletes only the simulator UUID it
created; it never resets another simulator. Build logs, run-tagged receipts,
simulator metadata and screenshots are under `.build/apple-app/{ios,tvos}`.
Core artifacts are copied from that run's sandbox before the owned simulator
is deleted. Large compiler output stays in `build.log`.

### Core probe

`CloverCoreSmoke.app` uses UIKit's UIScene lifecycle and runs the existing
self-authored NES/SNES smoke checks on one dedicated OS thread. Each test
creates, exercises and destroys its cores there. Generated programs test frame
output, input, finite stereo samples, SRAM and state/rejection behavior. The
thread-affinity negative test remains in the NES checks. Results cross into
Objective-C through bounded C diagnostics, not C++ exceptions.

The app writes an atomic, run-tagged JSON receipt with two distinct test results.
The runner rejects stale receipts, missing/failed systems and wrong platforms.
It copies generated test artifacts for diagnostics. Finite generated samples
are **not** evidence of device audio playback or real-time emulation pacing.

### Shared-scene preview

`CloverApplePreview.app` uses SDL's supported main callbacks and a small opaque
C-to-Rust boundary. A Rust host composes existing public card/title/HUD/thumbnail/
selection/toolbar builders with self-authored textures and bitmap glyphs. The
desktop and preview use the same extracted SDL leaf-command executor. The preview
renders to a 1280×720 stage and letterboxes it to the drawable.

Resource IDs retain checked owner generations. Native resources are destroyed
before their renderer; callbacks stay on the creating thread, return after one
draw, and do not poll events or sleep. Background pauses drawing; foreground does
not automatically resume it. Touch/key/controller input explicitly resumes the
preview. Scene hits remain data: the preview never activates menu/storage/core
actions.

The receipt records presented-callback count and a nonempty pitch-aware **stage**
readback checksum. The screenshot is a separate visible-window observation; a
valid offscreen checksum alone is not window-presentation proof. Prefer the
runner's conventional single-screen phone over multi-display/foldable simulators
when inspecting screenshots. The probe does not test real scene navigation,
audio interruptions, imports, durable saves, controllers or GPU reset recovery.

## Host-side checks

```sh
python3 scripts/test-apple-app-unit.py

SDL_VIDEODRIVER=dummy SDL_RENDER_DRIVER=software SDL_AUDIODRIVER=dummy \
CLOVER_NATIVE_BUILD_DIR="$PWD/.build/native" \
cargo test --locked --manifest-path rust/Cargo.toml \
  --features native-app,apple-preview --lib \
  apple_preview::tests::shared_scene_callback_abi_and_lifecycle \
  -- --exact --ignored --test-threads=1

# Caller-prepared imported assets; checks NES or SNES without modifying them.
SDL_VIDEODRIVER=dummy SDL_RENDER_DRIVER=software SDL_AUDIODRIVER=dummy \
CLOVER_NATIVE_BUILD_DIR="$PWD/.build/native" \
CLOVER_CLASSIC_ASSETS=/absolute/path/to/profile/assets \
cargo test --locked --manifest-path rust/Cargo.toml \
  --features native-app,apple-game --lib \
  ui::apple::tests::embedded_menu_matches_desktop_and_never_creates_a_core \
  -- --exact --ignored --test-threads=1
```

The Classic adapter check compares home/settings/help pixels against desktop,
exercises keyboard and pointer launch requests, and verifies that unsupported
desktop/suspend actions do not create a core. It is separate from simulator
presentation, lifecycle and physical-input evidence.

The `apple-preview` library feature supplies the probe implementation; its mobile
final link is owned by `portable/apple/CMakeLists.txt` (SDL and Apple frameworks).
For host-side tests use `native-app,apple-preview` so Cargo has the normal desktop
native linkage. This is not a general new public Apple frontend API.

### Generated NES video/audio mode

`--app game` runs the same SDL app with a fresh sandbox and the shared,
self-authored `nes_fixture.h` program (no commercial game or firmware). This
fixture displays solid backdrop colors in response to input and produces a pulse
tone; it is deliberately not a full game. Rust creates/uses/destroys ares on the
callback thread. Borrowed core pixels are copied into RGBA before the next step,
uploaded to an SDL texture and displayed at 4:3 within the existing stage.

An elapsed-time accumulator targets NES NTSC rate, caps work at three frames per
callback and drops excess debt rather than catching up after suspension. SDL
copies finite interleaved float stereo samples into its stream, primes before
starting and caps input buffering at 100 ms (38,400 bytes at 48 kHz). A stalled
stream drops new audio rather than growing memory without bound. Receipts record
sample/video-change counts, peak queue, discarded samples and observed input
queue drainage. This proves transport consumption, not audible physical output
or underrun-free playback.

Run-tagged probes inject button transitions and pause/foreground/resume through
the host ABI. While paused the frame count must stay fixed and queued audio
must be empty; foreground alone does not resume. Manual mode (environment
`CLOVER_APPLE_GAME=1`, without `CLOVER_SMOKE_RUN_ID`) omits injected events and
accepts keyboard/gamepad input and legacy B-touch input described below. The
interactive library alpha uses the full iOS overlay described at the end instead.
Physical controller/remote behavior is not validated.
Native events queue normalized data, and a main-thread
ownership gate serializes all Rust entry. Immediate lifecycle delivery pauses
and clears audio without waiting for another display callback; reentrant events
wait until the current Rust boundary returns. Off-thread notifications schedule
service on the main thread. No lifecycle handler renders or moves the session.
The generated-game probe is not evidence of real OS audio
interruptions or background app restoration.

The `apple-game` feature adds native ares to the preview; Apple CMake builds the
archive before Cargo and supplies its per-configuration simulator library path.
The synthetic scene remains available without starting a session. A desktop
ignored SDL test takes `CLOVER_GENERATED_NES` pointing to a caller-generated
fixture and checks video/audio/lifecycle/teardown. Ordinary pacing unit tests
exercise 50/60/120 Hz and long-stall debt rejection.

### OS background/foreground probe

`--app lifecycle` switches the running generated-NES app to the system Settings
app using `simctl`, waits in the background, then brings the **same process** back
to the foreground. It repeats this twice. Run-tagged, instance-tagged snapshots
must show frozen emulation/presentation, neutral input, an empty audio queue and
no stream restart during the foreground wait. Both cycles require fresh OS
event counters, not launch-time notifications. Identity checks reject process or
core-host replacement.

Only explicit resume is injected, through a small sandbox command file enabled
solely by `CLOVER_APPLE_LIFECYCLE=1` plus a test run ID. It accepts only sequential,
matching-run resume commands; it is not a production importer or network service.
The first resumed callback is limited to three emulated frames; subsequent
snapshots must show emulation progress and renewed SDL queue drainage. JSON
observations, sandbox probe snapshots and an independently validated final
screenshot are kept under `.build/apple-app/{ios,tvos}/lifecycle-*`.

This exercises real simulator OS background/foreground delivery, not injected SDL
lifecycle events. It does not prove hardware audio interruptions (calls/Siri/audio
route changes), audible output, process-kill restoration, background saving or
physical-device suspension behavior. The app does not request background audio
or background execution privileges.

### Audio-session interruption policy

The generated-game host observes `AVAudioSessionInterruptionNotification` while
it owns a session and removes the observer during shutdown. It does not replace
SDL's audio-session configuration or observer. Notifications use the same
main-thread/reentrancy gate as lifecycle events; callbacks from other threads
only queue state and schedule owner-thread service.

Beginning an interruption pauses emulation, clears queued audio and held input,
and latches a **resume blocker** in Rust. Touch/key input cannot resume the game
while that blocker is active. Ending the interruption removes the blocker but
keeps the game paused, including when Apple's notification sets `ShouldResume`.
Explicit resume then resets pacing debt, re-primes audio and starts playback.
Background and interruption blockers remain independent: an interruption ending
does not authorize background playback.

`--app interruptions` injects begin/end **notifications**, rather than simulating
phone calls, through the installed Foundation observer path. Each of two cycles
checks empty queues/frozen frames, a blocked resume attempt, no automatic resume
after an end notification carrying `ShouldResume`, stable core identity, and
bounded explicit resume with renewed audio queue drainage. Commands are enabled
only in a run-tagged probe sandbox. Results are explicitly labelled injected;
artifacts are under `.build/apple-app/{ios,tvos}/interruptions-*`.

These checks do not prove OS-issued call/Siri interruptions, actual audio-route
changes, media-services reset recovery or audible physical-device playback.
Missing interruption-end delivery and audio-session reactivation failures also
need device testing; the safe current behavior is to remain paused rather than
guess that playback is authorized.

### Source-aware input

The host initializes SDL gamepads, opens up to eight devices on its owner thread,
rescans hotplug events and closes handles before SDL shutdown. A bounded reducer
keeps keyboard, each gamepad and up to ten fingers separate. Releasing one source
cannot release another source's button. Disconnect/remap clears only the affected
controller; pause/focus loss/interruption clears all held input. Blockers are
independent and synchronized, so foreground alone cannot bypass an interruption.
Key repeat cannot resume or restore stale input. All controllers currently merge
into the **single player-one port**; player assignment is not implemented.

Mappings: Z/X → B/A, Return/right Shift → Start/Select, arrows → D-pad. Gamepad
south/east → B/A, Start/Back → Start/Select, D-pad and left stick → directions.
The stick uses hysteresis (engage at 16,000; release inside 12,000). Opposite
directions neutralize after merging. Stick movement alone does not resume a
paused game; a supported button/key/touch press is required. Touches still map
only to B, but releases/cancellation are tracked by both touch device and finger
ID. This legacy input probe is separate from the alpha's full iOS touch overlay.

Host-side tests use the pinned SDL build (no SDL edits):

```sh
cmake -S portable/apple/tests -B .build/apple-input-tests \
  -DSDL3_DIR="$PWD/.build/native/third_party/SDL"
cmake --build .build/apple-input-tests --parallel 3
ctest --test-dir .build/apple-input-tests --output-on-failure
```

`--app input` creates SDL virtual gamepads inside the simulator app and injects
keyboard/touch/focus events through SDL's normal app callback. Twelve checkpoints
check cross-source releases, stick/button detach neutralization, reconnect,
focus pause, ignored key repeat, explicit resume, and independent finger
release/cancellation. The resulting normalized mask feeds ares, not a fake core.
Receipts require changing video, bounded/draining audio and a colored compositor
screenshot. These are virtual-controller/synthetic-event tests, **not physical
MFi/Bluetooth controllers, Apple TV Siri Remote or real multitouch evidence**.

### Earlier raw-NES import/play alpha (probe path)

The `--app import` / `--app library` probe path shows the real owned NES library. iOS provides
**Import NES → Play / Resume → Stop**, plus Prev/Next/Open controls. The shared
scene supports row selection and keyboard/gamepad navigation on both platforms;
tvOS has no Files picker. For the new interactive profile/upload launcher use:

```sh
python3 scripts/test-apple-app.py --platform ios --app preview --interactive
```

This intentionally retains only the simulator it creates and prints its UUID and
cleanup commands. It is an interactive launch, **not automated test evidence**.
A GUI is needed for manual interaction; the runner attempts to open the selected
Xcode's Device Hub or Simulator and warns if only a headless app is available.
Interactive metadata is written separately as `interactive-simulator.json`. Each
invocation creates a fresh simulator; libraries persist within a retained app
container, not across newly created devices. Xcode 27 replaces `Simulator.app`
with Device Hub; its absence does not mean Xcode is incomplete. Signing/device deployment
is not configured by this command.

The iOS Files picker accepts one file URL. A single worker coordinates provider
reads, keeps security-scoped access alive for the copy, and stages at most 1 MiB
without mutating the source. The existing Rust NES validator accepts standard
uncompressed iNES 1.0 cartridges for enabled stock mappers. The alpha rejects PAL
timing because this callback adapter currently targets NTSC. Archives, NES 2.0,
FDS, Classic console images, SNES content and firmware import are not supported
by this picker yet. This is a raw-cartridge alpha path, **not the Classic profile
importer or a replacement desktop schema**.

Validated bytes are published under a private content-addressed `CloverRawNES`
directory: Application Support on iOS; explicitly evictable Caches in tvOS tests.
Reimport verifies existing ROM bytes and reuses its saves directory without
overwriting saves. Staging is exclusive and cleaned only when owned by this job.
The immutable ROM is separate from mutable SRAM. Directory syncing is best-effort;
this is not complete power-loss durability, storage quota or tvOS backup/recovery
certification.

No worker owns a live core. Only the main callback consumes a prepared path and
creates the ares session. Loading stays paused until explicit Play. Stop unloads
the core and persists its SRAM; stop before selecting another cartridge. This
slice does not yet provide automatic running-game restart recovery, periodic saves
or suspend-state UI. SRAM persistence is verified across explicit Stop/reimport
and stored-cartridge reopen, not process kill or background termination.

Cancelling invalidates the picker identity and any pending worker result; late
delegate callbacks cannot activate a session. The worker remains single-flight
until a blocked provider read finishes, so cancellation does not spawn unbounded
threads. A late cancellation can leave an already validated immutable cartridge
in the library, but never starts gameplay. Background/teardown cancel outstanding
work. External providers can block coordination; cancellation is not a guaranteed
instant provider abort.

`--app import` uses sandbox-generated source URLs, verifies invalid rejection,
picker cancellation/stale delegate rejection, explicit play, SRAM Stop/reimport,
source immutability and cancelled-worker suppression. iOS additionally presents
and captures the actual system picker before injecting cancellation. tvOS runs
the shared staging/publication/playback test with `picker_supported=false`; it
does **not** pretend to offer Files acquisition. Both modes label selection as
injected: real Files/iCloud/third-party-provider selection, denial and hydration
still need manual/device testing. Artifacts are under
`.build/apple-app/{ios,tvos}/import-*`.

### Owned library and iOS controls

The real library is derived by read-only scans bounded to 256 directory entries,
64 returned cartridges and a 16 MiB ROM-read budget. Hash-labelled rows use the
shared Rust scene/SDL renderer; selecting an identity requires a current owner,
window and presented revision. Opening freshly revalidates its bytes before core
creation. Corrupt/symlink/mismatched entries are skipped without repair/deletion.
`--app library` deletes only its generated source, restarts the process, compares
the real rendered library and reopens stored content with retained SRAM. It also
tests exclusion/restoration of its deliberately corrupted owned fixture.

During alpha gameplay iOS shows a UIKit safe-area D-pad (including diagonals),
A/B, Start and Select overlay. Ten contacts retain independent masks; drags,
releases, cancellation, resize, focus loss, interruption and shutdown clear the
appropriate state. Raw SDL finger events are suppressed there to avoid duplicate
B presses. A 150-point band needs at least 500 points width and 240 points of root
safe-area height; unsupported sizes show controller-required guidance. tvOS keeps
keyboard/gamepad input and never installs this touch UI.

`--app touch` checks 17 injected contacts/policy checkpoints on both simulators,
with changing video, bounded/draining audio and colored compositor screenshots.
iOS captures the visible overlay; tvOS exercises shared geometry/contact handling
with `touch_ui_supported=false`. This is **not real UIKit finger-delivery proof**.
Standalone geometry/source/blocker and SDL virtual-controller tests also pass
under ASan/UBSan. Artifacts use `library-*` and `touch-*` under each platform's
`.build/apple-app` directory.

Next: complete the Classic shared frontend/profile flow,
verify hardware interruption/route/input and provider behavior, Classic image ingestion and
tvOS off-device save recovery. See
[`docs/apple-host-plan.md`](../../docs/apple-host-plan.md).
