# Frontend split: small-agent task queue

This queue implements [the architecture decision](frontend-architecture.md).
**Unchecked tasks are planned, not implemented.** Names of destination modules
below are proposed locations, not existing APIs. Preserve desktop behavior and
keep each task independently testable and revertible.

## Current evidence

### Apple runtime checkpoint — 2026-10-25

**Repository publication polish — 2026-10-26:** the public overview now describes
Files-first iOS and browser-based tvOS correctly. Contribution/security/reporting
guides, a documentation index, changelog and [publication checklist](publishing.md)
are in place. Content/index/history audits, immutable workflow action references,
portable/browser tests and Apple compile/link CI are configured. Apple bundles
include complete native/Rust/font/flag notices with package-content verification.
Apple source formatting is standardized, and six native CTest targets now include
the shared input/touch and callable core-entry checks. Current local preparation
evidence is recorded in the checklist; hosted CI and physical-device gates remain
open. Publication requires explicit owner approval; no release tags or history
rewrites are part of this preparation.

**Multi-collection focus correction (current, verified):** tvOS
remote Select now activates the focused collection, never a gesture's pointer
location. Pointer taps use a separate touch-only recognizer. Preferred focus is
remembered by collection identity instead of always choosing the first row;
one-shot focus requests clear on the actual focus callback, not the next poll.
Status/job/pairing revisions no longer reload identical collection rows. SDL
Up/Down fallback routes to native focus, and duplicate opens are latched while
the worker validates a selected collection. Native collection screens no longer
render/present the hidden SDL preview underneath UIKit; the profile host requests
60 Hz callbacks. Region/version details distinguish imports on both platforms.
Native SDL navigation uses a bounded, generation-checked ordered key queue rather
than coalesced direction flags, preserving rapid input order. The transfer runner
accepts `--extra-image` to verify two real imports, movement, focus stability
across status/credential updates, correct menu identity, rapid input and return
focus. Both iOS multi-import runs pass. The tvOS test found a synchronous UIKit
default-focus callback overwriting the second-row return target; restoration now
uses a captured identity on the next main-queue turn, guarded against late work.
The complete tvOS test passes, including rapid alternating keys, both menu
identities, both return targets, stable row reload counts, gameplay and cold
restart. Evidence: `tvos/multi-collection-verified.log` and the corresponding
`multi-collection-verified` archive. All 25 runner tests and browser-page checks
pass; the latest iOS build also passes. Existing previews were updated in place:
iOS `655F524C-146A-4E7F-A30D-6B269917049A` (96 files unchanged), tvOS
`DD4A8779-A717-434E-8CBB-4E91820D6AD9` (192 files unchanged, Japan/USA collections).
The test-only tvOS device `C91F6B4C-4EE8-401E-8D15-8044430925E3` is also retained.
Actual remote/controller responsiveness still awaits user/device confirmation.

**Files-first iOS / browser polish (current):** iOS now uses a native, system-styled
Files import/collection screen with an empty state and import progress/cancel.
It never starts the LAN listener and has no URL, PIN, QR or TV transfer panel.
The browser transfer page remains for tvOS, with responsive two-step cards,
connection/file details, upload/import progress, clear limits and local-network
warnings. Browser tests cover pairing/revocation, file size/type validation,
progress, cancellation and reconnect; all 24 runner tests and seven Rust transfer
tests pass. WebKit mobile/desktop snapshots have no horizontal overflow. Fresh
iOS Files/SNES and tvOS browser/Classic NES/SNES end-to-end checks pass in
`files-first-run.log` and `web-refresh*-run.log`. All 18 iOS checkpoints report no
LAN listener, including initial entry, nested returns and cold restart. Native
screenshot checks now measure contrast rather than requiring a dark/colorful
screen and reject solid black/white frames. Earlier iOS LAN screenshots/checkpoints
are superseded; actual external Files-provider interaction remains a manual gate.

**Classic navigation correction — 2026-10-26 (current):** the native initial
screen selects collections, not games. A callback adapter embeds the existing
Rust `ui::App` renderer/navigation with the imported Classic assets. It owns no
core, desktop pacer, second event loop, controller handle or import worker.
Confirm emits a game ID to the existing Apple host; Stop/Menu/Escape returns to
Classic, and root Back (or iOS Collections) returns to acquisition and re-pairs.
LAN transfer remains disabled while in Classic. Embedded menu audio, suspend
slots and desktop file dialogs remain explicitly unsupported. Desktop/embedded
exact-pixel comparisons and no-core navigation pass for both NES and SNES.
Current host tests pass: 313 native+game unit tests, 21 ignored, plus 137 integration
tests and 24 runner tests; browser checks, formatting and warnings-denied Clippy
also pass. Default/portable suites pass 178/177 unit tests. Fresh NES and SNES
end-to-end flows pass on both iOS and tvOS. Both nine-mode regression reruns pass
(touch, library, import, input, lifecycle, interruptions, game, preview, core).
Evidence is archived under platform `classic-{nes,snes}` and
`classic-regressions.log`. The direct native-game/extra Play flow in the historical checkpoints
below is superseded; physical-device and tvOS durability gates remain open.
Fresh iOS interactive preview `655F524C-146A-4E7F-A30D-6B269917049A` is open in
Device Hub and retained at the collection/import screen. It has a fresh sandbox;
older retained simulators and their imported libraries were not altered. The
launch log is `.build/apple-app/ios/classic-interactive.log`. All work remains
uncommitted/unpushed, with SDL unmodified.

**tvOS activation follow-up:** a user found that the focused collection did not
activate with Select/Return. Earlier probes requested focus but opened by ID,
missing the production event path. SDL-delivered Return now activates only a
focused native collection, with foreground/focus/busy guards; a native Select/tap
recognizer shares the same row action. Returning to collections requests table
focus. The tvOS transfer probe now opens/reopens via queued SDL Return events,
not a direct prepare-by-ID call, and the full SNES flow passes
(`.build/apple-app/tvos/selection-run.log`). Both platform builds and 24 runner
tests pass. The existing interactive tvOS simulator
`DD4A8779-A717-434E-8CBB-4E91820D6AD9` was updated in place; all 96 existing
collection/save file digests were verified unchanged. Actual user remote/click
delivery still awaits confirmation.

**tvOS launcher / short pairing follow-up — 2026-10-26:** the native tvOS UI now
separates library cards/counts/empty state from a dedicated transfer panel, with
larger text, mint focus highlighting, a grouped six-digit code and explicit
expiry/New code controls. Both platforms exchange this random ten-minute PIN via
`POST /pair` for a separate 128-bit browser bearer credential. Five incorrect
attempts lock pairing globally for one minute. Refresh/background/gameplay
invalidate sessions; refresh is unavailable during an active import. QR codes
carry the six-digit PIN, not the long session secret. The web form preserves
leading zeroes and accepts the displayed space separator. New probe coverage
checks wrong-code rejection, session exchange, New code invalidation and actual
native tvOS focus requests; hardware remote delivery remains a separate gate.
Credential refresh keeps the existing browser URL and rejects refresh atomically
while an upload/import is active. Rust tests cover the five-attempt lockout, expiry,
leading zeroes, old-session revocation and the import/refresh race. The Node page
test covers spaced PIN entry, QR auto-pair and disabling upload on lost sessions.
NES/SNES pairing/upload/play flows pass on both simulators, including New code,
old-session rejection and native tvOS focus highlighting. Latest artifacts are
under `.build/apple-app/{ios,tvos}/launcher-{nes,snes}` and `launcher-verified.log`.
All 313 native+game unit tests (20 ignored), 137 integration tests, 24 runner tests,
the Node page test, formatting and warnings-denied Clippy pass. Updated interactive
tvOS preview `71257A69-1D87-4AEF-9B9E-825A76F58116` is retained in Device Hub;
existing user simulators/libraries were not replaced or deleted. Its compositor
QR also decodes successfully. Work remains uncommitted/unpushed.

**2026-10-26 browser-transfer follow-up:** ordinary launches on both platforms now
start in a native Classic profile/game launcher with a local upload URL, paired QR
code and progress. A foreground-only Rust HTTP service streams at most 1 GiB into
exclusive temporary files and calls the existing cancellable SquashFS/profile
importer. It has one job/four clients, bounded headers/timeouts, strict local Host
and Origin checks, 128-bit browser sessions and no arbitrary file/game-control endpoints.
Background closes the listener/cancels work and invalidates pairing; gameplay also
disables transfers. Imported NES/SNES catalogs are listed with title/availability;
opening revalidates a profile on a worker, then creates the existing ares session
on the callback thread. Explicit Play is still required. SNES now uses bounded
dynamic frame geometry and the profile's verified SMP firmware; games requiring
external coprocessor firmware remain unavailable. The Apple build includes XZ.

Real HTTP image-upload probes cover invalid preservation, identity, explicit-only
play, video/audio bounds, background re-pairing, source immutability, owned-stage
cleanup and cold restart into the launcher. The interactive runner now supports
tvOS and Xcode 27 Device Hub; `--app preview --interactive` is the user path,
not `--app library` (which ends in the synthetic color fixture). This remains an
initial native launcher, not the complete extracted Classic skin/menu UI. No
off-device save backup, tvOS durability, device signing or physical networking/
remote/finger-delivery certification is implied. See the Apple README for commands
and `.build/apple-app/{ios,tvos}/transfer-*` for local evidence.
The iOS launcher also supports `.hsqs` through Files, using a coordinated,
security-scoped read-only descriptor and a single-use Arc-backed worker lease.
Local imports and existing-profile launches work without a LAN listener. Unit
tests cover cancellation/shutdown lease lifetime and offline opening; sandbox
injection tests do not certify real external-provider selection. Metadata is
bounded to 64 scanned entries/192 KiB of projected profile rows with a limit
warning. Public SDL hints enable native tvOS focus in the launcher; Menu/Escape
returns from gameplay. SDL remains unmodified.
Verification: NES and SNES image-transfer flows pass on both simulators, including
coordinated local Files reimport on iOS. Both nine-mode regression matrices pass.
QR codes decode from compositor captures. All 312 native+game unit tests (20
ignored), 137 integration tests, six transfer-specific tests, 24 runner tests,
warnings-denied Clippy and normal/ASan/UBSan input tests pass. Default/portable
remain 178/177 unit tests with 137 integration tests each. Current transfer
artifacts are also archived in each platform's `transfer-nes` / `transfer-snes`
directories. Interactive tvOS device `7B07C402-037B-4B97-A38B-38E59B160D27` was
launched through Device Hub and retained with an empty real upload launcher; do
not confuse it with the user's older retained color-fixture simulator or delete
either as part of ordinary test cleanup. No commits or pushes were made.

After the extraction commit `85a4394`, separate UIKit generated-core checks and
the SDL/Rust shared-scene app pass on both iOS and tvOS simulators. The callback
host now also has a **generated NES video/audio mode** (`--app game`): the shared
self-authored fixture feeds the existing thread-affine `AresSession`, copied
video uploads and a bounded SDL float-stereo stream. Core creation, stepping and
destruction stay on the callback thread. Elapsed-time pacing caps work at three
frames per callback, discards excess debt and is tested at 50/60/120 Hz. Audio
input buffering is capped at 100 ms; injected pause clears it and neutralizes
input, foreground alone stays paused, and explicit resume resets the clock.

The runner checks run-tagged core/video/audio/lifecycle receipts and independent
compositor screenshots. Generated-game receipts require changing core video,
finite non-silent audio, started/draining SDL buffering and bounded queue/work.
The desktop SDL game test additionally compares uploaded stage pixels and
reopens the core after teardown. These are simulator/transport and injected
lifecycle tests, **not physical-device sound, OS interruption, controller/remote,
real-content import or durable-save certification**.

Current checks: 178 default / 177 portable / 313 native+game unit tests and
137 integration tests per configuration; warnings-denied Clippy and selected SDL
tests. Earlier Apple extraction regressions also passed all 132 desktop
snapshots, three-system suspend/navigation and release SNES/NES audio checks.
See [probe reproduction](../portable/apple/README.md),
[runtime evidence](frontend-feasibility.md) and [remaining plan](apple-host-plan.md).
Apple work remains uncommitted and unpushed. S02, full frontend target passes,
L21/L22, S07, Checkpoint C, physical devices, ingestion and tvOS save recovery
remain separate gates.

Lifecycle follow-up adds a main-thread Rust-entry gate: SDL's immediate lifecycle
events can pause/clear the core audio before display callbacks stop. Reentrant
delivery is deferred until the active Rust boundary returns; off-thread delivery
is scheduled on the main thread. `--app lifecycle` drives two actual OS
Settings/foreground cycles and observes fresh event counters, stable host/core
identity, frozen frames, neutral input and empty audio while paused. Only explicit
resume is sandbox-injected. Audio-session interruptions and physical-device
behavior remain separate from this background/foreground simulator gate.

Audio follow-up adds an `AVAudioSession` interruption observer without replacing
SDL's configuration or observer. Begin pauses/clears input and audio and latches a
Rust resume blocker. End clears that blocker but does not resume, even when
`ShouldResume` is present; explicit resume resets pacing and re-primes audio.
`--app interruptions` tests two injected notification cycles through the actual
observer path, including a blocked resume attempt. These are notification/policy
tests, not OS-issued calls/Siri, route-reset or physical-device evidence. The
observer is removed on teardown; callbacks never borrow the core off-thread.

Input follow-up replaces the shared held-button mask with bounded source-aware
state. SDL gamepads are opened/rescanned/closed on the owner thread; keyboard,
each pad and touch device/finger identities merge without cross-source releases.
Disconnect/remap clears the affected device, stick directions use hysteresis and
opposites neutralize. Focus loss, background and interruption clear held input;
independent synchronized blockers prevent stale key repeat from resuming play.
The new standalone C++/SDL test uses a virtual controller; `--app input` adds
twelve simulator event-to-core checks. Physical controllers/remote input, player
assignment and physical touch delivery remain open; the alpha overlay follows below.

**iOS-first raw-NES alpha:** a UIKit Files-picker service now feeds bounded,
single-flight coordinated staging and the shared Rust NES validator/content store.
Main-thread commands alone load/play/stop the existing ares host; loads stay paused
until explicit Play. iOS uses Application Support, while tvOS staging tests use
evictable Caches and expose no Files picker. Cancellation invalidates picker/job
identity, ignores delayed selection/results and never transfers a session to a
worker. Reimport preserves content/save identity. This is raw NTSC iNES only,
separate from Classic profile/image importing. The shared runtime/store is tested
on both simulators; iOS additionally exercises picker presentation/cancel, with
source selection injected from the probe sandbox. Provider selection, device
signing, the complete Classic shared frontend/profile flow and durable
background/restart recovery remain open. No SDL changes or commits were made.

**Owned library and iOS touch follow-up:** interactive launches use a real
hash-labelled cartridge scene, not synthetic cards. Bounded read-only scans skip
invalid entries; selected digests are freshly revalidated before opening. Both
simulators pass source removal/cold process restart, identical rendered library,
stored reopen/SRAM retention and corrupt-entry exclusion/restoration probes.
iOS now has a safe-area UIKit D-pad/A/B/Start/Select overlay with independent
contacts, dragging, cancellation, resize/focus/interruption cleanup and explicit
resume. Seventeen injected checkpoints pass on both simulators; tvOS tests shared
geometry/contact handling without a touch UI. Real finger delivery remains open.
Standalone SDL/input/geometry tests pass normally and under ASan/UBSan; all 21
runner unit tests and warnings-denied Clippy pass. See `library-*`, `touch-*` and
`touch-matrix.log` under `.build/apple-app/{ios,tvos}` for current evidence.
All nine modes (`touch`, `library`, `import`, `input`, `lifecycle`,
`interruptions`, `game`, `preview`, `core`) pass on both simulators.
Interactive runs attempt Simulator GUI launch and explicitly warn if headless;
each runner invocation creates a fresh device/container, so it does not inherit
another retained simulator's library. Device signing, provider selection, complete
profiles, periodic saves and tvOS off-device durability remain separate gates.

### Apple host research — 2026-10-25

[The Apple host plan](apple-host-plan.md) compares pinned RetroArch/Provenance
source with this repository and proposes early app-hosted core/runtime probes,
then an SDL-backed Apple callback host. It includes a first-ten-slice plan,
current linking/toolchain blockers and separate iOS ingestion/tvOS recovery gates.
This is a proposal, not completed S02/S03/S07/Checkpoint C or app support.
No simulator runtime or Rust target was installed during research; source-only
reference inspection is not an upstream-app runtime test.

### Extraction checkpoint — 2026-10-25 (L21ao–ax, ten chunks)

Ten chunks extract the generic modal panel (`render_panel`): **L21ao** row
viewport policy; **L21ap** bound row commands; **L21aq** footer commands;
**L21ar** heading/body capture; **L21as** owned panel composition and native
execution; **L21at** read-only native projection; **L21au** external whole-panel
tests; **L21av** native key-binding tests; **L21aw** executor fault tests;
**L21ax** retained panel resource replay. Luna implemented the first six slices;
Sol completed the last four directly at the user's request, reviewed the
projection and fixed a row-position regression before final verification.

Panel execution validates scene/context/current-assets generations before any
pixel, hit or keyboard-action replacement. Native bindings accept captured keys
only from the projected panel's allowed set; keys are never catalog indices.
Visible rows provide hits while all entries retain their original keyboard-action
order, followed by an enabled footer. Disabled import footers still draw but
have no hit or action. Profile/language enumeration, wrapping, translations,
focus mutation, cancellation eligibility and effects remain native. Native SDL
dialog rasterization and suspend/toolbar-specific resources remain unchanged.

Verification: **178 default / 177 portable / 290 native unit tests and 137
integration tests per configuration** pass. Formatting, diff checks and warnings-
denied portable/native Clippy pass. All **nine** opt-in SDL resource tests pass
with caller-supplied dummy video/software drivers. New tests verify panel
ordering/ownership, stable keys, scrolled keyboard actions, native allow-list
binding, owner rejection before replacement and fresh-assets/canvas replay.
The replay uses the production row-projection helper and asserts distinct row
positions, covering the regression found during review; message, scrolled-list,
enabled-footer and disabled-footer cases retain identical pixels/hits/actions
after resource recreation. This is synthetic software-SDL evidence, not GPU
context-loss or native font/driver failure certification.

All **132 UI/language snapshots** (50 SNES, 48 NES, 34 Famicom) match fresh
pre-L21ao baselines outside the elapsed launcher busy strip; 18 representative
snapshots also match after every production slice and final review. Three-system
real-core suspend/save/lock/overwrite/delete/replay/restart/navigation checks
pass. Release SNES/NES dummy-audio checks pass at 50/60/120 Hz with no queue
underruns. Logs and artifacts are under `.build/l21ao*` and `.build/l21ax-after`.
Importer fault matrices and physical-device/runtime gates were not rerun. Nothing
was committed or pushed.

**Next:** distinct toolbar-panel and suspend/confirmation presentation, followed
by App target-pass contracts. Continue direct Sol work unless redirected.
**L21 remains partial**; L22, S07, L25, Checkpoint C and Apple/browser runtime
gates remain separate.

### Previous checkpoint — 2026-10-24 (L21ae–an, ten chunks)

Ten serial Luna chunks extend owned composition through the toolbar and overlay
boundary: **L21ae** toolbar layout/focus policy; **L21af** tooltip commands;
**L21ag** bound toolbar scenes and closed hit tags; **L21ah** native read-only
toolbar projection; **L21ai** combined base/toolbar composition; **L21aj** closed
overlay routing; **L21ak** owned notice presentation; **L21al** external
composition/ownership tests; **L21am** native toolbar/notice executor faults;
**L21an** retained combined-menu, tooltip and notice replay.

Base and toolbar generations must agree before merging, then the existing native
executor checks the combined scene/context/assets owner before any draw or hit
append. Per-item active/icon/hit/tooltip order, simultaneous hover and keyboard
tooltips, open-panel suppression and exact legacy focus strings are preserved.
Toolbar hit tags bind only the five existing native actions. Notice glyphs and
dialog geometry are owned data with an owner check before drawing; dialog fill
and three SDL rectangle outlines retain their original native rasterization.
Overlay routing preserves native dispatch order and hit/action clearing behavior;
it does not extract suspend or toolbar-panel payloads or authorize effects.

Verification: **178 default / 177 portable / 288 native unit tests and 121
integration tests per configuration** pass. Formatting, diff checks and warnings-
denied portable/native Clippy pass. All **seven** opt-in SDL resource tests pass
when explicitly invoked with dummy video/software rendering. New tests exercise
closed toolbar hit mappings, whole-scene/context rejection, missing/stale
individual resources, notice rejection before native dialog drawing, warm replay
and fresh-owner pixel/hit parity. Sol strengthened actual mixed-generation,
missing-resource execution, two-tooltip and retained-command assertions.

Each production chunk and the final batch preserve the 18 representative
snapshots. All **132 UI/language snapshots** (50 SNES, 48 NES, 34 Famicom) match
fresh pre-L21ae baselines outside the elapsed launcher busy strip. Three additional
notice snapshots match exactly across the extraction. Three-system real-core
suspend/save/lock/overwrite/delete/replay/restart/navigation tests pass. Release
SNES/NES dummy-audio checks pass at 50/60/120 Hz with no queue underruns. Logs and
artifacts are under `.build/l21ae*`, `.build/l21ak-notice-*` and
`.build/l21an-after`. Software-SDL/headless checks do not certify physical GPUs,
audio hardware, general context-loss recovery or Apple/browser runtime. Importer
fault matrices were not rerun; nothing was committed or pushed.

**Next:** extract remaining modal/toolbar-panel presentation in small slices,
preserving native action binding, hit replacement and suspend target ownership.
**L21 remains partial:** full overlay payloads and App target-pass contracts,
L22, S07, L25, Checkpoint C and Apple/browser runtime gates remain separate.

### Previous checkpoint — 2026-10-24 (L21u–ad, ten chunks)

Ten serial Luna chunks complete the base-menu boundary: **L21u** bound
thumbnails; **L21v** bound four-item HUD; **L21w** menu bars; **L21x** owned
ordered base-frame composition; **L21y** native generation-checked execution
and current-catalog hit binding; **L21z** read-only native projection;
**L21aa** external frame tests; **L21ab** actual executor fault tests;
**L21ac** complete-frame native resource replay; **L21ad** shared reverse-
priority, half-open hit selection used by native hover and click handling.

The base frame ends before toolbar/tooltips and modal overlays. Game hits retain
captured keys, never catalog indices; native binding ignores absent keys. Frame,
binding-context and current-assets generations must agree before any draw or
hit append, including fills. Individual invalid textures still skip only their
draws. Firmware/ROM checks retain candidate order before culling; thumbnails
retain their separate firmware-only guard. Clocks, activation, double-click game
start, stage targets and emulator/audio ownership remain native. Projection and
composition allocate; this is not a new frame-budget or atomic-filesystem claim.

Verification: **178 default / 177 portable / 288 native unit tests and 109
integration tests per configuration** pass. Formatting, diff checks and warnings-
denied portable/native Clippy pass. All **five** opt-in SDL resource tests pass
when explicitly invoked with caller-supplied dummy video/software drivers. New
executor tests verify rejection before pixel/hit mutation; full-frame replay
matches pixels and native hit bindings after warm replay and assets/canvas
recreation for normal, suspend and NES cases. These synthetic software-SDL tests
do not certify arbitrary GPU context loss, physical devices or driver failures.

Production composition/executor/projection changes and the final batch preserve
18 representative snapshots. All **132 UI/language snapshots** (50 SNES, 48 NES,
34 Famicom) match fresh pre-L21u baselines outside the elapsed launcher busy
strip. Three-system real-core suspend/save/lock/overwrite/delete/replay/restart/
navigation checks pass. Release SNES/NES dummy-audio checks pass at 50/60/120 Hz
with no queue underruns; physical audio and earlier debug timing caveats remain.
Logs and artifacts are under `.build/l21uad-*`; importer fault matrices were not
rerun for this presentation-only batch. Nothing was committed or pushed.

**Next:** extend owned composition to toolbar/tooltips and modal overlays, then
address complete App target-pass contracts. **L21 remains partial**; L22, S07,
L25, Checkpoint C and Apple/browser runtime gates remain separate.

### Previous checkpoint — 2026-10-24 (L21k–t, ten chunks)

Ten serial Luna chunks extend App carousel/title scene output, with independent
Sol review and verification after each: **L21k** native current-owner atlas/image
source snapshots; **L21l** shared owned bitmap-glyph command capture; **L21m**
injected-time menu motion policy; **L21n** card layout/style planning; **L21o**
typed card command/hit geometry output; **L21p** typed title scene; **L21q** typed
arrows with closed Previous/Next hit tags; **L21r** bound selection scenes;
**L21s** external mixed-scene/ownership/resource tests; **L21t** actual dummy-SDL
retained carousel/title replay. Sol strengthened the fake-host ownership/current-
generation assertions and required native fresh-owner replay for all three
normal/suspend/NES cases, not just the normal case.

Native code still performs candidate iteration, game-ID lookup, firmware/ROM
checks before x-culling, normalized save/lock projection, current resource lookup
and hit-action binding. Shared output retains only plain geometry/style data and
generation-bound IDs; missing resources skip their draws without inventing a
fallback or deleting hit geometry. Scenes never rebind stale IDs. All actual
launch/save/delete eligibility, stage/target ownership, translation and native
resource destruction remain unchanged. Menu motion receives elapsed seconds,
not clocks or emulator/audio owners. Command building allocates and is not a
new whole-frame memory/responsiveness guarantee.

Verification: **178 default / 177 portable / 287 native unit tests and 99
integration tests each** pass; formatting and warnings-denied portable/native
Clippy pass. All three ignored native SDL resource tests pass when explicitly
run with dummy video/software rendering. The new synthetic scene test exercises
real atlas/cover/font capture and copies, warm replay, old-ID rejection after
assets/canvas destruction and identical fresh-owner pixels for normal, suspend
and NES scenes. Native font/driver error injection, arbitrary context-loss
recovery and physical GPU performance are not implied.

Every production chunk kept **18 representative SNES/NES/Famicom snapshots**
pixel-identical. Final UI/language tests passed for **132 snapshots** (50 SNES,
48 NES, 34 Famicom), matching freshly built pre-L21k baselines outside the
elapsed-time launcher busy strip. Real-core suspend/save/lock/overwrite/delete/
replay/restart/navigation checks pass for all three systems with disposable save
data. Release SNES/NES dummy-audio checks pass at 50/60/120 Hz presentation with
no queue underruns. This is release/headless buffering evidence, not physical
audio/controller/display certification; earlier debug timing limitations remain
recorded below. Local-image importer fault matrices were not rerun for this
rendering-only batch. Artifacts/logs remain untracked under `.build/l21k*`.

**Next: L21u**, bind HUD/thumbnail command groups and project/compose the remaining
menu-frame display data, preserving current native hit bindings and pass order.
**L21 is still partial:** complete App scenes/target-pass gates, L22, S07, L25,
Checkpoint C and Apple/browser runtime gates remain separate.

### Previous checkpoint — 2026-10-24 (L21a–j, ten chunks)

Ten serial Luna chunks were independently reviewed and verified after each:
**L21a** checked owner-generation/slot texture bindings wired to native loading;
**L21b** typed texture-quad execution; **L21c** atlas sprite naming, rounded
placement and flipping; **L21d** fitted-image commands; **L21e** shared bitmap-text
centering/scaling; **L21f** selection nine-slice planning; **L21g** wallpaper fill/
texture commands; **L21h** HUD prompt placement; **L21i** thumbnail placement;
**L21j** stale-resource validation and opt-in actual dummy-SDL copy/recreation.
Sol strengthened text extent/overhang coverage and retained NES-only palette
resolution. Every production slice kept the 18 representative SNES/NES/Famicom
snapshots pixel-identical to the freshly built pre-L21 baseline.

`frontend::menu_scene` now describes leaf texture/fill commands using plain data,
not filesystem keys or SDL handles. Native UiAssets registers only successful
loads under a fresh checked non-wrapping generation; stale generations/unknown
slots are rejected before touching textures. Every resolved copy sets tint,
alpha and filtering, retaining the old ignored SDL-copy error behavior. Native
texture destruction, atlas/font/game lookup, stage targets, translation, clocks,
ROM/firmware/save guards and hit/action binding remain in place. Selection/HUD/
thumbnail records are layout plans, not retained texture capabilities. Empty
thumbnail input is defined as empty; native catalog nonempty validation remains.

Verification: **178 default / 177 portable / 287 native unit tests and 74
integration tests each** pass. Formatting and warnings-denied portable/native
Clippy pass. Both opt-in SDL resource tests pass explicitly. The new synthetic
red/green test verifies real nearest/linear, flip/crop, modulation reset, fill,
unknown/stale ID rejection and fresh-owner replay after destroying/rebuilding
assets and canvas. Generation exhaustion/uniqueness tests do not mutate the
global counter. This is dummy/software-renderer evidence, not generic GPU-loss
recovery or native GPU failure injection.

The full UI/language regression passed for **132 snapshots** (50 SNES, 48 NES,
34 Famicom), all pixel-identical to fresh baselines outside the existing elapsed-
time launcher busy strip. Real-core suspend/save/lock/overwrite/delete/replay/
restart/navigation checks pass on SNES/NES/Famicom using disposable save data.
Release SNES/NES dummy-audio checks pass at 50/60/120 Hz presentation with no queue
underruns. A prior debug SNES audio run failed the rate threshold at its 120 Hz
phase (53.04 Hz, zero underruns); it is recorded as a failed timing run, not
silently counted as passed. The menu renderer does not run in that audio loop;
release verification passed separately without weakening thresholds. Logs and
generated media are untracked under `.build/l21-*`. These checks do not certify
physical audio/display/controller performance or new-platform runtime.

**L21 remains partial. Next:** project carousel/title display data and compose
their typed draw/hit output in small slices, then extend App scene composition
and its target-pass/resource-recreation gates. The native stage/core/suspend
ownership boundaries, L22, S07, L25 and Checkpoint C remain separate.

### Previous checkpoint — 2026-10-24 (L20t–x, five chunks)

Five serial Luna chunks complete the **L20 desktop launcher scene extraction**:
**L20t** native projection into owned, resource-free presentation data (current
six-row page and selected details only); **L20u** shared composition around the
explicit status-measurement barrier and native execution; **L20v** external
fake-host whole-scene/error-order tests; **L20w** opt-in actual dummy-SDL View/
canvas recreation; **L20x** native projection/interaction parity tests. Sol
reviewed and independently verified every chunk, strengthened readback dimension
checks and tested half-open hit edges through the native coordinate wrapper.

The full composed scene owns all commands/status strings and can outlive native
entries and projection inputs. Built-in font/flag roles resolve against each
current View. Native code retains classification, path display conversion,
primary eligibility, font metrics/rasterization and actual resource ownership.
Status drawing still occurs before the next measurement and stops on propagated
text errors; fake-host failure injection is not native font/GPU failure testing.
Scene building/cloning has allocation/input-dependent cost, not a new latency or
memory bound. Dynamic App image/video resource lifetimes are not solved by this
fixed-role launcher contract.

Verification: **176 default / 175 portable / 282 native unit tests and 57
integration tests each** pass. Formatting and warnings-denied portable/native
Clippy pass. The additional ignored SDL recreation test was explicitly run with
dummy video and software rendering and passes: three retained scenes (normal,
empty, busy/error, all four flag roles and both font roles) produce identical
pixels with warm resources and after destroying/rebuilding View and canvas.
Six projection tests cover page/ownership, classification, malformed selections,
focus/busy style separation and native/2×/wide/portrait hit mapping. All **14
launcher previews** match a fresh pre-L20t baseline outside the elapsed-time busy
strip. Opt-in local-image and broader App/audio/suspend regressions were not
rerun for these launcher changes; physical devices/new-host runtime remain open.

**Next: L21**, one App carousel/HUD view at a time after Sol scopes its dynamic
resource/target-pass contract. L22, S07, L25 and the full Checkpoint C regression/
platform gates remain separate. L20's desktop extraction is not Checkpoint C or
mobile/browser support.

### Previous checkpoint — 2026-10-24 (L20o–s, five chunks)

Five more serial Luna chunks were independently reviewed, tested and preview-
compared before each next dispatch: **L20o** semantic built-in flag commands and
their SDL adapter; **L20p** complete library-row command groups from plain display
data; **L20q** detail-card header/controller/badge group; **L20r** detail region/
source/note group; **L20s** incremental shared status wrapping with native width
measurement. Every returned text command owns its string. Native Entry/Profile
classification, path-to-display conversion, selection and action eligibility
remain in the adapter. Built-in flag roles resolve against each current View's
assets, not old texture handles; SVG loading/fallback/error behavior is unchanged.

StatusText exposes exact candidate strings and consumes one host-measured width
per word. It preserves Unicode whitespace, leading-space candidates, the strict
550px threshold, overlong first words, draw-before-next-measure order and the
legacy empty terminal request at y=720 after the three-line cutoff. Shared code
has no font/measurement callbacks or I/O. Host word processing still has legacy
input-dependent cost, not a new bounded-update guarantee. Eight pure tests cover
fixtures, original-loop traces, scalar edge cases and simulated draw failure;
native font/texture failure injection remains unimplemented.

Verification: **176 default / 175 portable / 276 native unit tests and 50
integration tests each** pass. Formatting and warnings-denied portable/native
Clippy pass. All **14 launcher previews** match a fresh pre-L20o baseline outside
the elapsed-time busy strip after each chunk and at final verification. Final
native verification exposed a pre-existing App-import test race: the readiness
barrier preceded begin_commit, so a legitimate cancellation could win. Sol moved
that test-only barrier after begin_commit; the exact test passed **20/20** runs
and the full native suite passed. Production cancellation behavior was unchanged.
Opt-in local-image and broader App/audio/suspend regressions were not rerun for
these launcher presentation changes; no new-platform runtime claim is made.

**L20 remains partial. Next:** compose the verified command groups behind a
host-neutral launcher presentation input, preserving the status measurement/draw
handshake and native display-data projection. Verify View/resource recreation,
host interaction parity and full launcher composition before marking L20 done.
Font metrics/rasterization and dynamic image/video resource-key lifetimes remain
host concerns; later L21/L22, S07 and L25 are still separate tasks.

### Previous checkpoint — 2026-10-24 (L20j–n, five chunks)

Five serial Luna chunks were independently reviewed, tested and screenshot-
compared after each: **L20j** empty-library geometry/text requests; **L20k**
primary-button geometry/text requests; **L20l** footer hints plus the unchanged
four-value cancellation presentation enum moved to shared launcher activity;
**L20m** page-indicator requests; **L20n** exact flag-region marker/label metadata.
Native compatibility re-exports preserve existing callers. Button eligibility,
worker state-to-view mapping and cancellation ownership are unchanged; pagination
retains legacy unclamped selected-index/count arithmetic. Flag SVG loading,
textures, fallback geometry and rendering/error behavior stay native.

Verification: **176 default / 175 portable / 276 native unit tests and 35
integration tests each** pass. Formatting and warnings-denied portable/native
Clippy pass. All **14 launcher previews** match a fresh pre-L20j baseline outside
the elapsed-time busy strip after every chunk and at final verification. Ten
text-scene tests now cover exact payload/order/style, owned temporary strings,
all footer state combinations, page boundaries and maximum indices/counts. A
pure region test covers exact aliases/labels and rejects case/whitespace/unknown
markers; both native flag tests (including sourced SVG colors) remain intact.
App import and launcher tests pass with the shared cancellation enum. Opt-in
local-image and broader App/audio/suspend regressions were not rerun for these
launcher presentation changes. No new-platform compilation/runtime is claimed.

**L20 remains partial. Next:** Sol resolves the flag-resource contract and status
measurement seam before dispatching row/detail/status scene slices. Flag-region
metadata is not a resource key or flag draw command. Native font fitting/glyph
layout, flag resources and full ordered scene composition remain open, alongside
later L21/L22, S07 and L25.

### Previous checkpoint — 2026-10-24 (L20g–i)

Sol approved the incremental owned text-request contract in
[the update contract](frontend-update-contract.md). Three serial Luna chunks now
emit mixed geometry/text requests for branding, the sidebar, and library
header/filter tabs. Each chunk was independently reviewed, tested and compared
against freshly built launcher previews before the next dispatch. Sol retained
exhaustive font-role mapping and the sidebar's shared button-coordinate source.

Requests own strings, plain geometry/color/fitting data and a typed embedded
font role, not SDL glyphs/textures or font paths. The native executor preserves
command order and calls the unchanged fitting/kerning/ascent/rasterization
routine, propagating its errors. These are presentation requests, not shared
glyph layout or a dynamic generation-checked resource registry. Host hit tests
still supply styling; scene output does not grant action eligibility.

Verification: **176 default / 175 portable / 276 native unit tests and 28
integration tests each** pass. Formatting and warnings-denied portable/native
Clippy pass. All **14 launcher previews** match the fresh pre-L20g baseline
outside the elapsed-time busy strip. Four pure text-scene tests cover exact
payloads/order/font roles/style/layout, independent string ownership, sidebar
state combinations, every filter/hot/busy combination and source counts including
zero and `usize::MAX`. Broader App/audio/suspend and opt-in image tests were not
rerun for this launcher-only change; no new-platform runtime claim is made.

**L20 remains partial. Next:** empty-library text, then primary/footer text in
small chunks. Status wrapping needs its own measurement contract; row/detail
text interleaves flags, whose resource contract is not yet approved. Font
measurement, flag drawing/resources and full ordered scene composition remain
native. L21/L22, S07 and L25 remain later work.

### Previous checkpoint — 2026-10-24 (L20d–f)

Three more small, serial Luna chunks extract the remaining top-level launcher
chrome: brand/library sidebar accents and filter tabs; library row borders,
insets and flag backplates plus the empty-library panel; detail/unknown-controller
panels, separator, primary-button background and import-progress geometry.
Sol reviewed and independently tested each chunk before dispatching the next.
All production calls retain their original order relative to text and flags.
Shared functions consume plain indices/booleans/elapsed seconds, not native
entries, paths, workers or textures. Rendering does not grant action eligibility;
the existing cancellation, picker, import and activation owners are unchanged.

Verification: **176 default / 175 portable / 276 native unit tests and 24
integration tests each** pass. Formatting and warnings-denied portable/native
Clippy pass. All **14 launcher previews** match the freshly built pre-L20d
baseline outside the elapsed-time busy-animation strip. Pure tests separately
characterize that strip's track/thumb ordering and exact geometry at elapsed
0, 1, -1 and 42 seconds, plus selected/hover/busy priorities and invalid sidebar/
tab indices. Native scanline rasterization is unchanged. Broader App/audio/
suspend regressions and opt-in local-image tests were not rerun for this
launcher-only extraction; no new-platform runtime evidence is claimed.

**L20 remains partial. Next:** Sol approves the launcher text/flag resource and
measurement contract, then dispatches one small production text/flag scene slice
at a time. Text fitting/wrapping, flag textures (including fallback geometry),
entry-to-display mapping and full ordered scene composition remain native.
L21/L22, S07 and L25 remain later work.

### Previous checkpoint — 2026-10-24 (L20b/c)

L20b/L20c continue with two small, serial Luna chunks, independently reviewed and
verified after each. The launcher background emits ordered plain fill commands
(760 wash scanlines, translucent sidebar and separator); the SDL adapter handles
both fill and rounded-rectangle commands. Shared Rust also owns stage/page sizes,
button/filter-tab/row rectangles and letterbox coordinate mapping. Native drawing
and click handling use compatibility adapters over those same values. Existing
minimum-scale and out-of-stage coordinate behavior is preserved, not clamped.

Verification: **176 default / 175 portable / 276 native unit tests and 21
integration tests each** pass. Formatting and warnings-denied portable/native
Clippy pass. All **14 launcher previews** match a fresh pre-L20b baseline;
only the elapsed-time busy-animation strip (548×3 pixels) is excluded in busy
previews. Pure tests cover scanline positions/alpha and representative colors,
overlay order, fixed layout, scaling, letterbox margins and zero/tiny windows.
Broader App/audio/suspend regressions and opt-in local-image tests were not rerun
for these launcher-only changes. No new-platform compilation/runtime claim is
made; text/flags/resource lifetimes and remaining chrome are still native.

**L20 remains partial. Next:** remaining launcher chrome geometry in small
slices, then an approved text/flag resource contract before moving those
commands. L21/L22, S07 and L25 remain later work.

### Previous checkpoint — 2026-10-23

L20a starts launcher scene output with two small, serial Luna chunks: the
NES/Famicom/SNES controller illustrations now emit owned host-neutral rounded
rectangle commands, followed by external-host characterization tests of every
command's order, geometry, color and radius, translation and independent ownership.
Sol reviewed each chunk, repaired a test-only Clippy warning and strengthened the
ownership assertion. SDL still owns scanline rasterization; classification,
fonts, flags, hit testing and the rest of the launcher view remain native.

Verification: **176 default / 175 portable / 276 native unit tests and 19
integration tests each** pass; formatting and warnings-denied native Clippy pass.
All three NES/Famicom/SNES launcher previews are pixel-identical to the freshly
built pre-extraction baseline. Opt-in image tests and broader App/audio/suspend
regressions were not rerun for this illustration-only change. No new-platform
runtime evidence or resource-ID lifecycle is implied by resource-free geometry.
**L20 remains partial. Next:** extract launcher background/chrome geometry in a
separate chunk, then define the text/flag resource contract before moving those
commands. L21/L22, S07 and L25 remain later work.

### Previous checkpoint — 2026-10-22

Batch 21 adds the native App host-frame seam: bounded SDL dispatch and dialog
intake, closing-request cleanup before completion processing, explicit native
update work, separate stage creation/drawing, and an overflow-safe shared frame
limit. The desktop driver visibly orders update, draw, optional screenshot,
present, frame counting, pacing and finalization. Finite previews do not close
reusable request owners; native capture/settings/core-unload order is retained.
Final verification is now **175 default / 174 portable / 275 native unit tests
and 17 integration tests each**, formatting and warnings-denied native Clippy.
All 31 UI screenshots are pixel-identical to the pre-extraction baseline;
SNES/NES/Famicom suspend/navigation/replay and SNES/NES dummy-audio regressions
pass. Audio checks cover queue buffering, not physical audio or speed guarantees.
No additional platform runtime evidence was added. Next is **L20**, view-by-view
scene output; S07 core/audio ownership and L25 lifecycle gates remain separate.

The previous checkpoint below covers batches 14–20.

- Batches 14–20 completed 21 Luna-sized chunks, with combined Sol review after
  every third chunk and immediate repair of test/contract failures. Profile JSON
  reads are stream-bounded; migration directory/entry/metadata failures are
  injectable and covered by real-image tests. Legacy old-manifest skip behavior
  is explicitly retained.
- Launcher startup, native session/service ownership, input translation, pure
  navigation/activity/frame planning, native effects, and drawing/pacing are
  separate seams. Frame plans are tested by a non-SDL host. Raw input and host
  notification drains retain FIFO tails at budgets of 128 and 64. Accepted picker
  sources dispatch directly, avoiding an idle gap behind queued progress.
- App keyboard/gamepad menu input uses shared logical keys. Main menu, general
  panel, toolbar and suspend-key decisions emit native intents from shared
  policies; core/storage/save/delete guards remain in the native executor.
  Malformed confirmation focus cannot emit destructive confirmation intents.
- Final verification: **173 default / 172 portable / 268 native unit tests**, plus
  **17 integration tests** in each configuration. Six portable/eight native unit
  tests and one source-parity integration test remain opt-in local-image/helper
  tests. Formatting, warnings-denied native Clippy, 31 UI snapshots, SNES/NES/
  Famicom suspend/navigation/replay checks and three native CTest targets pass.
  SNES/NES dummy SDL audio tests pass at 50/60/120 Hz presentation with no queue
  underruns; this is not physical speaker/VSync/performance verification.
- Review repairs included deterministic producer-readiness barriers, removal of
  duplicated controller policy, invalid-focus guards, direct accepted-source
  handoff, and the real-image worker's stale post-cancellation progress assertion.
- Next: **L19c**, the App host tick/render/pacing seam, followed by view-by-view
  renderer-independent scene output (L20–L22) and S07 emulator/audio ownership.
  The shared policies/protocol are reusable; synchronous native filesystem effects
  and the remaining App driver are not a universal nonblocking application loop.
  No new Apple/browser app/runtime or physical-device support is claimed.

The entries below retain the earlier incremental evidence and test counts.

- Two Luna source audits and a separate adversarial plan review completed. Sol
  revised the plan to split oversized tasks and reviewed the first extraction.
- **L01–L08 implemented and verified:** launcher input/filter/selection,
  main-menu stick/repeat, language-grid navigation and audio/frame timing
  policies, suspend-picker decisions and gameplay-mask assembly now run without
  SDL or filesystem/profile types. Save/overwrite/delete/lock protection remains
  in the existing suspend operations, not in the navigation reducer.
  Native gamepad ownership, SDL scancode translation, `Instant` conversion,
  presentation pacing and sleeping remain in thin `ui/` adapters.
- Sol compared the extracted policy and its original seven tests against the
  baseline: identical apart from comments, visibility and removal of the adapter.
  An external integration test exercises the public API as an alternative host.
- Verification: default **57 unit + 7 integration**, no-default-features
  **56 unit + 7 integration**, native-app **80 unit + 7 integration** tests pass;
  formatting and warnings-denied Clippy pass for pure/native configurations.
  Bounded dummy-driver window/launcher-preview runs, three native CTest targets,
  image-backed UI snapshots, SNES/NES/Famicom suspend/controller checks and audio pacing at
  50/60/120 Hz pass. This is not a new platform runtime or physical-controller
  certification.
- Independent suspend tests compare all 800 valid state/input combinations with
  the original key branch, distinguish activation/shortcut/dismissal effects,
  and protect invalid-focus Close from becoming Delete. Gameplay integration
  tests cover all 4096 keyboard masks with pad/stick/block combinations. Native
  key/pad mapping arrays and all other keyboard branches match the baseline.
- Apple core smoke executables now compile/link for **both** iOS and tvOS,
  **simulator and device** targets, using reviewed build-tree compatibility
  generation. Mach-O platform checks confirm each target; upstream is unchanged.
  Synthetic generator success/rejection tests run in the native build. Equivalent
  CI compile/link jobs are configured; their remote run is pending.
- Tooling inventory: only the macOS Rust target is installed; Emscripten and Apple
  simulator runtimes are not installed. No Apple app/core has run. Details are in
  [feasibility evidence](frontend-feasibility.md). S02 is **partial**; S01/S03a–c
  remain **unresolved**, not implicitly successful.
- **Checkpoint A passed on desktop:** pure/native tests, dependency-direction
  inspection and behavior regressions complete. **S04a/S04b and L10** now define
  and implement correlated image-picker results for the desktop launcher; see
  [the contract](frontend-effects.md). Stale/duplicate results cannot clear newer
  requests, and queued legacy sources cannot overtake an open picker. **L11**
  now correlates prepared-folder selection through the same tracker, checking
  both ID and picker kind; import-worker messages remain uncorrelated.
  **L09** is also complete: startup/profile switching share preference decoding,
  schema-1 snapshots preserve unknown/lock/history/ROM/display keys, and the host
  save helper commits the updated document in memory only after a successful
  atomic JSON write. Non-object preference roots now fail validation explicitly;
  no schema bump or crash-durability guarantee is implied. **L12a/L12b** now
  correlate App ROM/firmware selection with captured profile/game identity and
  reject stale/wrong-kind results before settings changes or auto-start. Native
  validation and firmware readiness checks remain in place. The three-chunk
  batch passed combined Sol review (including repair of current-error coverage):
  57 default / 56 portable / 86 native unit tests, plus 7 integration tests in
  each configuration; warnings-denied native Clippy, formatting, headless UI
  snapshots and the real-core suspend/navigation regression pass. Real system
  dialogs were not exercised by those tests.
  **L13a/L13b.1** also passed combined review: catalog models/decoding and language
  metadata/table/fallback policy are shared; native JSON reads are bounded to
  32 MiB even when a file grows after metadata inspection. Defaults and native
  file/path checks remain intact. Verified 70 default / 69 portable / 100 native
  unit tests plus 7 integration tests each; formatting, warnings-denied native
  Clippy, UI snapshots and real-core suspend/navigation regressions pass.
  L13b's text-policy seam is complete; native file/font/texture ownership is
  deliberately unchanged. **L14a/L14b** are complete: font metrics, fallback text
  placement and sprite/theme metadata now have CPU-only shared implementations.
  Native textures and SDL drawing stay put; permissive metadata behavior and the
  distinct initial/runtime color rules are preserved, not newly hardened.
  Combined review verified 79 default / 78 portable / 109 native unit tests,
  plus 7 integration tests each; formatting, warnings-denied native Clippy and
  suspend/navigation checks pass. All **25** headless UI snapshots are pixel-exact
  against the pre-font-extraction baseline. **L15** now provides a bounded
  `Read + Seek` image input and keeps the path wrapper. Gzip and SquashFS obey
  the declared extent without requiring a whole-image copy or `Send`. Combined
  review verified 88 default / 87 portable / 118 native unit tests plus 7
  integration tests each, formatting and warnings-denied native Clippy. An
  opt-in integration test also passed on local valid SquashFS and gzip/ustar
  images: selected files were identical and source SHA-256 was unchanged.
  Host-supplied snapshot stability and S03 browser memory/storage/responsiveness
  remain unresolved; the input seam is not a browser importer.
  **L16 is partial:** profile/skin write primitives, exact-owned-stage cleanup,
  and final rename/publication decisions now use narrow native adapters. Tests
  inject short/zero/disk-full writes, cleanup failure, pre-rename failure,
  concurrent valid/invalid destinations and errors observed after rename; fresh
  disk reads and real profile validation check the outcomes. Combined review
  verified 103 default / 102 portable / 133 native unit tests plus 7 integration
  tests each (1 opt-in local-image test ignored); formatting and warnings-denied
  native Clippy pass. The full six-image SNES importer audit also passes: nine
  languages per image, cache reuse, v4–v6 data migration and unchanged sources.
  Follow-up L16 batches now thread write/copy/publication injection through the
  actual importer and skin preparation. Eight local-image error scenarios pass,
  including directory failures and partial manifest writes. Exclusive staging
  creation has real collision/exhaustion/permission tests. Two actual child
  process exits before/after rename verify restart visibility and preserved old
  saves/settings; unpublished orphan stages remain ignored, not auto-deleted.
  Latest combined review: 109 default / 108 portable / 139 native unit tests,
  plus 7 integration tests each; formatting and warnings-denied Clippy pass.
  These are function-error/process-exit checks, not fsync/power-loss guarantees.
  Source/migration read-fault coverage remains open. **L17a–c** now provide a
  pure one-shot lifecycle and a native controlled-import API. Cancellation and
  the commit boundary are serialized; requested cancellation is distinct from
  acknowledged abort and too-late cancellation. A reused handle cannot start a
  second executor or acknowledge the running executor's cancellation.
  Eight real-image cancellation scenarios pass, including migration/manifest
  boundaries and too-late cancellation during publication. Cancellation waits
  for phase boundaries; hashing, reads, skin decoding and conversions are not
  preempted mid-operation. Existing desktop callers retain the synchronous API;
  The following launcher integration now wires correlated worker progress,
  nonblocking result polling and cancellation controls (see below).
  Final combined review: 122 default / 121 portable / 152 native unit tests,
  plus 7 integration tests each; 4 local-image/helper unit tests and 1 parity
  integration test are ignored by default. Cancellation, eight-case filesystem
  errors and both subprocess-restart matrices were explicitly run and passed.
  Formatting, warnings-denied native Clippy, the six-image importer/migration
  audit, headless UI and real-core suspend/navigation regressions pass.
  **Launcher L17 integration** now has typed job IDs and private per-job result
  channels. The UI never joins import threads; completed work is distinct from
  final thread/TLS teardown. Closing requests cancellation and suppresses opening
  profiles. Escape/controller East and the existing primary button cancel busy
  imports; requested/finishing states stay busy until an eligible result.
  Same-frame closing and picker/source overtakes are guarded. Combined review
  verified 122 default / 121 portable / 171 native unit tests plus 7 integration
  tests each, formatting, warnings-denied Clippy, 28 UI snapshots (including the
  three cancellation states), and real-core suspend/navigation regressions.
  Tests use scripted input policies and rendered previews, not physical hardware.
  Follow-up batches also cover source hash/parser/rehash/selected-source failures
  and ordinary failure after an unacknowledged cancellation request. Real-image
  worker tests verify cancellation, restart of that worker, and shutdown.
  **App import integration** now shares the worker, correlates its image picker
  with ROM/firmware pickers, and performs image imports off the UI thread.
  Captured profile/game and sticky notice ownership gate automatic switching;
  dismissed or stale results refresh the library without interrupting gameplay.
  App progress drains at most 64 tagged messages per frame. Owned notices expose
  Cancel import (Escape/controller East/mouse) and Continue browsing; requested/
  finishing states do not falsely report cancellation. Preview states cannot
  grant live action eligibility. Final combined review: **124 default / 123
  portable / 195 native unit tests + 7 integration tests each**, formatting and
  warnings-denied native Clippy pass. Five portable/seven native local-image or
  helper unit tests and one parity integration test are opt-in. The real-image
  App notice lifecycle (switch/background/cancel), 31 UI snapshots, suspend/
  navigation checks and six-image importer/cache/migration audit passed.
  Physical input/dialog testing, mobile/browser runtime and remaining lifecycle/
  storage gates remain distinct; Checkpoint B is not a new-platform support claim.
  Checkpoint B
  remains open. The next Sol task for
  S02 is runtime verification once simulator runtime/device setup is approved.

## How to run a task

Use `openai/gpt-6-luna` for L/P tasks against the contracts in the architecture
document. Sol owns S tasks, design approval and the final integration review.
If a slice needs a new ownership/ABI/persistence design, return a question and
evidence instead of implementing a speculative interface.

Copy this brief into each agent handoff:

```text
Task ID and prerequisites:
Read: relevant source plus docs/frontend-architecture.md and this queue.
Own: explicit source/test files; other agents must not edit them concurrently.
Implement: exactly this seam, wired into the current desktop app.
Preserve: listed behavior, signatures at compatibility boundary, content policy.
Do not: add backends/dependencies, alter core ABI, weaken checks, commit/push,
        modify upstream SDL/ares, or access/delete user saves/images.
Acceptance: named pure tests plus fmt/Clippy/native tests as applicable.
Return: files changed, test output/counts, tests not run, risks, next task.
Stop: contract ambiguity, scope spill, failed parity test or missing toolchain.
```

Keep execution serial whenever `ui.rs`, `launcher.rs`, `lib.rs`, Cargo or shared
interfaces overlap. Parallelize read-only audits, or adapters after their
contracts and file ownership are fixed. A suggested size is one policy/module
and at most two production callers; split larger tasks before dispatch. No
fixed token/line estimate guarantees success—test each completed slice.

Read project build-script sources for usage; do not assume `--help` is harmless.
The current Python build scripts can run their build/package workflow even with
that argument. Build only the authorized target, and leave generated artifacts
untracked; do not undo unrelated user work.

### Three-chunk Luna batches

Sol screens each proposed chunk for a clear existing seam, bounded ownership,
an agreed contract and meaningful tests **before dispatch**. Split or keep with
Sol any task that needs an unresolved ABI, ownership or persistence decision.
Luna then implements three compatible chunks sequentially, preserving its prior
work and running focused tests after every chunk. Failed tests or a contract
ambiguity are immediate escalation points, not permission to accumulate three
unverified changes.

After chunk three, Sol reviews the combined diff and runs integration/regression
verification once for the batch. Do not confuse this cadence with omitting
Luna's per-chunk tests. Reserve enough usage for that final review and possible
repairs; check the active account quota before/after delegation and stop with a
margin above the user's current floor. No numeric account quota is a permanent
repository setting.

Completed batch: L12a.1 native request-owner tests, L12a.2 ROM picker lifecycle
wiring, and L12b firmware integration. The first attempted ROM slice was too
large for Luna to finish with its mandatory native checks, so it was split
rather than counted as complete. Completed second batch: L13a.1 catalog
model/value decoding, L13a.2 bounded catalog byte decoding, L13b.1 language
metadata/table decoding. Completed third batch: L14a.1 font metrics, L14a.2 text placement,
L14b sprite/theme metadata, with native resource ownership unchanged.
Completed fourth batch: L15a generic bounded source, L15b reader entry point and
bounded gzip stream, L15c hostile-input parity. Sol strengthened error-reason
assertions and independently verified valid local-image parity; the local-image
test is ignored by default and requires `CLOVER_SOURCE_PARITY_IMAGE`.
Completed fifth batch: L16a.1 native profile/skin writes, L16a.2 owned-stage
cleanup, L16b.1 publication decision seam. Sol strengthened actual file-creation
and disk-full assertions. These are partial L16 seams, not a complete fault or
crash harness. Completed sixth batch: injectable skin writes, whole-import
write/copy backend threading, six-case local-image fault matrix. Completed
seventh batch: exclusive-stage collision tests, expanded eight-case directory/
write/copy/publication matrix, actual before/after-publication process restart.
Local-image tests are opt-in; neither orphan cleanup nor power-loss durability
was added. Completed eighth batch: pure cancellation lifecycle, native controlled-import
integration, cancellation/race verification. Desktop cancel-button/job-message
correlation was a later slice, not implied by a controlled-import API. Sol caught
and corrected running-handle reuse semantics before acceptance. Current quota
pause is a session policy; preserve a margin above the user's active floor.
Completed ninth batch: launcher worker correlation, nonblocking retirement and
shutdown/source-queue traces. Sol removed update-path joins after reviewing the
limits of `JoinHandle::is_finished`. Completed tenth batch: keyboard/controller
cancel requests, visible primary-button cancel states, scripted-input and UI
snapshot coverage. Completed eleventh batch: source operation injection,
six-case source-failure matrix and actual worker/image cancellation. Completed
twelfth batch: shared native worker visibility, correlated App image picker,
context-safe asynchronous App import. Completed thirteenth batch: bounded App
progress notices, cancellation/browse controls, real-image notice lifecycle and
three App preview states. Sol tightened sticky notice release and separated
render-only preview presentation from live action ownership. Next: evaluate
Checkpoint B gaps, then S06 bounded update/scene contracts before L18/L19.
These share native integration files and therefore run serially, not in parallel.

## Phase 0 — evidence and early feasibility

| ID / owner | Deliverable | Acceptance / dependency |
|---|---|---|
| S00 / Sol | Record current test commands, desktop limitations and extraction rules. Review two Luna audits (UI seams and platform services). | Initial baseline at `08072c8`: `CLOVER_NATIVE_BUILD_DIR="$PWD/.build/native" cargo test --locked --manifest-path rust/Cargo.toml --all-targets --features native-app` passed 44 tests. Counts are an audit aid, not parity proof; relevant tests must still run after extraction. Window smoke must set `CLOVER_RUST_WINDOW_FRAMES=2` because its interactive default runs indefinitely. No claim that Apple/WASM library checks build an app. |
| S01 / Sol | Browser ares feasibility: tiny generated NES ROM through the C ABI, then generated SNES ROM; test coroutine switching and frame/audio delivery before choosing module/ABI strategy. | Runnable browser harness, finite nonzero audio/video, save/replay; document exact SDK/link flags, threading, memory, isolation and tested browsers. If NES succeeds but SNES fails, the gate is not passed. No user ROM needed. |
| S02 / Sol | Apple ares feasibility: separate iOS and tvOS simulator smoke executables, then device targets; establish session-owner/lifecycle contract. | Target-specific archives, generated ROMs, no accidental macOS linkage; record simulator vs device evidence and signing/toolchain prerequisites. |
| S03a / Sol | Prototype browser local-file access and persistent transaction/flush, with a tiny self-authored file. | Selection, denial, quota/eviction recovery and reload test on intended browser engines; specify cancellation and paired-file selection. No uploads. |
| S03b / Sol | Prototype iOS document access and sandbox persistence. | Simulator/device acquisition and restart/recovery; permission lifetime, interrupted access, backup/export and companion selection. |
| S03c / Sol | Prototype tvOS non-picker ingestion and evictable-storage recovery. | User can acquire an image and optional firmware using a controller/remote-accessible flow, then recover from cache loss. If a network transfer is proposed, obtain explicit approval and specify privacy/authentication; do not silently add uploads. |

S01–S03c may run independently of safe L01–L07 desktop extractions. Resolve them
**before** selecting browser/Apple service implementations or making platform
support promises. This environment initially has only the macOS Rust target;
emsdk, Apple application tooling/targets and device availability must be checked,
not assumed or silently installed.

## Phase 1 — pure behavior, no new backend

Each L task gets a pure test that runs without `native-app`; retain adapter-level
tests where they exercise SDL behavior. Keep old/private imports temporarily if
that limits changes to callers. Do not merge distinct launcher/game-menu repeat
policies just because their implementations look similar.

| ID | Exact slice and owned source area | Prerequisite / acceptance |
|---|---|---|
| L01 | Move only the pure policy from `ui/launcher/controller.rs` to `frontend/launcher_controller.rs`; expose host-neutral input/snapshot/action types unconditionally from `lib.rs`. Keep the SDL owner/adapter in `ui/launcher/controller.rs`. Update standalone test paths. | S00. All seven existing controller tests run in default and no-default-feature tests; unchanged action mapping, neutral suppression, hysteresis, repeat and focus behavior. Native test total must not drop. No SDL references or feature-gated SDL submodule in shared code; a second host can call the pure API. |
| L02 | Extract the main-menu stick hysteresis/mask policy from `ui/gamepad.rs`; define a backend-neutral directional value. Keep SDL `Scancode` conversion in that file. | L01. Extreme axes, engage/release boundaries, diagonal gameplay and vertical menu priority unchanged. Do not adopt launcher's different thresholds. |
| L03 | Extract main-menu repeat policy from `ui/gamepad.rs` using injected elapsed `Duration`, keeping an SDL-facing wrapper. | L02. Immediate edge, 350 ms initial/110 ms subsequent repeat, release/clear, stalled tick emits at most one repeat. No `Instant::now` in the policy. |
| L04 | Move only `toolbar::language_next` decision logic into a pure helper using directional input; retain scancode translation in toolbar. | L02. Preserve `language_grid_arrows_follow_columns` and `legacy_single_language_cannot_escape_grid`; add non-directional/no-op case. Do not move page actions. |
| L05 | Extract timing calculations from `ui/timing.rs` into elapsed-time policy; leave `FramePacer`, SDL VSync and sleeping in desktop code. | L01; Sol approves elapsed-time contract. Existing buffer-size/missing-audio/catch-up tests plus zero/large time steps. 50/60/120 Hz tests do not gain extra sleeps or change emulated speed. |
| L06 | Extract `LibraryFilter` labels/cycling/persistence conversion and visible-index selection from `launcher.rs`; pass entry IDs/system classification as data, not `Entry`, filesystem paths or `view` functions. | L01. Unknown filter defaults; stable selection identity across filter changes; empty lists; no stale activation. Keep actual source classification and disk I/O at caller. |
| L07 | Extract resume-picker slot/control selection from `controls.rs` into a pure reducer. Operations in `suspend.rs` stay put. | L02. Four slots/five controls wrap exactly as before; NES special case; pending capture/no capture; confirm/back. Saving/deleting/overwriting must still use existing guards. |
| L08 | Extract keyboard/gamepad-to-core-mask assembly from `App::button_mask` into a pure held-button policy. Map SDL enums at the adapter. | L02. Every SNES bit; NES A/B; opposite directions policy preserved; blocked/neutral controllers; keyboard/pad/stick composition. No remapping feature. |

**Checkpoint A / Sol:** inspect dependency direction, run pure/no-default and
native suites, and verify every moved policy is used by production. Reject
shared modules that still import SDL, ares, `ui::*`, platform dialogs or OS paths.
L01–L08 are complete; subsequent tasks must not run concurrently against
overlapping files without coordination.

## Phase 2 — asynchronous effects and data boundaries

Before these tasks, S04a/S04b establish the minimal request/completion contract.
Avoid a giant `Platform` trait which exposes raw SDL handles, `PathBuf` and
blocking calls to shared code. Expand the service boundary one real use at a time.

| ID / owner | Exact slice | Prerequisite / acceptance |
|---|---|---|
| S04a / Sol | Define request IDs and generation/eligibility semantics for the first real desktop image-picker caller. Add profile/session identities only for callers that need them. | Checkpoint A; do not wait on all platform spikes for safe desktop work. Fake-host tests: delayed/out-of-order/duplicate result, profile switch, cancel, shutdown, focus and controller transitions. State which focus transitions invalidate results versus only suppressing input. |
| S04b / Sol | Define that caller's capability and typed success/failure/cancellation outcomes, informed by whichever S03 host findings are available. Review the boundary again before each new host adapter. | S04a. Unsupported vs denied vs cancelled are distinct; fake host tests. Do not select an async runtime, expose raw paths as a universal contract, or force `Send` on ares. Later `S04` dependencies mean both S04a and S04b. |
| L09 / Luna | Extract preference JSON decoding/encoding from `App` and launcher; host still reads/writes existing files. | S04. Round-trip, unknown/missing values, per-profile isolation, write failure does not display a false success. No schema bump. |
| L10 / Luna | Introduce the desktop picker adapter for **image selection only** and route `launcher::Message::Image/PickerClosed/Error` through S04 completion IDs. | S04. Cancel, picker error, duplicate/stale result, shutdown; no import after invalidation and no filesystem scanning in shared model. Preserve desktop picker behavior. |
| L11 / Luna | Route **prepared-profile/folder selection** separately through capability-gated effects. | S04/L10 integration. Cancel, picker error, stale profile and shutdown tests. Preserve desktop behavior; a fake host lacking folder access can still select a filesystem image or cached profile. A tvOS host must supply S03c's proven ingestion flow. |
| L12a / Luna | Correlate the ROM picker with its captured profile/game/start context; reject stale completion before changing overrides or starting a game. | L10. Cancel/error, reused game IDs across profiles, changed-and-back context, duplicate completion and shutdown; native extension/regular-file checks unchanged. |
| L12b / Luna | Extend the same native request owner to firmware-folder selection; single-flight across ROM/firmware, with wrong-kind rejection. | L12a. All context/cancellation cases; missing/incomplete firmware still fails closed; imported IPL separate from user DSP firmware. Preserve existing desktop behavior. |
| L13a / Luna | Extract catalog data/model and bounded byte parsing from native path opening. | L09. Schema, malformed/oversized JSON, safe/duplicate IDs, defaults and field parity. Native file-size/symlink and ROM-path checks stay intact. No image import rewrite. |
| L13b / Luna | Separately extract language-table decoding and fallback decisions from path/texture loading. | L13a. Nine/eight-language metadata, malformed input, legacy fallback and locale selection parity; no font/layout/backend changes. Later L13 dependencies mean both tasks. |
| L14a / Luna | Extract CPU-only bitmap-font metrics/layout from `UiAssets`; use plain geometry and keep texture creation/upload in SDL. Own a new font-layout module and only the necessary caller edits. | L13; Sol approves geometry contract. Synthetic glyph/kerning/whitespace/advance and malformed/truncated-input fixtures; imported glyph placement and measurement equal. |
| L14b / Luna | Separately extract sprite/theme parsing into CPU metadata with plain rectangles/colors, not SDL `FRect`/`Color`. | L14a. Synthetic atlas/theme fixtures, invalid/truncated values and out-of-range bounds; clipping/measurement preserved, no SDL textures in shared output. Later `L14` dependencies mean both tasks. |
| L15 / Luna | Add a bounded seek/read input interface to `image_reader::Source`, retaining the path-backed wrapper and existing limits. | S03 import findings. Path vs in-memory fixture equivalence, truncation/path/duplicate/expansion/XZ tests. No loosening of source hashing or immutable-input assumptions. |
| L16a / Luna | Isolate staging-write operations in the existing filesystem implementation with failure injection. | S04. Short write, disk-full and cleanup failure tests; old published profile/data untouched. Keep hashes/migrations/source identity checks. |
| L16b / Luna | Isolate publication/rename operations with explicit commit point and restart-visible outcomes. | L16a. Fail before/after manifest write, target replacement/rename and cleanup; reconstruct disk state in a fresh reader. No partial valid profile or loss of old user data. Later `L16` means both tasks, not a browser backend. |
| L17 / Luna | Add cancellation/progress at existing import phase boundaries with an injected job token; desktop worker remains executor. | L15/L16. Cancel during hashing/read/conversion/pre-publish and race cancellation against commit. Distinguish requested cancellation from acknowledged abort and already-committed success. No publication after an acknowledged abort. Document granularity; phase checks do not prove browser responsiveness. |
| S05 / Sol | Choose cooperative import chunks vs a dedicated browser worker; resolve XZ build/memory, bounded queues and temporary storage using a real browser spike. | S01/S03/L15–L17. Large-image responsiveness and memory test on target, not only small in-memory unit fixtures. The importer limit can exceed browser budgets; do not assume copying a whole image is safe. |

**Checkpoint B / Sol:** use a fake host to drive picker/import/preference failures
and stale results. Run local-image source/hash/cache/migration audits with
disposable saves. Verify no cancellation path deletes user data and no new
service interface turns a completed memory write into claimed durable storage.

## Phase 3 — frame/update and rendering boundary

S06's incremental [update/ownership contract](frontend-update-contract.md) now
approves the next desktop extraction. Batch 14 closes the identified profile
stream-bound and migration traversal fault gaps: 133 default / 132 portable /
204 native unit tests plus 7 integration tests each pass, alongside formatting,
warnings-denied Clippy, real-image migration read faults and desktop regressions.
A pre-existing progress-test synchronization race was fixed (producer readiness
is signalled after messages are queued), then the test passed 20 repeated runs.
The desktop service checkpoint permits mechanical L18a work; retained migration,
orphan, durability and unverified-host limitations remain explicit in S06.

Next batch: native startup loading, native session state, explicit shutdown
ownership. These are L18a preparations, not yet a shared/bounded update loop.

Batch 15 completed native startup/state/service ownership. Batch 16 completed
data-only keyboard/controller/pointer navigation reduction, with opaque library
keys and host-side hit testing/effect dispatch. Input drains stop at 128 raw SDL
events and notifications at 64 messages per frame, leaving the FIFO tail intact.
Sol removed duplicated controller policy, added invalid-focus activation guards,
and verified 141 default / 140 portable / 226 native unit tests plus 7 integration
tests each, formatting, warnings-denied Clippy, UI and suspend regressions.
**L18b remains partial:** navigation is shared, but service activity/completion
state and native effect execution still sit in the outer loop. Next batch:
isolate native effects, reduce service activity as data, separate drawing/pacing.

| ID / owner | Exact slice | Prerequisite / acceptance |
|---|---|---|
| S06 / Sol | Define one bounded update/frame contract and owned scene/resource lifetime contract; select command emission vs a narrow renderer interface based on current SDL operations. | Checkpoints A/B. Specify target textures, clip/blend/flip/filter/text operations and video-buffer ownership. No unimplemented generic graphics framework. |
| L18a / Luna | Extract launcher state initialization and shutdown from `select`; keep event/render loop unchanged. | S06. Existing last-profile/filter/busy/error behavior and shutdown paths; no duplicate resource owners or workers left waiting. |
| L18b / Luna | Extract launcher event/completion reduction into one bounded update; desktop loop calls it. | L18a/S04. Deterministic busy/empty/error/filter/focus traces, stale completion tests; update never sleeps/waits on dialogs/workers. |
| L18c / Luna | Separate launcher render invocation and desktop pacing from update. | L18b. Scene snapshots and letterboxed interactions unchanged. Later `L18` means all three. |
| L19a / Luna | Translate main `App::run` host input into agreed logical events, retaining the current desktop loop. | L18. Keyboard/pad/toolbar/language/suspend event trace parity; no core/render changes. |
| L19b / Luna | Extract menu/panel event reduction from main loop; keep emulator stepping/audio at existing host caller until S07. | L19a. Repeated fake-host menu ticks need no SDL pump, core or filesystem; lock/overwrite/delete guards unchanged. |
| L19c / Luna | Separate main menu rendering and desktop pacing invocation from update; provide host scheduling seam for later S07 integration. | L19b. Snapshot/audio timing parity and bounded tick return. Later `L19` means all three. Do not move rendering/core/audio together to make one patch compile. |
| L20 / Luna | **Implemented on desktop:** owned launcher presentation/scene output and existing SDL adapter; status measurement remains an explicit host handshake. | L14/L18/S06. Fixed built-in font/flag roles, actual dummy-SDL View/canvas recreation, letterboxed hit tests and 14 previews verified in L20t–x. No new GPU backend or new-platform support claim. |
| L21 / Luna → Sol | **Partial (L21a–ax):** combined base/toolbar scenes, tooltips, notices and generic modal-panel scenes are wired into SDL. Distinct toolbar/suspend payloads and App target-pass composition remain. | L14/L19/L20. 132 UI snapshots, pure/native tests and nine dummy-SDL resource tests pass; stage target/core/suspend ownership and later target-pass gates remain native. Keep gameplay screen separate. |
| L22 / Luna | Extract **modal/toolbar/suspend** scene output in separate patches per panel family. | L07/L21. Dimming, clipping, scroll, lock indicators and screenshots remain correct; render output does not trigger save/load/delete. |
| S07 / Sol | Introduce emulator owner and audio-output contract, preserving current synchronous owner-thread baseline; choose any worker design only after feasibility evidence. | S01/S02/S06. Session created/used/destroyed on same owner; borrowed buffers never escape; queue bounds/backpressure; load/state/unload failures and SRAM persistence handled. No unsafe `Send`. |
| L23 / Luna | Wire existing ares through the approved emulator adapter; keep C ABI unchanged unless S07 explicitly requires otherwise. | S07. Generated NES/SNES tests; single-session enforcement; load/unload errors; suspend capture/replay; bounded copied frame/audio if crossing ownership boundary. |
| L24 / Luna | Wire existing SDL audio through the approved adapter; expose queue demand as data to shared pacing. | L05/S07/L23. Priming, pause/load reset, missing device, 50/60/120 Hz and stall tests. No per-refresh emulator clock or sample drops to hide drift. |
| L25 / Luna | Add explicit focus/background/audio-interruption/resume transitions using host events; implement desktop event mapping first. | L19/L23/L24. Input cleared, no unintended resume, no catch-up burst after background, save pending/failure visible. OS termination is not the only persistence trigger. |

**Checkpoint C / Sol:** test shared behavior without SDL/native linkage, replay
desktop event traces, run the full native/image/UI/audio/suspend/relocation
suite, and inspect ownership and nonblocking behavior. Only then consider a
renderer replacement. New Metal/Vulkan/wgpu code is a separately justified task.

## Phase 4 — one real host at a time

These P work packets are **not single-agent tasks**. Dispatch each listed
numbered subtask separately to Luna after Sol has resolved the host contracts.
Use the architecture document's runtime matrix as the release gate.

### P-Apple (depends on S02/S03 and Checkpoint C)

1. Create a minimal iOS app shell and target-specific native library build;
   simulator displays synthetic shared scene data. No image import yet.
2. Wire iOS local document ingestion and sandbox staging with cancellation and
   permission-lifetime tests; retain no external path assumptions.
3. Wire persistent settings/SRAM/suspend storage and explicit backup/export;
   test restart, interrupted/partial writes, denied/out-of-space access and
   restoration from exported data. Report durable completion, not buffered write.
4. Wire ares and audio; test interruptions and background/resume before loading
   local game profiles. Touch navigation and hardware-controller tests are
   separate steps, not implied by SDL initialization.
5. Create the tvOS shell and remote/controller focus adapter separately; no
   keyboard or mouse may be required for recovery from empty/error states.
6. Implement the ingestion and evictable-storage recovery strategy proven by S03c
   for tvOS; test image plus optional firmware acquisition, denied/quota errors,
   simulated cache eviction, interrupted writes and restart recovery.
7. Add target packaging/signing instructions and CI simulator app tests, then
   record physical iPhone/iPad/Apple TV runtime/performance evidence separately.

### P-Web (depends on S01/S03/S05 and Checkpoint C)

1. Build a browser shell that renders a synthetic scene through the selected
   supported renderer, with a browser-driven callback; add an actual browser CI
   launch test, separate from `wasm32-unknown-unknown` library checks.
2. Wire keyboard/gamepad, resize/letterbox, user-gesture fullscreen/audio and
   visibility handling; test input release on blur, neutral-on-reconnect and
   rejected audio-context resume without an emulator catch-up burst.
3. Implement local file ingestion using the bounded/worker strategy proven by
   S05; test cancellation, missing companions, unsupported XZ policy and memory
   failure. No upload endpoints or bundled console content.
4. Implement browser profile/save transactions with durability acknowledgments,
   quota/denied-storage failure, eviction recovery, export/import and restart
   recovery; test interrupted transaction and reload using actual storage APIs.
5. Wire the proven WASM ares owner and bounded audio transport; validate generated
   NES and SNES runs/replay. Web Audio sample rates, suspended contexts and tab
   throttling must not change emulator speed or leak buffers.
6. Package a deployment with required MIME types/headers and a documented isolation
   policy; test fresh origin, reload/offline behavior, denied storage, lost-context
   recovery (including rebuilding render resources without discarding saves)
   and supported browser engines. Service-worker/PWA installation is optional
   later work, not a prerequisite for a working browser app.

## Final gate / Sol

- Review complete diff and dependency direction; exercise shared behavior with
  fake host/renderer/core and both failure and successful completion paths.
- Re-run desktop regression evidence and actual new-host app/runtime tests; a
  generic badge or successful static-library build is not an app-support claim.
- Record toolchain/target/browser/device versions and unavailable tests. Preserve
  distinctions between configured CI, compile check, simulator, runtime and
  physical-device verification in `docs/platform-builds.md` and the README.
- Do not declare completion while S01–S03c or any required platform release gate
  is unresolved. It is legitimate to ship the improved desktop architecture while
  mobile/browser tracks are explicitly experimental or blocked.
