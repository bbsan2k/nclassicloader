# Desktop-to-shared update contract (S06)

This is an incremental extraction contract, not evidence that an Apple/browser
app or a nonblocking shared application loop already exists. SDL stays the
desktop renderer; ares stays on its current owner thread.

## Desktop service checkpoint

The tested desktop picker/import/preference seams are sufficient to start the
next mechanical extraction. Evidence is in [the task queue](frontend-tasks.md):
stale/cancelled outcomes, partial/error writes, directory/source reads, concurrent
publication, before/after-publication process exits, source immutability, cache
reuse, and old-profile migration. Profile JSON reads are stream-bounded as well
as metadata-checked. This is not a claim of universal storage durability.

Keep these explicit limitations: malformed/unreadable old manifests are skipped
under existing migration policy; interruption can leave unpublished orphan
stages; no power-loss/fsync guarantee, physical dialog/controller verification,
or mobile/browser storage/runtime gate is implied. New hosts need their own
service acceptance tests. The existing input stability/race assumptions are not
new filesystem snapshot guarantees.

## Ownership and dependency direction

- Desktop startup reads profiles/preferences, discovers sources, and creates
  window/controller/render resources. Shared initialization accepts plain data.
- Native launcher session ownership is extracted first (L18a). Its filesystem
  paths, channels and worker owners do **not** become shared frontend types.
- Shared reducers accept logical input and already-eligible host notifications.
  Picker/job correlation stays authoritative in the existing native owners;
  reducers must not invent another request counter or accept raw callbacks.
- Shared state and effects contain data/opaque keys, not SDL handles, paths,
  clocks, threads, ares sessions or I/O callbacks. Host maps keys to owned data;
  an opaque key is not authorization to open arbitrary paths.
- Desktop executes effects outside reduction. Profile validation, persistence,
  source resolution and dialogs are host operations, even if initially still
  synchronous in the outer desktop adapter. Do not call that adapter responsive
  on all hosts until its blocking effects are moved/proven there.

## One update

Input is an explicit ordered batch plus elapsed time supplied by the host. An
update reduces a bounded number of logical events/completions and returns owned
effects and display state. It never polls SDL, performs filesystem work, opens a
dialog, waits for a worker, joins a thread, sleeps, renders or steps ares.

Choose and test a concrete batch budget when wiring L18b; backlog must remain
ordered for the next update. Never silently discard completion, close, release
or focus transitions to meet a budget. Only explicitly replaceable progress or
pointer-position updates may be coalesced. No unbounded effect/feedback loop may
run inside one update. Cancellation is requested, not acknowledged, until the
existing job lifecycle says so.

Closing has priority over source/picker activation captured earlier in a batch.
During a pending picker/import, no cached/image/folder source overtakes it.
Focus changes reseed input but do not invalidate eligible selections. Held
confirm/back inputs must be released before acting across busy/focus transitions.
Stale completion/progress cannot mutate busy, notice, selection or activation.

## Launcher frame protocol (L18b)

The reusable boundary is a data-only coordination protocol, not a claim that the
native SDL loop is now a shared application loop. A host begins an
`InputFrame`, streams an ordered, bounded set of logical events through the
production `Navigation`/`Activity` reducer, and executes any returned
cancellation request outside that pure call. `finish_input` yields a typed
`FramePlan`; the host applies persistence (acknowledging its saved-filter copy
only after a successful write), service shutdown, exit, and then the planned
activation in that order.

After input effects, each host owner validates notification identity and kind
before passing an eligible picker/import completion to the shared completion
plans. An accepted picker source is handed directly to the host operation in
that same completion phase; there is no intervening logical input tick. A new
asynchronous host must similarly acknowledge acceptance immediately or reserve
the busy/picker phase before deferring work, so another source cannot overtake
it. Shared plans receive neither request IDs nor paths/profiles; host-side
correlation remains authoritative. Cancellation requests do not clear busy;
only the owner's accepted result does. Rendering/presentation and outer-host
present/pacing follow these updates.

The current desktop still performs some filesystem validation and picker/import
effects synchronously outside policy. This protocol does not make those effects
nonblocking or establish a universal responsive backend. The external-host
trace demonstrates reducer/plan reuse and host-owned correlation, not OS dialog
or hardware behavior. L20 renderer-command work remains a separate boundary.

L18a keeps current loop ordering and behavior while separating startup, state
and shutdown. L18b then extracts logical event/effect reduction, possibly in
multiple medium chunks. L18c leaves event polling, effect execution, rendering,
presentation and pacing as visible outer-host steps. Do not mark all of L18
complete because state moved into a native struct.

## Render boundary

Prefer an owned scene/draw-command description rather than a giant renderer
trait. Current desktop operations require rectangles, textured quads, source
frames, tint/alpha, blend/filter mode, horizontal flipping, clipping and target
passes, plus bitmap text layout. Preserve the 1280x720 App stage and existing
launcher coordinates and letterboxed hit tests.

Resource keys must be typed/generation-checked by the host before resolving to
textures. A scene never retains borrowed SDL/core buffers or raw pointers.
Desktop owns upload, target creation/destruction and missing-resource errors.
Define actual command variants one existing view at a time in L20–L22; do not
build unused GPU/backend abstractions now. Font measurements and placement stay
the tested shared CPU policy. Drawing must not perform save/load/delete actions.

Emulator video/audio ownership is S07, not a side effect of renderer extraction:
keep current owner-thread creation/use/destruction, synchronous stepping and
borrowed-buffer lifetimes until an evidence-backed replacement is approved.

## Acceptance

### Verified implementation seams (2026-10-22)

`frontend::launcher_navigation`, `launcher_activity` and `launcher_frame` now
provide the data-only launcher policies and frame protocol. The production SDL
adapter uses them; a second fake host drives their public API without SDL/native
linkage. Native startup, services, effects and presentation have separate owners
and adapters. Rendering still uses the existing SDL `View`, not a new renderer.

The App uses shared `menu_input`, `menu_navigation` and `toolbar_navigation`,
alongside the existing language/suspend policies. Native keyboard/gamepad
translation feeds the same logical inputs; held gameplay masks remain separate.
Tests retain context/repeat priority, paused-versus-playing distinctions and
malformed-focus rejection. The App run driver and core/audio/render ownership
are still native. L20–L25/S07 remain separate work. L19c now separates the
desktop driver into the host steps documented below; it does not make those
native effects a universally nonblocking update.

Budgets constrain raw input/notification processing, not the latency of native
filesystem/dialog/persistence effects. Active service acknowledgments and
accepted-source handoff still obey the phase protocol above. These seams do not
establish browser/Apple responsiveness or a new storage durability guarantee.

Per-chunk tests exercise production helpers, not duplicated algorithms. After
three compatible Luna chunks, Sol reviews the combined diff and runs portable/
native tests, warnings-denied Clippy and applicable UI/import/suspend regressions.
Shared reducer traces must run without SDL/native linkage; native-only session
tests do not satisfy that gate. Snapshot parity alone proves neither input
correctness nor bounded updates. Platform runtime gates remain separate.

## Main menu, toolbar, and suspend policy (L19b)

The current shared keyboard boundary is a set of small transitions, not a shared
application loop. `frontend::menu_navigation` owns main-menu/general-panel
priority and returns stable indices or semantic intents. `toolbar_navigation`
owns toolbar focus/scroll and returns page intents; `menu_input::suspend_input`
translates logical keys to the existing `suspend_navigation::Input` before its
existing picker transition runs. A no-SDL external-host test composes these
production functions with a fake owner; it does not model SDL, core execution,
files, threads, or clocks.

The current native adapter binds indices to the frame it just rendered:
`ActivateFocused(i)` is eligible only when `actions.get(i)` exists, toolbar
`ChooseLanguage(i)` resolves through the current available-language list and
falls back to the existing `close` activation if absent, and toolbar pages
continue to use their existing action bindings (`display:*`, `frame:none`,
`emulator-settings`, `help`, and `close`). The transition does not retain action
strings or claim that an index authorizes an operation. Confirmation effects
are requests only: overwrite/delete target identity, lock state, save-slot
existence, and core availability remain checked by native activation/effect
handlers before I/O or core operations. Back dismissal is intentionally distinct
from Confirm activating `close`.

These traces characterize policy/effect routing and fail-closed malformed focus;
they do not establish physical keyboard/controller behavior, persistence, save
integrity, or emulator replay. They do not make the App driver platform-neutral,
move ares ownership (S07), or define renderer commands (L20). External
hosts still need their own resource-authority guards and service acceptance
tests; an intent is neither an I/O result nor an acknowledgement.

## App desktop frame steps (L19c)

The production desktop driver now explicitly polls at most 128 SDL events,
executes native update work, draws the fixed stage, captures a requested final
finite-frame screenshot, presents, advances the shared frame limit, and paces.
The notification batch is capped at 64; import progress has its existing separate
64-message budget. Lazy iterator limits retain FIFO tails instead of truncating
a fully drained queue. After Quit, new activation/drop/menu commands are ignored;
release and focus-loss cleanup can still clear held input.

On actual closing, import/picker requests close before dialog completions are
processed, and gamepad navigation is skipped. Native title/fullscreen effects
and the existing last-frame audio/core behavior retain their order. Stage
creation/drawing are native adapters; drawing still builds hit/action metadata
and animation state, not a renderer-independent scene. Present/pacing and notice
expiry remain visible outer-host operations.

Finite-run completion never re-enables a latched Quit and does not permanently
close reusable request owners. Existing finalization remains pending capture
when playing, settings writing, then loaded-core unload. Native I/O/core work can
still block: the reusable pure policy and explicit native phase boundaries do
not establish a new asynchronous emulator owner or a new-platform runtime.
