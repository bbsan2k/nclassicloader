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

### Launcher text-request slice (L20g onward)

The next approved incremental boundary is an ordered owned `DrawCommand` with
`Geometry(GeometryCommand)` and `Text(TextCommand)` variants. A text request owns
its string and plain stage x/y, u16 pixel size, RGBA tint, maximum width and a
typed `FontRole::{Regular, Semibold}`. Roles name the launcher's two embedded
Barlow faces, not reusable texture IDs, external font paths or per-View resource
handles. A new native View resolves those semantic roles to its own successfully
loaded fonts; no old glyph/texture reference can survive View destruction in a
request. This does not implement the dynamic generation-checked texture-key
contract required for later image/video resource commands.

The native executor preserves the existing top-of-line y convention, single-line
ellipsis fitting, size/kerning/ascent calculations, 2× glyph rasterization,
linear filtering, tint/alpha and errors by calling the unchanged native `text`
routine. Maximum width is a fitting limit, not a clip rectangle. It executes
mixed geometry/text requests strictly in order and propagates text errors before
later commands. This is owned presentation intent, **not** a shared glyph-layout
implementation or proof of cross-renderer pixel parity. No measurement callback,
fontdue dependency, SDL handle, filesystem access or action runs in scene building.

Start with branding, then sidebar, then header/filter tabs in separate verified
chunks. Native input still supplies eligible styling data; scenes do not grant
click/action authority. Remaining row/detail/status text, font measurement,
flags/resources and full scene composition are separate follow-ups. Flags are
not added to this command enum until their own resource contract is approved.

L20g–i implement these first three groups in production. The native executor maps
both font roles exhaustively and retains the original font routines unchanged;
sidebar label placement derives from shared button geometry. Four external text
tests characterize owned strings, exact requests and interleaving, sidebar state
combinations and header/filter/count cases without SDL. All 14 launcher previews
match a fresh pre-L20g baseline outside the elapsed-time progress strip. This
does not establish native font failure injection, shared glyph measurement or a
second renderer; those remain separate verification work.

The next five approved slices are empty-library requests, primary-button
requests (caller supplies label/enabled/hover; action eligibility stays native),
footer requests, page-indicator requests, and shared flag-region metadata.
Footer extraction moves the existing four-value `CancellationView` presentation
enum to shared launcher activity with a native compatibility re-export; worker
state-to-view mapping and cancellation ownership remain unchanged. Pagination
retains legacy selected-index/count arithmetic without new clamping. Shared
region metadata moves only exact marker/label matching, not texture resolution,
SVG loading, fallback geometry or a new flag draw-command variant. These slices
do not change the unresolved status-measurement or image-resource contracts.

L20j–n implement those five slices. Empty-library and primary-button commands
keep geometry before text and own their string data; native callers retain the
empty-list guard and primary-button eligibility/label decisions. Footer requests
use shared launcher activity's `CancellationView` through the native compatibility
re-export; worker mapping and lifecycle behavior are unchanged. The page
indicator preserves empty counts, out-of-range selections and maximum-index
arithmetic. `launcher_scene::Region` now provides exact marker/label policy via a
native flags compatibility re-export, without changing SVG/texture/fallback
ownership or adding a flag-command variant. Ten external text tests and one
external region test run without native linkage. All 14 launcher previews match
the fresh pre-L20j baseline outside the elapsed-time busy strip after each slice.
These additions still leave status measurement, row/detail text, flag resources
and complete scene composition unresolved.

### Approved flag/row/detail/status slices (L20o–s)

`DrawCommand::Flag { region: Region, rect: Rect }` names a fixed built-in flag
role, not a programmable texture ID. Every native View owns its three loaded
SVG textures and resolves the enum against that current owner; Unknown retains
the existing neutral fallback. Scene replay after View replacement resolves
the new owner's assets and cannot reference an old texture. SVG loading, border/
fallback geometry, filtering and the existing ignored copy-error behavior stay
native. This semantic-role command is not a dynamic resource registry or proof
of the later generation-checked image/video resource contract.

Extract the two flag placements, then complete one library row, then the detail
header and metadata as separate chunks. Row/detail inputs contain plain display
strings, normalized region and an optional controller-illustration kind, never
Entry/Profile/path/worker types. Input strings may be borrowed during building;
all returned text requests own their strings. Native classification, selection,
source-path-to-display conversion and primary action eligibility stay native.
Preserve the exact mixed geometry/text/flag order.

Status wrapping uses a narrow `StatusText` measurement handshake, **not** a
measurement callback in scene building. The pure policy exposes the next exact
candidate string; the host measures it with existing size-15 regular-font width,
then advances one word using that scalar width. An advance can emit one owned
line request, drawn before the host measures another candidate. The host finally
draws the policy's final request, even when empty. Preserve leading-space
candidate measurement, Unicode split_whitespace, strict width >550 comparison,
non-wrapping first overlong word, y=657/678/699 and the legacy final empty line at
y=720 after the cutoff. Error color and single-line fitting remain unchanged.
This handshake moves neither font metrics nor native errors into shared code;
the host's total word processing still has legacy input-dependent cost, not a
new bounded-update guarantee. Do not premeasure all lines before drawing, or
drop the terminal empty request, since that changes failure/measurement order.

L20o–s implement these five production slices. Library rows and detail headers/
metadata now build complete ordered groups from plain borrowed display inputs,
returning owned commands. Native classification/path-display conversion and
primary action eligibility remain native. `frontend::launcher_status::StatusText`
implements the scalar-width handshake, and the native host draws emitted lines
before measuring another candidate. Eight external status tests cover original-
loop traces and edge cases; the simulated draw-failure test is not native GPU/
font failure injection. Complete frame composition and View-recreation evidence
remain open. All 14 launcher previews match a fresh pre-L20o baseline outside the
elapsed-time busy strip; no second-renderer or new-platform runtime is implied.

### Approved launcher composition slices (L20t–x)

Native projection builds an owned shared `launcher_presentation::Presentation`
from the visible native entries and host styling inputs. It contains only plain
display strings, counts, booleans, elapsed seconds, existing cancellation/font/
region/controller roles, visible row display data and selected detail/primary
display data. Project only the current six-row page plus selected details, not
the entire library. Filesystem paths remain native; their displayed spelling is
text, not a storage capability. Keep the current empty/malformed-selection
behavior and native label/eligibility decisions.

Shared composition emits an owned `Scene` with `before_status` commands, owned
status/error data and `after_status` commands. This explicit measurement barrier
preserves the current order: background, branding, sidebar, header/tabs, rows or
empty panel, detail header/metadata, primary button, page indicator; then the
status handshake; then progress and footer. Status is not premeasured. Native
execution stops on the first propagated text error and never measures/draws later
work after it. Building display data and commands is pure but allocation/input-
dependent; no new whole-frame latency or memory bound is claimed.

The five slices are production projection, production composition/execution,
pure fake-host composition/error-order tests, opt-in dummy-SDL resource-recreation
tests, and native projection/interaction parity tests. Replay a retained scene
after rebuilding View/canvas resources: symbolic built-in roles resolve against
the current owner only. Opt-in SDL tests must use caller-supplied dummy drivers,
not mutate process environment or open interactive windows during default tests.
These tests concern launcher resources, not App textures/core buffers or new
platform runtime support. L20 acceptance remains a separate review of coverage
and retained font/render-host responsibilities.

L20t–x implement that boundary in production: native projection owns the visible
page's display strings, `launcher_presentation::compose` owns the complete
before/status/after scene, and SDL's `draw_scene` executes it with the existing
status handshake. Seven external composition tests cover full mixed-command
order, malformed display input, owned scenes, fake-host error stops and symbolic
role replay. Six native projection tests retain classification, focus/busy
styling and half-open letterbox hit behavior. These tests do not grant scene
inputs action authority or claim physical-controller coverage.

The ignored native test
`ui::launcher::view::recreation_tests::retained_scenes_replay_after_view_and_canvas_recreation`
was explicitly verified with `SDL_VIDEODRIVER=dummy SDL_RENDER_DRIVER=software`
and `--exact --ignored --test-threads=1`. It reads full-size RGBA pixels, checks
both font-role caches, compares warm replay, destroys View/canvas and compares
the same retained scenes against newly created resources. Normal, empty and
busy/error scenes include all four flag roles. It writes no files and mutates
no environment. This is software SDL resource-recreation evidence, not generic
GPU context-loss recovery, native font-error injection or Apple/browser runtime.
All 14 launcher previews match the fresh pre-L20t baseline outside the elapsed-
time busy strip. L20's desktop extraction is implemented; L21's dynamic App
texture/target-pass contracts and Checkpoint C remain separate.

## Initial App carousel/HUD slices (L21a–j)

The next ten slices introduce an App-specific `frontend::menu_scene` leaf-command
boundary, not another launcher API or new GPU backend. A `TextureId` contains a
process-local owner generation and slot, never a pointer/path. Native UiAssets
bindings issue a fresh checked, non-wrapping owner generation per assets owner
and bind only successfully loaded textures. Repeated loads keep the same ID;
new profile/assets owners reject old-generation IDs before touching a texture.
Unknown slots also fail closed. IDs are presentation references, not storage
capabilities, persistent identifiers or a stable binary ABI.

Texture quads carry plain destination/source rectangles, tint/alpha, nearest or
linear filtering and horizontal flip. The SDL adapter retains Blend mode on
loaded textures, resets modulation/filter on every resolved copy and preserves
the existing ignored copy-error/missing-texture behavior. Invalid IDs are skipped,
not reinterpreted as paths or automatically rebound. UiAssets still owns upload
and explicit texture destruction. The native fixed 1280×720 stage target,
clear/present order and App/core/suspend-preview textures stay outside this first
leaf-command contract; no target-pass or video-buffer ownership is moved here.

The ten serial slices are: (a) typed texture IDs and native bindings wired to
actual texture loading/resolution; (b) texture-quad execution; (c) atlas-sprite
placement/flip commands; (d) fitted-image quads; (e) CPU bitmap-text placement;
(f) selection nine-slice planning; (g) wallpaper fill/texture output; (h) HUD
prompt placement; (i) thumbnail placement; (j) invalid/stale-resource and native
dummy-SDL copy/recreation tests. Sprite/selection/HUD/thumbnail planning data is
plain layout/style data, not itself a retained authorized texture reference;
native atlas/font/game lookup binds current-owner IDs before command execution.
Native ROM/firmware/save guards, translation, hit/action binding and clocks remain
native. Preserve arithmetic, fallback names, rounding, clipping/culling and all
valid-desktop behavior. These are partial L21 seams, not complete App scenes.

Verify pure policies without native linkage, native bindings/adapter tests and
warnings-denied Clippy per slice; compare fresh SNES/NES/Famicom menu snapshots.
Final verification includes existing UI/suspend/audio checks where local assets
permit, plus opt-in synthetic dummy-SDL resource tests. Native GPU error handling,
lost-context recovery, full scene composition and new-host runtime remain later
gates. Do not silently add target-pass/renderer abstractions during these slices.

L21a–j implement those ten production/verification slices. Current UiAssets owns
the native name-to-ID bindings and generation validation; shared quads never
resolve a path. Sprite flipping, fitted-image geometry and bitmap-font placement
use the original arithmetic. Wallpaper output is a real fill/texture command
group; selection, HUD and thumbnail output is layout data whose native caller
binds atlas/font/game resources and actions. No App target-pass or core-buffer
ownership moved. Nonempty catalog validation remains native, and shared thumbnail
planning explicitly returns empty for zero items.

The opt-in native test
`ui::menu_resource_tests::typed_texture_commands_reject_stale_ids_and_replay_with_fresh_assets`
was verified with dummy video/software rendering and `--exact --ignored
--test-threads=1`. Synthetic pixels exercise actual flip/crop/filter/modulation
reset, fill, malformed/stale IDs and newly created assets/canvas owners; no user
image files are required. The pre-existing launcher resource-recreation test also
passes. Neither test proves arbitrary GPU context-loss recovery or moves dynamic
stage/video/preview ownership into the leaf-command contract.

All 132 SNES/NES/Famicom UI/language snapshots match fresh pre-extraction baselines
outside the elapsed-time launcher busy strip; real-core suspend/navigation checks
pass on all three systems. Release SNES/NES dummy-audio timing checks pass; a
failed debug SNES rate sample (53.04 Hz at the 120 Hz presentation phase, no queue
underruns) is retained as a separate limitation. No timing threshold was relaxed.
Complete App scenes, target-pass contracts, physical performance and new-platform
runtime gates remain open.

### App carousel/title slices (L21k–t)

The next ten serial chunks are: current-owner atlas/image source snapshots;
owned bitmap-glyph command capture; injected-time menu motion policy; card
layout/style planning; typed card-scene composition; typed title-scene output;
typed arrow/hit output; typed selection-scene binding; external fake-host/
ownership/resource tests; and native dummy-SDL retained-scene replay tests.

`AtlasSprite` snapshots contain only TextureId/source rectangle/flip data;
`ImageSource` adds plain texture dimensions. Native lookup never loads a file
while binding these snapshots. Font-atlas bindings are plain font-name-to-ID
data; shared glyph command construction uses the existing measured bitmap
layout/placement rules. All returned scenes own their commands and retain no
native texture/font/game objects. Failed lookups skip only the corresponding
draw, preserving positions and hit output; stale IDs remain stale until the
native executor rejects them, never implicitly rebound to a replacement owner.

Keep native carousel candidate iteration, game-ID lookup, firmware/ROM checks
and culling order unchanged, including checks before the x-range cull. Native
save/lock queries supply normalized four-slot presentation data. Shared card
plans describe draw order and a hit rectangle, not save/launch authority or game
paths. Native code binds hit geometry to the current game's existing action and
retains all actual activation guards. Arrow output uses a closed Previous/Next
hit tag; it is not an arbitrary effect or string-to-I/O dispatcher.

Motion receives native elapsed seconds and layout scalars as data. Preserve
existing suspend shifts, 0.24-second scroll interpolation, wallpaper-duration
floor, and motion-disabled selection time. Do not touch emulator/audio clocks.
Title/carousel/selection scenes still render into the existing native stage at
their original draw sites; no target-pass, clipping, render-loop or core-buffer
ownership changes are authorized. Projection/command building allocates and is
not a new whole-frame responsiveness/memory guarantee.

L21k–t implement those ten slices in production and focused tests. Native source
snapshots feed shared typed card/title/arrow/selection builders; bitmap glyph
commands are captured using the existing fallback/placement policy. The native
carousel loop retains firmware/ROM checks before culling and binds resulting hit
geometry to the current game's existing action. Arrow hit tags remain a closed
Previous/Next pair and keep their original unshifted hit rectangles. Slot state
normalization stays native; shared scene output does not authorize operations.

Three external mixed-scene tests cover composition via a test-local host (not a
production full-frame composer), actual input ownership and generation/slot
rejection without rebinding. The explicitly verified ignored native test
`ui::menu_carousel_tests::retained_carousel_title_scenes_require_current_assets_generation`
uses synthetic atlas/cover/font textures, current native source/font captures and
shared scene builders. Full-size RGBA readback matches warm replay and newly
created assets/canvas owners for normal, suspend and NES scenes. Old typed
commands are rejected by the replacement owner and leave the clear canvas
unchanged. This is software-SDL resource evidence, not general GPU reset/error
handling or a new renderer implementation.

All 132 UI/language snapshots match fresh pre-L21k baselines outside the elapsed-
time launcher busy strip; real-core suspend/navigation checks pass on SNES/NES/
Famicom and release dummy-audio checks pass on SNES/NES at 50/60/120 Hz. Full
menu/App composition, target passes and platform runtime gates remain open.

### Base-menu frame slices (L21u–ad)

The next ten slices are bound thumbnail scenes; bound four-item HUD scenes;
menu-bar scenes; owned base-frame composition; native owner-checked execution/
hit binding; native read-only frame projection; external whole-frame tests;
native executor fault tests; native complete base-frame resource replay; and
shared reverse-priority hit selection used by native hover/click handling.

The base frame ends before the toolbar/tooltip loop and modal/panel overlays.
It orders wallpaper, title, each card plus its hit, selection, both arrows plus
their hits, each thumbnail plus its hit, each HUD item plus its hit, then bars.
Composition returns owned Draw/Hit steps, not callbacks. Game hits carry an
opaque captured GameKey string, not a mutable catalog index or path. Native hit
binding prefixes only validated current-catalog keys and maps closed arrow/HUD
tags to the existing action names; unknown game keys are ignored. Stable keys
preserve bindings across sort changes without resolving an old index against a
new catalog. Hit registration never executes an activation/save/core operation.

Each frame and its native hit-binding context carry the current assets owner
generation. The native executor checks both against its current owner before
any drawing or hit mutation, then still validates each texture ID normally.
Wrong-owner frames are rejected wholesale, including fill/hit steps. Missing
individual resources skip only their draws; valid current-context hits keep the
legacy behavior. Game keys and generation stamps are presentation context, not
storage/launch capabilities or persistent IDs.

Projection owns display data and generation-bound resources, leaving clocks,
selection mutations, filesystem checks and action binding native. Preserve
firmware/ROM check order among carousel candidates and before culling, and the
distinct firmware-only thumbnail guard. Read-only projection now precedes draw
execution; no I/O side effects, native copy-error propagation or frame-atomic
filesystem snapshot guarantee is introduced. The existing metadata/source
stability assumptions remain. Toolbar/overlay execution follows the base frame
at its original site and can still replace/append hits as before.

Keep stage creation/clearing/presentation and core/video/suspend resources native.
Do not invent generic target-pass or renderer traits in this batch. Reverse hit
selection must retain last-hit-wins and half-open rectangles without allocating
a second hit list, while hover/click/game-start decisions remain native.

L21u–ad implement these ten slices. `frontend::menu_frame::compose` emits owned
ordered steps; native read-only projection feeds it, and `execute_base_menu`
checks scene/context/assets generations before touching pixels or hits. Native
binding uses current-catalog keys and closed action tags. Shared `last_hit`
returns a borrowed hit without an additional list; both native hover and click
use it while activation and double-click policy remain native.

External tests cover ordering, ownership, keys, missing draws, retained IDs and
half-open reverse-priority selection. Explicitly invoked dummy/software SDL
tests cover actual executor mismatches and complete base-frame pixel/hit replay
after assets/canvas recreation for normal, suspend and NES scenes. All five
resource tests, 132 UI/language snapshots, three-system suspend/navigation and
release SNES/NES 50/60/120 Hz dummy-audio checks pass. The base frame still stops
before toolbar/tooltips and modal overlays; complete App target passes and
Apple/browser runtime gates are not completed by this batch.

### Toolbar and overlay-boundary slices (L21ae–an)

The next ten serial slices are toolbar layout/focus policy; owned tooltip
commands; bound toolbar scenes and closed hit tags; native read-only toolbar
projection; combined base/toolbar composition; closed overlay routing; owned
notice presentation; external composition/ownership tests; native executor fault
tests; and retained combined-menu resource replay.

Preserve per-item order: optional active background, icon, hit, then optional
tooltip (body, left/right edges, arrow, glyphs), before the next toolbar item.
Hover and open-panel highlight checks remain exact legacy string comparisons;
keyboard focus is eligible only with no open panel. Tooltips appear for hover
or eligible keyboard focus only when the panel is empty. Translation, palette
and JSON style decoding stay native. Closed toolbar tags bind only the existing
five toolbar actions, never arbitrary action strings or filesystem operations.

Combined base/toolbar scenes require matching owner generations before native
execution; stale texture IDs still fail individually, without deleting valid
current-context hits. Projection may precede copies but preserves existing game
checks and ordering. Overlay route selection is pure display routing, not a
replacement for native eligibility/activation logic. Retain existing dispatch
precedence, overlay hit/action clearing and append behavior. Notice presentation
may capture glyph commands and plain dialog geometry, but native SDL outline
rasterization remains unchanged. Check the owner before notice drawing.

Do not move suspend/toolbar-panel resources, dialogs' native rasterization,
stage targets, core/audio clocks, storage authority or effects. Full overlay
scene extraction and App target passes remain later work. Tests must distinguish
synthetic software-SDL replay from real GPU recovery or Apple/browser support.

L21ae–an implement these ten slices. Native read-only toolbar projection feeds
owned toolbar steps, and `with_toolbar` rejects differing generations before
merging with the base scene. Production executes the combined scene once before
the unchanged native overlay calls. Closed toolbar hit tags bind to the existing
five action names. Tooltip commands retain item-local order and native text,
palette and JSON style policy. Overlay route selection preserves exact strings,
including the legacy `toolbar:` prefix behavior for unrecognized suffixes.

Notice scenes own glyph commands and dialog geometry. The native notice executor
checks its owner first and uses the same black fill and three white SDL outline
calls as other dialogs; no shared fill approximation replaces SDL rasterization.
Other overlays retain native payload rendering and hit/action replacement.

External composition tests and actual software-SDL fault/replay tests cover
combined ordering, closed hits, missing resources, mixed texture generations,
notice pre-draw rejection, simultaneous hover/keyboard tooltips and fresh-owner
pixel/hit parity. All seven resource tests, 132 UI/language snapshots, three
extra notice snapshots, three-system suspend/navigation and release SNES/NES
50/60/120 Hz dummy-audio checks pass. Remaining modal payloads, complete App
target passes and platform runtime gates are still open.

### Generic modal panel slices (L21ao–ax)

The next ten serial slices cover generic-panel row viewport policy; bound row
commands; bound footer commands; heading/body text capture; owned panel
composition and native execution; native read-only projection; external panel
tests; native key-binding tests; actual executor fault tests; and retained panel
resource replay. This concerns `render_panel`, not the distinct toolbar-panel
or suspend renderers.

Preserve dimmer/dialog/heading/body/visible rows/footer order and exact viewport
arithmetic, including zero entries, focus equal to entry count, and the minimum
one-visible-row policy. Focus clamping, profile/language enumeration, import
notice/cancellation state, action strings, translations, wrapping and all actual
effects remain native. Shared row/footer builders receive normalized highlight
and enabled geometry data, not permission to operate on files or imports.

Panel scenes own commands, plain dialog geometry and opaque captured PanelKey
strings. Native binding accepts only keys in the current projected panel's
allowed action set; never resolve scene indices against a mutable profile or
language catalog. Keep the keyboard action list in full entry order (not only
visible rows), then the enabled footer action. Disabled import footers have no
hit or keyboard action. Existing activation/cancellation guards remain native.

Scene, binding context and current-assets generations must agree before native
execution changes pixels, hits or keyboard actions. Successful panel execution
replaces hits/actions as before; missing individual textures skip only draws,
while valid allowed-key hits remain. Native SDL dialog rasterization is reused,
not approximated. Read-only projection may precede copies; retain native state
ownership and existing frame-local metadata assumptions. No new storage,
whole-frame responsiveness or context-loss guarantees are introduced. Remaining
toolbar/suspend payloads, App target passes and platform runtimes stay open.

L21ao–ax implement these slices. Native `project_panel` captures metadata,
current resources and allowed keys; shared builders produce owned row/footer/
text commands and ordered draw/dialog/hit steps. Execution checks all three
generations before replacing hits/actions or drawing. Native action binding
filters absent keys without resolving mutable catalog indices. Focus updates
only after successful execution; full keyboard entry order and enabled-footer
semantics are retained. The native row-projection helper advances the running
Y coordinate for each visible row and is exercised directly by replay tests.

Luna implemented the first six chunks; Sol completed the final four directly at
the user's request and fixed a row-position regression found in review. External
tests cover ordering, ownership, missing draws, stale IDs and captured keys;
native tests cover allow-list binding, whole-owner rejection before pixels/hits/
actions change, and warm/fresh-owner pixel/hit/action replay for scrolled lists,
messages and enabled/disabled footers. All nine opt-in SDL resource tests, 132
UI/language snapshots, three-system suspend/navigation and release SNES/NES
50/60/120 Hz dummy-audio checks pass. Generic-panel extraction does not complete
distinct toolbar/suspend payloads, App target passes or platform runtime gates.

L20a implements only resource-free launcher controller illustrations in
`frontend::launcher_scene`: owned rounded rectangles with plain geometry and
RGBA colors, consumed by the existing SDL scanline adapter. External-host tests
characterize every command without native linkage, and three launcher previews
retain pixel parity. L20b adds ordered background fills (scanline wash, translucent
sidebar and separator), preserving alpha and draw order. L20c shares stage/page
sizes, button/tab/row rectangles and window-to-stage letterbox mapping between
native rendering and click handling. Mapping retains points outside the stage
and the original minimum scale, including zero-sized windows; it does not itself
authorize an action. External scene/layout tests run without native linkage.
All 14 launcher previews match the pre-L20b baseline outside the explicitly
excluded elapsed-time busy-animation strip.

L20d–f additionally emit geometry for the sidebar/tab chrome, library row
border/inset/backplate, empty-library panel, detail/unknown-controller panels,
separator, primary-button background and progress track/thumb. Production retains
each command group's order relative to text and flags. Styling consumes plain
indices/booleans/time; it neither grants input/action eligibility nor changes
service ownership. The progress animation preserves legacy arithmetic and is not
an import-progress acknowledgment. Seven external scene/layout tests cover the
public API, including fixed-time animation geometry. All 14 launcher previews
match the fresh pre-L20d baseline outside the elapsed-time strip. Text fitting/
wrapping, flag drawing/resources and full ordered scene composition are still
native; these geometry seams do not complete L20 or select a new graphics backend.

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
