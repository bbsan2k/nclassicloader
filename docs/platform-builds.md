# Platform build status

| Platform | CI coverage | App status |
|---|---|---|
| macOS | Native ares/SDL build and tests; Rust release bundle/package | Locally built and smoke-tested; ad-hoc signed, not notarized |
| Windows | Native ares/SDL build and tests; Rust release executable/package using MSVC | CI configured; runtime behavior and installer not verified |
| Linux | Native ares/SDL build and tests; Rust release executable/package | CI configured; runtime behavior not verified and host libraries may be needed |
| iOS | Portable Rust, core simulator/device compile/link and full simulator-app build jobs configured; macOS host-side Apple Rust tests | Files-first native collection screen → actual Rust Classic skin/menu → NES/SNES gameplay → Classic → collections; coordinated `.hsqs` import, no LAN/pairing UI or listener. Complete SNES touch, menu audio/suspend slots, providers, signing and physical devices remain open |
| tvOS | Portable Rust, core simulator/device compile/link and full simulator-app build jobs configured; macOS host-side Apple Rust tests | Collection/import screen → actual Rust Classic skin/menu → NES/SNES gameplay → Classic → collections; paired LAN `.hsqs`, no Files picker/touch UI. Off-device save recovery, menu audio/suspend slots, remote validation and physical devices remain open |
| WebAssembly | Rust portable-library target check only | No browser app: SDL/ares app, browser storage, and web packaging are not ported |

The iOS/tvOS/Web Rust-library checks use `--no-default-features`; they validate
only the portable Rust library subset and publish no application artifacts.
Separately, local iOS/tvOS simulator apps pass all nine modes: `touch`, `library`,
`import`, `input`, `lifecycle`, `interruptions`, `game`, `preview` and `core`.
Desktop builds include XZ, SDL, and native ares integration. The separate Apple
simulator/device core jobs compile/link C++ smoke executables only; they do not
run those executables or publish installable application packages. Separately,
local [app-hosted simulator probes](../portable/apple/README.md) now build and run;
the full simulator app now has CI compile/link jobs as well. These do not install
simulator runtimes, run the apps or produce release app packages. The latest CI
configuration still needs a successful run on GitHub; local builds are not proof
that a hosted runner has passed.

The Apple probes also cover injected audio-session notifications and source-aware
input using SDL virtual gamepads and synthetic keyboard/touch/focus events on
both simulators. This is not physical-controller, Siri Remote, real multitouch,
audio-route or audible-device certification. See the probe README for the exact
test modes and remaining gates.

Portability work is tracked in the [architecture decision](frontend-architecture.md)
and [incremental task queue](frontend-tasks.md). Shared controller policy now runs
without SDL, but this does not change the app-support status in the table above.
Physical-device Apple and browser-app runtime gates remain open.
The [native Apple compilation probe](frontend-feasibility.md) now links generated
NES/SNES smoke executables for simulator/device targets using build-tree
compatibility adaptations, without modifying upstream. Physical-device
execution and complete app-level gates remain open;
the newly configured CI compile/link jobs are not runtime tests.

Desktop packages contain code and license notices only. Nintendo firmware, ROMs,
system images, and other game assets are not included. The complete importer,
frontend, settings, and profile logic are Rust. The ares execution cores remain
C++, with a small Objective-C++ adapter retained for AppKit's macOS menu bar.

Run `python3 scripts/build-rust-desktop.py` on the target desktop OS. It builds
the pinned native dependencies, runs native smoke tests, compiles/packages the
Rust app, and collects native/Rust/font/flag notices. Archives are under
`dist/rust-desktop/`; the macOS app bundle is `dist/Classic Menu.app`. The
workflow is `.github/workflows/portable.yml` and runs on pushes, pull requests,
and manual dispatch. Windows/Linux CI settings are configured but still need
their first successful run and runtime testing.
