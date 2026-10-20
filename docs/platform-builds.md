# Platform build status

| Platform | CI coverage | App status |
|---|---|---|
| macOS | Native ares/SDL build and tests; Rust release bundle/package | Locally built and smoke-tested; ad-hoc signed, not notarized |
| Windows | Native ares/SDL build and tests; Rust release executable/package using MSVC | CI configured; runtime behavior and installer not verified |
| Linux | Native ares/SDL build and tests; Rust release executable/package | CI configured; runtime behavior not verified and host libraries may be needed |
| iOS | Rust portable-library target check only | No app: SDL frontend, ares integration, lifecycle, and signing are not ported |
| tvOS | Rust portable-library target check only | No app; same native/frontend blockers as iOS |
| WebAssembly | Rust portable-library target check only | No browser app: SDL/ares app, browser storage, and web packaging are not ported |

The iOS/tvOS/Web checks use `--no-default-features`; they validate only the
portable Rust library subset and publish no application artifacts. Desktop
builds include XZ, SDL, and native ares integration.

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
