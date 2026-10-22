# Frontend portability: architecture and incremental delivery

Status: **design and staged extraction**, not a claim of mobile/browser support.
The desktop Rust/SDL app remains the working product and regression reference.
See [platform status](platform-builds.md) for the existing build scope.
The executable handoffs, prerequisites and gates are in the
[small-agent task queue](frontend-tasks.md).
Concrete cross-target probes and blockers are recorded in the
[feasibility evidence](frontend-feasibility.md).
The first asynchronous service contract is the
[image-picker request/completion contract](frontend-effects.md).

## Decision

Separate application behavior from host services before replacing the graphics
backend. Keep SDL as the first renderer/audio/input adapter. Do not introduce
Vulkan, Metal, wgpu, a new UI toolkit, or a new emulator in an extraction task.
Reassess the renderer only after a second host can exercise the shared behavior.

The shared frontend must be usable without SDL, ares linkage, a desktop event
loop, an OS directory layout, or an available thread pool. Merely moving files
under a new folder does not meet this requirement.

### Target dependency direction (not yet implemented)

```text
desktop / Apple mobile / browser shell
  ├── maps host events to application input
  ├── drives update with monotonic elapsed time
  ├── executes effects through host services
  └── presents a scene through a renderer
                         │
                  shared frontend
           model → update → effects / scene
                         │
        plain IDs, rectangles, colors, data, errors

host implementations → importer / storage / emulator / audio
desktop host          → SDL / filesystem / ares C ABI
Apple host            → lifecycle / sandbox / platform ingestion
browser host          → browser callbacks / local storage / WASM core
```

This is a direction, not a requirement to create all these crates or traits in
advance. Extract modules inside the current crate first. Add public interfaces
only when a real caller needs them; do not create an unused portability layer.

## Contracts that need Sol review

- **Update is nonblocking:** input, elapsed time, service completions and lifecycle
  events go in; state changes, bounded work requests and scene data come out.
  No file dialogs, disk scans, sleeping, joining workers, or emulator loops in
  shared update/render code. Avoid an unlimited effect-draining loop.
- **Effects have identities:** asynchronous import/load/picker/save results carry
  request and profile/session identities. Ignore stale results after switching
  profiles, cancellation, shutdown or controller/focus transitions. Failure and
  cancellation are distinct. A host may complete work later without OS threads.
- **Capabilities describe usable behavior:** no directory picker or nearby-image
  discovery is assumed. Unsupported operations are hidden or explicitly explained,
  not exposed as buttons that cannot work. tvOS needs an actual user-data ingestion
  path, not an imaginary desktop filesystem dialog.
- **Storage acknowledges durability:** an in-memory write is not a durable save.
  Report pending, successful durable completion, quota exhaustion and failure.
  Keep imported assets, user settings, SRAM, suspend metadata and previews separate.
  Profile upgrades and corrupt-state protections remain intact. Provide export/
  recovery where a platform can evict local data.
- **Emulation keeps thread affinity:** `AresSession` is intentionally not `Send`.
  Never transfer a live session to a worker or renderer thread. If a host owns a
  dedicated emulation thread, create, use and destroy the session there. Borrowed
  frame/audio buffers cannot outlive the next mutating core call; cross-thread
  transport needs explicitly owned data and bounded buffering.
- **Presentation is not the emulation clock:** preserve audio-buffer-based pacing,
  catch-up bounds and missing-audio fallback. Inject time; do not assume each
  browser callback, display refresh, or redraw means one emulated frame.
- **Lifecycle is explicit:** focus loss, controller disconnect, audio interruption,
  suspend/resume, hidden browser tabs, lost graphics context and termination have
  defined transitions. Stop input, avoid surprise resume, and do not rely on a
  termination callback to save progress. Background flushes may not finish.
- **Scene data has no backend handles:** texture/resource IDs and geometry may be
  shared; SDL textures, windows, pointers and GPU lifetimes stay in the adapter.
  Specify clipping, alpha, scaling, filtering and text measurement before moving
  a drawing path. Preserve imported bitmap-font metrics and the 1280×720 stage.
- **Import remains read-only and bounded:** browser file selection is local data,
  never an authorization to upload content or execute extracted programs/scripts.
   Keep hostile-image checks and fail closed for unsupported compressors/formats.

### Parsing boundaries (L13)

Catalog and language decoders consume JSON values or bounded byte slices, not
paths. Their plain output data is available without native features. Desktop
adapters retain profile-relative file opening, regular-file/symlink checks,
size checks and path-qualified read/parse errors. A byte decoder checks the
existing 32 MiB limit before JSON allocation; native reads must also enforce it
after reading, not only trust earlier metadata. This does not promise atomic
filesystem snapshots or introduce streaming imports.

Preserve schema-1 catalog defaults, entry order, safe/unique IDs and all game
metadata. A serialized ROM or firmware name remains data, not authorization to
open a path: the existing native path, system and firmware checks still apply.
Language extraction preserves shipped-locale validation and metadata order,
legacy profiles without an index, the default-locale entry, and per-key fallback
to default strings then caller text. Texture/font loading and rendered text
measurement remain with the native renderer.

### Bounded image input (L15)

`image_reader::read_image_from(reader, size)` accepts an owned or borrowed
`Read + Seek` implementation with a host-supplied stable extent. The existing
`read_image(path)` wrapper supplies a file and its metadata length. Both enter
the same parser; range checks and the 1 GiB archive cap precede source reads,
and gzip streaming cannot consume beyond the declared extent. Decompressed
file/member/total limits and compressor checks remain unchanged. No `Send`,
background executor or whole-image copy is required by this API.

The host still owns immutable/snapshot-stable input and source identity checks;
this API does not establish a browser file/storage implementation or acceptable
browser memory usage. Parsing can still block. Local valid-image equivalence
can be checked without publishing/importing a profile:

```sh
CLOVER_SOURCE_PARITY_IMAGE="/path/to/local/filesystem.hsqs" \
  cargo test --locked --manifest-path rust/Cargo.toml \
  --test image_source_parity -- --ignored
```

This opt-in test compares selected file names/bytes and verifies unchanged
source SHA-256; it never executes extracted content or stores fixtures in Git.

### Native import storage seams (partial L16)

`import_storage` is a private native filesystem adapter, not shared frontend
policy or a universal platform storage trait. Profile and skin wrappers retain
their error messages while sharing parent creation, file creation and
`write_all`. Tests can inject partial/zero writes, disk-full and creation errors.
An owned staging guard is constructed only after exclusive directory creation;
it attempts removal of that exact staging path on drop, never the destination.
Cleanup errors remain ignored as before and can leave an orphan staging folder.

Publication retains native rename semantics. Rename success is the observed
publication point; on rename error with an existing destination, the profile
caller reads and validates that destination before returning an existing valid
winner. Invalid destinations/read errors still fail, without deleting old user
data. This preserves existing behavior; it is not a portable atomic-no-replace,
fsync or power-loss durability guarantee. Write/copy/publication failures are
now injectable through the actual importer and skin preparation, with separate
exclusive-stage creation tests. Opt-in local-image matrices cover eight function
error cases and actual process exits immediately before/after rename. A restart
ignores unpublished orphan stages and can import again; it does not automatically
remove those orphans. These are not tests of disk/controller power-loss behavior.

### Controlled import cancellation (L17a–c)

`frontend::import_lifecycle` is a pure, one-shot state machine. Native
`import_job::ImportControl` shares it behind a short-held mutex; no filesystem
work or user callback runs under that lock. Cloned controls can request
cancellation or observe state but cannot start a second import with the same
control. `profile::import_image_cancellable` returns distinct cancellation,
already-started and ordinary-failure errors. Existing desktop callers retain
`import_image` and its previous display errors.

Cancellation is a request until a checkpoint acknowledges it. Checkpoints occur
between hashing, reading, decoding and game-conversion phases, and around
migration operations and manifest publication. Running operations are not
preempted. A serialized `begin_commit` either acknowledges a pending abort or
makes future cancellation too late. `Committing` does not mean publication has
succeeded: rename failure still fails, and successful rename/validated existing
winner completes. No acknowledged abort may publish. Terminal jobs are not reused.

Opt-in tests use `CLOVER_IMPORT_FAULT_IMAGE` with exact test filters
`whole_import_filesystem_fault_matrix`, `whole_import_process_restart_matrix`,
or `whole_import_cancellation_matrix`, followed by `-- --ignored`. Do not run
the child helper directly. Each test uses disposable caches and verifies old
profile/save/settings and source hashes. These controlled APIs alone are not
UI integration; the desktop adapters below now provide it.

### Desktop import adapters

Launcher import progress uses typed job IDs, distinct from picker IDs. Each job
has a private result channel; update polls with `try_recv` and never joins a
thread, even if `is_finished` is true. Ordinary unwinding panics become job
errors. Work completion and final thread/TLS teardown are distinct; dropping an
owner requests cancellation and detaches, not a guaranteed joined shutdown.
Normal launcher close suppresses results and waits for import work to retire.

Launcher Escape/controller East and the existing primary button request
cancellation. Busy remains true until the actual outcome. Requested and
too-late/finishing states are not reported as successful aborts. Neutral input
reseeding prevents held confirm/back buttons crossing import transitions.

The in-menu App uses the same native worker. Its image, ROM and firmware pickers
share one correlated request owner. Import activation captures profile/game
identity; changing context or starting gameplay invalidates automatic switching
and requests cancellation. Dismissing an import notice instead leaves background
work running but permanently releases its activation ticket. Success without an
eligible ticket only refreshes the library, never unloads a running game.

App progress is job-tagged and drains at most 64 messages per frame. Only a
still-owned notice can change its text or expose Cancel import/Continue browsing.
Notice release is sticky even if the same text returns later. Render-only preview
states do not confer live action eligibility. CLI image preparation before the
event loop remains synchronous. This is not the full bounded update/render or
emulator/audio ownership boundary; S06/S07 and platform runtime gates still apply.

## Agent-sized delivery rules

Luna handles mechanical extraction and adapter implementations against an agreed
contract. Sol handles ambiguous contracts, threading/ABI/ownership changes,
coroutine feasibility, persistence guarantees, integration failures and final
verification. A Luna agent must escalate these issues rather than guess.

Each task must name existing symbols, owned files, prerequisites, behavior that
must not change, an executable acceptance test, and a stop condition. One
behavioral seam per task; a new backend is not one task. Characterize behavior
before moving it, wire the extraction into production immediately, and keep
existing callers behind a thin compatibility import if that prevents unrelated
edits. Do not combine cosmetic rewrites, behavior changes and extraction.

Parallel work is permitted only on disjoint files/contracts. Changes to `ui.rs`,
`lib.rs`, Cargo features and shared types are serial integration work unless
ownership is explicitly coordinated. Agents do not commit, push, change upstream
dependencies, install toolchains, or delete local assets without authorization.
SDL remains unmodified; no AI-generated contributions are submitted upstream.

Every task returns its diff, tests actually run, outstanding checks and the next
safe task. A failed test or an unavailable target is not a successful platform
gate. Do not advertise a platform based on a model's review alone.

## Verification ladder

1. Pure shared tests: `cargo test --locked --manifest-path rust/Cargo.toml`.
2. Native integration: formatting, warnings-denied Clippy and all `native-app`
   tests with `CLOVER_NATIVE_BUILD_DIR` set to the built native libraries.
3. Desktop behavior: UI snapshots, controller neutral/repeat/hotplug behavior,
   suspend lock/overwrite/delete/replay, language persistence, audio pacing,
   read-only imports/migrations and relocated package checks. Image-backed checks
   use local assets and disposable save directories; do not commit screenshots.
4. Host failure tests: cancelled/stale operations, storage failure, lifecycle and
   capability changes, unavailable audio, rendering reset and malformed content.
5. Runtime gates per platform below; target compilation alone is insufficient.

## Platform acceptance gates

| Host | Required evidence before claiming a working app |
|---|---|
| macOS / Windows / Linux | Built package; real window/input/audio; generated NES/SNES core tests; import/cache/restart; suspend/save recovery; relocated launch. CI success is not physical controller/audio/display certification. |
| iOS | Simulator app launches; real local-file ingestion; sandbox-safe profiles; native ares links/runs; touch or controller navigation; audio interruption and suspend/resume tests; save/reload across app restart. Device performance, signing and controller tests are separate required evidence. |
| tvOS | Simulator/device shell; remote focus/Back and controller-only navigation; achievable image/firmware ingestion without a file picker; storage eviction/recovery/export strategy; native ares/audio/lifecycle tests; actual device runtime/performance and signing. |
| browser | Actual app in a browser, not a library check; generated NES/SNES frame/audio/state tests; local image ingestion; cancellation/memory limits; persistent saves after reload; quota/export/recovery; user-gesture audio/fullscreen; visibility and context-loss handling; keyboard/gamepad or touch; documented browser/hosting requirements. |

Apple simulator support is not device support. WebAssembly library compilation
is not browser emulation. A web proof should cover the intended browser engines,
including Safari for Apple users, not just headless Chromium.

## Feasibility gates before promising platforms

- **Web core spike (Sol):** validate libco coroutine switching, ares SNES video
  workers, Emscripten threading and C ABI ownership on generated ROMs first.
  Compare a single Emscripten-linked Rust/C++ module with separately hosted WASM
  modules; do not assume `wasm32-unknown-unknown` Rust and Emscripten C++ archives
  link interchangeably. Select a supported toolchain/ABI only after a runnable
  proof. If threads require SharedArrayBuffer, test cross-origin isolation and
  hosting headers; ordinary static hosting may not provide them.
- **Apple core spike (Sol):** build/run the generated-ROM C ABI smoke on Apple
  simulator targets, then devices; validate libco, compiler/architecture choices,
  worker behavior and code-signing constraints. A macOS library cannot be reused
  as an iOS/tvOS library. State formats remain architecture/build-specific.
- **Import spike (Sol contract, Luna adapter):** choose bounded browser file
  reading/temporary storage and an XZ strategy. Current no-default-feature WASM
  checks do not establish support for XZ-compressed NES filesystems. Preserve
  source identity checks even where inputs are immutable browser file handles.
- **Storage/ingestion spike (Sol):** validate iOS security-scoped file access,
  tvOS ingestion and evictable storage, and browser durable transactions/quotas
  before selecting portable service contracts. No server upload is implicit.

Stop a platform track if these gates fail; record the blocker and revise its
adapter/ABI plan. Continue safe desktop extractions rather than silently swapping
emulators, weakening import validation, or claiming unsupported features work.
