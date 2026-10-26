# iOS/tvOS host plan, informed by RetroArch and Provenance

Research date: **2026-10-25**. Status: **initial simulator probes implemented;
complete app support remains planned**.
Repository baseline: `85a4394`. Research inspected upstream source and docs;
neither reference app was built or run during this investigation.

Implementation follow-up: [the Apple probe README](../portable/apple/README.md)
documents two app targets now run on both simulators: generated NES/SNES core
checks and the SDL/Rust shared-scene preview. Both have passing run-tagged
receipts and visible-window checks. The SDL host also has a generated NES mode
with copied video, bounded buffered audio and injected pause/resume tests.
Separate lifecycle runs pass two real simulator OS background/foreground cycles
per platform while preserving the session; only explicit resume is injected.
The host also observes audio-session interruptions and blocks resume until an
end notification followed by explicit user resume. Notification-injection probes
are distinct from OS-issued interruption and physical-device evidence.
The iOS-first follow-up adds a Files-picker raw-NES alpha with coordinated bounded
staging, existing NES validation, content-addressed publication and owner-thread
load/play/stop. Shared sandbox import/playback probes run on both platforms;
tvOS explicitly has no Files picker.
These probes are not complete real-content app support or completion of this plan. Current evidence is in
[the feasibility record](frontend-feasibility.md#app-hosted-runtime-follow-up--2026-10-25).

The 2026-10-26 follow-up adds paired foreground LAN `.hsqs` acquisition on both
platforms and coordinated `.hsqs` Files import on iOS. The initial native screen
selects collections; their actual imported Classic skin/menu runs through a
callback adapter for the existing Rust UI. Game selection and nested returns use
the existing owner-thread game host, not desktop pacing or a second SDL loop.
NES and SNES end-to-end flows pass on both simulators, as do both systems'
desktop/menu pixel-parity and pointer/key checks.
Off-device save recovery, physical devices and complete menu audio/suspend support
remain open. The baseline inventory below is historical, not current availability.

## Recommendation

Build a small **SDL3-backed Apple app host**, retaining Rust application logic
and the existing ares C ABI. Use a thin Objective-C/Objective-C++ layer for Apple
startup, document access and lifecycle services; Swift may provide ancillary
screens later. Do not adopt libretro, embed RetroArch, switch emulator cores or
rewrite the frontend in SwiftUI.

First deliverable: an iOS simulator app that renders a synthetic shared scene
and runs generated NES/SNES test content. Then prove the same core host on tvOS
before spending more time on menu extraction. These are early feasibility
probes, not permission to skip Checkpoint C before claiming a complete port.

This adjusts the earlier generic suggestion of a new native renderer: our pinned
SDL already supports Apple UIKit, Metal rendering, audio and controllers. Start
with that existing dependency, not a second implementation of all those systems.
Keep a direct Metal adapter as a fallback only if a bounded SDL probe exposes a
specific blocker. No SDL modifications or upstream contributions are planned.

## What the reference apps actually do

### RetroArch

Source snapshot: [`684394ad71f40fa4f8a0dd795ce7792ceb5654aa`][ra].

| Concern | Observed implementation | Lesson for this repo |
|---|---|---|
| Application host | `ui/drivers/ui_cocoatouch.m` handles UIKit/scene lifecycle; `ui/drivers/cocoa/cocoa_common.m` integrates `runloop_iterate` using CFRunLoop and display-link paths. | Apple owns application startup/event delivery. Do not invoke our desktop `App::run` loop unchanged. |
| Rendering | `gfx/drivers/metal.m` uses Metal, CAMetalLayer and MetalKit interfaces. | Metal is a proven platform backend; SDL can supply our first implementation. RetroArch's large shader/HDR stack is unnecessary for our quads. |
| Audio | `audio/drivers/coreaudio.c` separates producer and real-time callback with a bounded ring; UIKit code handles AVAudioSession interruptions. | Keep emulation out of the real-time callback; handle actual device rates, underruns and interruptions explicitly. |
| Controllers | `input/drivers_joypad/mfi_joypad.m` maps GameController profiles and observes connection/disconnection; Siri Remote is distinguished from an extended gamepad. | Remote menu navigation and NES/SNES controller input are separate capabilities. |
| iOS ingestion | User docs describe Files visibility and the system chooser; CocoaTouch URL handling uses security-scoped access. | Copy selected data into owned staging; do not retain an external file-provider path as the game library. |
| tvOS ingestion | `pkg/apple/WebServer/WebServer.m` exposes HTTP upload and WebDAV, rooted in tvOS caches. Docs describe browser upload and Finder WebDAV. | A local transfer screen is a practical substitute for a nonexistent tvOS Files workflow. |
| Persistence | `frontend/drivers/platform_darwin.m` chooses Documents on iOS and Caches on tvOS. User docs explicitly warn of tvOS cache eviction, including saves. | Copy the transfer idea, **not** the assumption that a local save file is durable on Apple TV. |
| Packaging | Apple targets package/sign executable cores before installation; App Store builds do not rely on JIT. | Bundle our existing interpreter-based ares build. No runtime emulator-code downloader or debugger/JIT requirement. |

The inspected WebServer wrapper enables broad access to its content root and
does not specify authentication options there. This is **not** a template to
copy verbatim: our service should be opt-in, paired, scoped and short-lived.
Likewise, reference lifecycle behavior is evidence of integration work, not a
reason to duplicate every resume policy.

Primary references: [installation/content transfer][ra-install], [Apple build
and signing guide][ra-build], and the source paths linked in [Source index](#source-index).

### Provenance

Source snapshot: [`dd631b7c2c071eccd3e2f01e87f16bc9807c57f7`][pv].

- `PVMetalViewController.swift` is an Apple-native Metal/MetalKit renderer,
  separate from emulator-core abstractions. It is an alternative host design,
  not evidence that we need to replace SDL.
- `CloudKitSaveStatesSyncer.swift` implements explicit CloudKit save-state
  synchronization, including retries and metadata dependencies. Off-device saves
  are a real subsystem, not a filesystem-path setting.
- `StreamingMultipartUpload.swift` streams uploaded file bodies to disk rather
  than collecting an entire image in memory. This matters for our bounded image
  importer and potentially large source images.
- The project's own README documents native tvOS UI, web-based importing and
  CloudKit synchronization. We do not infer that every advertised ingestion
  method exists on every platform; in particular, no AirDrop/Files-on-tvOS path
  is assumed here.

Use these as architecture references. Do not paste implementation from
RetroArch's GPL code into our MIT frontend. Audit exact file/dependency licenses
before any reuse; the same applies to Provenance and bundled web-server code.

## Research starting point and blockers here (baseline `85a4394`)

Already available:

- Owned launcher/menu/toolbar/generic-panel scenes, bitmap-font metrics, stable
  keys and checked texture-owner generations in `rust/src/frontend/`.
- Working SDL desktop renderer and input/audio adapters, plus retained-resource
  replay tests. They provide behavior references, not an exported Apple host API.
- Thread-affine `AresSession` in `rust/src/ares.rs` and
  `portable/core/ares_c_api.h`: frame words are `0xAARRGGBB`; audio is interleaved
  float stereo. Both buffers are borrowed until the next core operation.
- Separate iOS/tvOS simulator/device core archives and linked smoke executables
  from the earlier [feasibility probes](frontend-feasibility.md).

Not available yet:

- No app target, startup bridge, exported Rust application C API or Apple runtime
  evidence. On this research date, `simctl list runtimes --json` is still empty;
  `rustup target list --installed` reports only `aarch64-apple-darwin`.
  The active toolchain's available-target list includes `aarch64-apple-ios`,
  `aarch64-apple-ios-sim`, `aarch64-apple-tvos` and `aarch64-apple-tvos-sim`;
  they have not been installed. Pin the tested toolchain rather than assuming
  historical tvOS target-support limitations still apply.
- `rust/build.rs` links ares only under `native-app` and its non-macOS/non-Windows
  branch assumes Linux-style libraries. `native-ares` alone does not arrange
  final native linkage. Apple linking must be designed, not enabled by a flag.
- The root CMake file gates AppKit on `APPLE`, which also includes iOS/tvOS.
  Restrict the macOS menu target to macOS, or use an explicitly Apple-mobile
  native-libraries build. Never link a macOS archive into an arm64 simulator app.
- `ui.rs` still owns application state/effects, filesystem assumptions and a
  polling/sleeping event loop. Shared scenes alone do not constitute a complete
  reusable frontend. Distinct toolbar/suspend payloads and target passes remain.
- Current importer publication is not a power-loss durability guarantee. Mobile
  paths and tvOS cache recovery need their own tested storage contracts.

## Host boundary to implement

```text
Xcode iOS target / Xcode tvOS target
  Apple startup + document/network transfer + sandbox paths + lifecycle
  SDL3 (UIKit/Metal/audio/GameController adapters)
          |
  small C-compatible Rust host boundary (opaque owner, explicit lifetimes)
          |
  bounded frontend update / owned scenes / typed effects
          |
  existing importer + profile/storage adapters + thread-affine ares session
```

Use a tiny C entry shim with SDL's supported main callbacks. Do not install a
second competing UIKit application delegate. Coordinate native services with
SDL's supported window/lifecycle interfaces. Callbacks must bound work and
return; no nested polling loop, desktop frame-pacer sleep or unbounded import.
Define bridge errors and ownership explicitly; no Rust panic or C++ exception
may unwind across the C boundary. Avoid serializing whole scenes to JSON per
frame: keep scene execution in Rust with the SDL adapter wherever possible.

For the first core smoke test, create/use/destroy ares on one identified thread.
For interactive emulation, prove either bounded main-thread execution or a
dedicated **OS thread** that owns the entire session. A serial DispatchQueue or
Swift actor alone does not guarantee physical-thread affinity. If a dedicated
thread is needed, create the session there; do not move an existing session.
Copy frame/audio into bounded owned transport before another core operation.
The audio callback consumes ready samples only, with no allocation, I/O or core
calls. Presentation callbacks are not the emulation clock.

## Delivery sequence and executable gates

| Stage | Work | Must demonstrate before advancing |
|---|---|---|
| A: runtime/core | With permission, install simulator runtimes and appropriate Rust targets. Add minimal iOS and tvOS app-hosted generated-ROM tests using the existing C ABI. | Launch in each simulator; finite frames/audio, expected SRAM, save/load deterministic replay; correct platform slices; no JIT. Repeat on physical devices before support claims. |
| B: packaging/host | Add separate Xcode targets, target-aware CMake/Cargo linkage and a versioned minimal C bridge. Prefer built pinned archives; optionally package platform slices as XCFrameworks. | Reproducible simulator install/launch from CLI; no AppKit/Linux linkage; device/simulator arm64 remain distinct. Existing desktop builds unchanged. |
| C: synthetic scenes | SDL callback host renders synthetic launcher/menu/panel scenes with generated textures. Expose only renderer/resource code needed by this caller. | 1280×720 stage, letterboxing, points-to-pixels/touch transform, safe areas, bitmap metrics, alpha/filtering, render-target passes and stale-ID rejection; resize and resource recreation tests. |
| D: playable vertical slice | Wire generated NES/SNES content, bounded update, video/audio transport and input; save/load from owned local paths. | Audio-driven pacing at multiple display rates, no UI stalls/catch-up bursts, disconnect neutralization, interruption/background/resume, memory pressure and repeated session teardown. |
| E: iOS real content | UIDocumentPicker/Files import, bounded staging and existing importer. Add optional ROM/firmware companions and export/restore. | Provider/security-scope lifetime, cancellation, stale picker/import results, invalid image/XZ behavior, insufficient space, restart and interrupted writes; user source unchanged. |
| F: full frontend | Finish only the remaining seams required by the Apple caller: shared state/update/effect routing, toolbar/suspend scenes, target passes and capability-dependent actions. | Real profile launcher → menu → gameplay → suspend → restart with current desktop schema; no unavailable desktop actions, no copyrighted bundled assets. Checkpoint C and desktop regression gates pass. |
| G: tvOS acquisition/recovery | Remote-first empty/transfer/recovery screens, foreground LAN upload, persistent preferences plus explicit off-device save/recovery policy. | Remote-only navigation of all non-gameplay states; real upload/import; denied networking, quota, interrupted transfer and cache eviction recovery. An extended controller may be required for gameplay, not recovery. |
| H: distribution | Signing, icons, release schemes, notices, simulator CI and physical-device testing; optional TestFlight/App Store work. | Signed installable apps; no development-only/JIT dependencies; measured iPhone/iPad/Apple TV performance and restore tests. Store approval remains a separate gate. |

Run A for both platforms early. Build the interactive host primarily on iOS,
then reuse it on tvOS while starting storage/acquisition feasibility in parallel
as research, not as an assumption. Do not wait for every remaining desktop
presentation extraction before A–D; do not advertise a complete app before F–H.

## Storage and acquisition decisions

### iOS

- Obtain source images through the document picker; coordinate external reads
  and security-scoped access, then stage into our sandbox. Retain request/profile
  identities and never treat a picker URL as an everlasting capability.
- Keep durable settings/SRAM/states and their metadata in Application Support;
  expose explicit exports in Documents/Files. Keep reproducible prepared assets
  separate and apply backup-exclusion policy deliberately. Cache loss must not
  delete the only save copy.
- Preserve profile/save identities and state-version checks. Record durable local
  completion separately from pending exports or cloud uploads. Save during normal
  operation, not only termination/background callbacks.

### tvOS

Apple's [archived tvOS guide][apple-storage] and RetroArch's current user docs
describe purgeable local files and only limited persistent preferences. The
archive's numeric limits are historical, not a freshly measured OS quota; do
not design the save system around an assumed exact byte allowance. Treat all
bulk local profiles, SRAM and suspend states as reconstructible/evictable until
the actual target's behavior and recovery are proved.

- Start with an explicit **Transfer files** screen displaying an address/QR and
  short-lived pairing token. HTTP upload/download is sufficient for the first
  prototype; WebDAV interoperability can follow. Use a maintained dependency
  after a license/security review, not an ad hoc HTTP parser.
- Expose only upload staging and selected exports, not the sandbox root. Enforce
  authentication, origin/request protections, path/symlink traversal prevention,
  size/decompression limits, quotas and bounded streaming. A pairing token over
  plain HTTP is not encrypted transport: restrict the prototype to a trusted LAN
  and evaluate TLS/pairing before broader deployment.
- Run the server only while explicitly enabled in the foreground. Stop on
  background/exit; report local-network permission denial. Upload completion is
  not profile publication or save-backup completion.
- For an internal prototype, label local saves **cached / not backed up** and
  provide export/restore immediately. For supported use, add acknowledged
  off-device durability: e.g. private CloudKit assets, or a paired companion
  which confirms a verified durable write. CloudKit is a separate entitlement,
  account, quota/conflict and offline-retry project—not required to show a frame.
- Keep original images/firmware off-device for cache reconstruction. Test deleting
  local prepared assets and saves between launches, then restoring by stable
  profile/game identity. Never interpret a missing cached state as permission
  to overwrite the off-device copy with an empty/default state.

## First ten implementation slices

Execute directly with Sol unless redirected. Each slice has a real caller/test;
none authorizes installing tools, changing upstream or configuring signing
accounts without permission.

1. Inventory target tooling and define app-hosted NES/SNES smoke result format;
   record runtime/target installation prerequisites and user approval needed.
2. Add a minimal iOS simulator app target which invokes the generated NES test
   and emits a machine-readable result on the creating thread.
3. Add SNES/frame/audio/state-replay checks to that host; verify target slices.
4. Add the separate tvOS simulator target and run the same core checks there.
5. Split Apple-mobile native linkage from desktop AppKit/Linux assumptions;
   keep pinned dependencies unchanged and desktop CI green.
6. Add the minimal C-to-Rust SDL callback bridge with bounded init/event/tick/
   shutdown and explicit ownership/error handling.
7. Render one synthetic shared scene through the existing SDL command adapter;
   test target rendering, letterboxing and input coordinate conversion.
8. Add lifecycle pause/neutral-input transitions and audio interruption handling;
   test suspend/resume without implicit game restart or a catch-up burst.
9. Add generated-ROM video/audio pacing in the Apple host, choosing and testing
   session-thread ownership and bounded copied buffers explicitly.
10. Run simulator end-to-end tests on both targets and document physical-device
    follow-ups; scope the next batch for iOS ingestion and tvOS recovery probes.

## Source index

- [RetroArch CocoaTouch lifecycle][ra-ui], [run-loop integration][ra-loop],
  [Metal driver][ra-metal], [CoreAudio driver][ra-audio], [GameController driver][ra-input].
- [RetroArch Darwin paths][ra-paths], [HTTP/WebDAV wrapper][ra-web],
  [Apple installation guide][ra-install], [Apple compilation guide][ra-build].
- [Provenance Metal view][pv-metal], [CloudKit states][pv-cloud],
  [streaming upload][pv-upload], [project overview][pv-readme].
- Our pinned SDL guidance: `third_party/SDL/docs/README-ios.md` and
  `third_party/SDL/docs/README-main-functions.md`; [online SDL Apple guide][sdl-ios]
  and [callback guide][sdl-callbacks]. Read as dependencies' documentation only;
  do not modify SDL or submit generated contributions.
- [Apple tvOS storage guide (archived)][apple-storage]. Recheck current SDK/API
  availability, store rules and device behavior during implementation; historical
  hardware and quota descriptions are not current certification.

[ra]: https://github.com/libretro/RetroArch/tree/684394ad71f40fa4f8a0dd795ce7792ceb5654aa
[ra-ui]: https://github.com/libretro/RetroArch/blob/684394ad71f40fa4f8a0dd795ce7792ceb5654aa/ui/drivers/ui_cocoatouch.m
[ra-loop]: https://github.com/libretro/RetroArch/blob/684394ad71f40fa4f8a0dd795ce7792ceb5654aa/ui/drivers/cocoa/cocoa_common.m
[ra-metal]: https://github.com/libretro/RetroArch/blob/684394ad71f40fa4f8a0dd795ce7792ceb5654aa/gfx/drivers/metal.m
[ra-audio]: https://github.com/libretro/RetroArch/blob/684394ad71f40fa4f8a0dd795ce7792ceb5654aa/audio/drivers/coreaudio.c
[ra-input]: https://github.com/libretro/RetroArch/blob/684394ad71f40fa4f8a0dd795ce7792ceb5654aa/input/drivers_joypad/mfi_joypad.m
[ra-paths]: https://github.com/libretro/RetroArch/blob/684394ad71f40fa4f8a0dd795ce7792ceb5654aa/frontend/drivers/platform_darwin.m
[ra-web]: https://github.com/libretro/RetroArch/blob/684394ad71f40fa4f8a0dd795ce7792ceb5654aa/pkg/apple/WebServer/WebServer.m
[ra-install]: https://docs.libretro.com/guides/install-ios/
[ra-build]: https://docs.libretro.com/development/retroarch/compilation/ios/
[pv]: https://github.com/Provenance-Emu/Provenance/tree/dd631b7c2c071eccd3e2f01e87f16bc9807c57f7
[pv-metal]: https://github.com/Provenance-Emu/Provenance/blob/dd631b7c2c071eccd3e2f01e87f16bc9807c57f7/PVUI/Sources/PVUIBase/PVGLViewController/PVMetalViewController.swift
[pv-cloud]: https://github.com/Provenance-Emu/Provenance/blob/dd631b7c2c071eccd3e2f01e87f16bc9807c57f7/PVLibrary/Sources/PVLibrary/Cloud%20Sync/iCloud/CloudKit/CloudKitSaveStatesSyncer.swift
[pv-upload]: https://github.com/Provenance-Emu/Provenance/blob/dd631b7c2c071eccd3e2f01e87f16bc9807c57f7/PVWebServer/Sources/PVWebServerSwift/StreamingMultipartUpload.swift
[pv-readme]: https://github.com/Provenance-Emu/Provenance/blob/dd631b7c2c071eccd3e2f01e87f16bc9807c57f7/README.md
[sdl-ios]: https://wiki.libsdl.org/SDL3/README-ios
[sdl-callbacks]: https://wiki.libsdl.org/SDL3/README-main-functions
[apple-storage]: https://developer.apple.com/library/archive/documentation/General/Conceptual/AppleTV_PG/
