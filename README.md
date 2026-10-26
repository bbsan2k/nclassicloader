# Classic Menu

[![Build and test](https://github.com/bbsan2k/nclassicloader/actions/workflows/portable.yml/badge.svg?branch=main)](https://github.com/bbsan2k/nclassicloader/actions/workflows/portable.yml)
[![Rust](https://img.shields.io/badge/frontend-Rust-dea584)](rust/)
[![License: MIT](https://img.shields.io/badge/license-MIT-green)](LICENSE)

A Rust reconstruction of the Classic console menu, with a read-only image
importer and NES/SNES emulation powered by [ares](https://github.com/ares-emulator/ares).
Import your own Classic filesystem image, open its regional menu, and play from
a validated local profile. SDL3 provides rendering, audio and controller support.

**Experimental and unofficial.** This is not Nintendo's original engine, an
exact behavior/pixel-parity claim, or a Nintendo-affiliated product. No console
images, commercial ROMs, firmware or extracted console assets are included or
downloaded. Supply only content you are entitled to use.

## Platform status

| Platform | Current status |
|---|---|
| macOS | Desktop app builds and passes local smoke/regression tests. Ad-hoc signed, not notarized. |
| Windows / Linux | Desktop build/test/package CI configured. Runtime behavior still needs platform testing. |
| iOS | Working simulator preview: direct Files import, Classic menus and NES/SNES gameplay. Physical devices/signing remain open. |
| tvOS | Working simulator preview: paired browser import, collection focus, Classic menus and NES/SNES gameplay. Physical remote/device validation remains open. |
| WebAssembly | Portable Rust library check only; no browser emulator app. |

Apple simulator previews are development builds, not App Store releases.
See [platform status and limitations](docs/platform-builds.md) for the distinction
between CI compile checks, injected simulator tests and hardware evidence.

## Desktop quick start

Prerequisites: Rust stable, CMake 3.28+, Python 3, Git, Ninja and a C/C++ toolchain.
macOS needs Xcode command-line tools; Windows uses MSVC. Linux graphics/audio
development packages are listed in the [build guide](docs/portable-build.md).

```sh
python3 scripts/build-rust-desktop.py
```

The first build fetches pinned ares/SDL sources and Cargo dependencies. It does
not install system software or fetch console content.

On macOS:

```sh
open "dist/Classic Menu.app"
```

Windows/Linux packages are under `dist/rust-desktop/windows/` and
`dist/rust-desktop/linux/`; desktop archives are under `dist/rust-desktop/`.
Default packages contain application code and dependency notices, not game data.

### Import and play

1. Choose your local **`.hsqs` or `.tar.gz` filesystem** in the desktop image library.
2. Let the importer validate and prepare its profile without modifying the source.
3. Open the collection's Classic menu and select a supported game.

A boot `.img` can locate a matching local filesystem companion; a boot image
alone contains no games. Later launches can use the prepared cache while the
source is offline. Press **F2** or use **Settings → Image Library** to switch or
import another image. Settings and saves are separate from imported assets.

The importer supports documented SNES Classic Europe/USA/Japan revisions and
NES/Famicom Classic filesystem variants. It validates supported iNES boards and
converts supported SNES game images, including signal-exact BRR restoration.
This is not byte-identical retail ROM recovery or a guarantee of game compatibility.
See [image importing](docs/image-import.md), [supported regions](docs/regions.md)
and [conversion limits](docs/sfrom-import.md).

External coprocessor firmware is never downloaded or guessed. Famicom Disk
System conversion, BIOS integration and disk controls are not implemented.

## iOS and tvOS previews

Prerequisites: Xcode with the selected simulator SDK/runtime, CMake, Python 3,
Rust stable and the relevant Rust simulator targets. Runtime downloads, physical
device provisioning and signing-account changes are intentionally not automated.

```sh
python3 scripts/bootstrap-portable.py
rustup target add aarch64-apple-ios-sim aarch64-apple-tvos-sim

# iOS: import a .hsqs image directly through Files.
python3 scripts/test-apple-app.py --platform ios --app preview --interactive

# tvOS: upload a .hsqs image from a browser on the same trusted network.
python3 scripts/test-apple-app.py --platform tvos --app preview --interactive
```

The runner opens Device Hub on newer Xcode versions, or Simulator on older
versions, and retains only the new simulator it creates. Existing simulator
libraries are not reused or reset. Use `--build-only` for compile/link checks
without launching a simulator.

Both previews follow **collections → imported Classic menu → game → Classic →
collections**. iOS uses a native Files-first screen, with no network listener or
pairing UI. tvOS shows a local upload address and six-digit PIN; the browser page
exchanges that PIN for a separate, short-lived session credential.

**Use trusted Wi-Fi. tvOS transfer uses unencrypted HTTP.** Keep Clover
foregrounded on the TV while importing from a different device. Never port-forward
the listener. Transfers are disabled inside Classic and during gameplay.

Keep your source images and independent save backups. **tvOS caches can be
evicted, including saves.** Off-device recovery, kill/power-loss durability,
complete SNES touch controls, menu audio and suspend slots remain unfinished.
See the [Apple guide](portable/apple/README.md) and [security notes](SECURITY.md).

## Controls

- Desktop: keyboard and SDL gamepads, with hotplug and one active controller.
- Menu: arrows to navigate, **Return** to confirm, **Escape** to go back.
- Desktop: **F11** toggles fullscreen; macOS also supports **Control–Command–F**.
- Desktop gameplay: **Escape/Guide** opens the suspend picker. Its ares states
  are not compatible with Nintendo's original save states.
- iOS gameplay: D-pad, A/B, Start and Select touch overlay. A controller is
  recommended for SNES games requiring additional buttons.
- Apple previews: stopping gameplay returns to Classic; root Back returns to
  collections. iOS also provides a **Classic menu / Collections** button.

See [controller mappings](docs/controller-support.md) for details. Physical
display, audio, remote/controller and finger delivery still need hardware testing.

## Development and validation

Portable checks require no console images or native libraries:

```sh
python3 scripts/check-repository.py
python3 scripts/test-repository.py
python3 scripts/test-rust-licenses.py
python3 scripts/test-apple-app-unit.py
node scripts/test-apple-transfer-page.js
cargo fmt --manifest-path rust/Cargo.toml -- --check
cargo test --locked --manifest-path rust/Cargo.toml
cargo test --locked --manifest-path rust/Cargo.toml --no-default-features
```

After a desktop build, POSIX-shell native checks include:

```sh
ctest --test-dir .build/native --output-on-failure
CLOVER_NATIVE_BUILD_DIR="$PWD/.build/native" cargo test --locked \
  --manifest-path rust/Cargo.toml --all-targets --features native-app
CLOVER_NATIVE_BUILD_DIR="$PWD/.build/native" cargo clippy --locked \
  --manifest-path rust/Cargo.toml --all-targets --features native-app -- -D warnings
```

CI checks repository content/history, portable Rust, browser state, desktop
builds and Apple compile/link targets. Local Apple tests also cover generated-ROM
probes, lifecycle, touch/input, Files/browser imports, multi-collection navigation
and real menu/game returns. Image-backed tests require privately supplied inputs;
they are not public CI payloads. Build success and injected input are not evidence
of physical-device support. See [contributing](CONTRIBUTING.md) for the full workflow.

## Repository layout

| Path | Purpose |
|---|---|
| [`rust/`](rust/) | Frontend, importer, shared policies, ares wrapper and tests |
| [`portable/core/`](portable/core/) | C++ ares bridge and self-authored generated-ROM checks |
| [`portable/apple/`](portable/apple/) | UIKit/SDL callback host, Files access, transfer UI and simulator guide |
| [`portable/platform/`](portable/platform/) | macOS menu-bar adapter |
| [`scripts/`](scripts/) | Bootstrap, builds, packaging, audits and test runners |
| [`docs/`](docs/) | User guides, architecture, research and validation limits |

ares and SDL are not Git submodules. Ignored checkouts under `third_party/` are
fetched at the revisions in [`dependencies.lock.json`](dependencies.lock.json)
and must remain unmodified. Rust dependencies are locked in `rust/Cargo.lock`.
`.build/`, `dist/`, local images, imported profiles, captures and saves must stay
out of Git. See the [repository-content policy](docs/repository-content.md).

## Provenance and licensing

This reconstruction was developed with AI coding assistance and static
reverse-engineering evidence, including REA (Reverse Engineer Anything), native
binary inspection, resource analysis and LuaJIT bytecode inspection. REA is not
a build or runtime dependency. The [investigation ledger](docs/reverse-engineering.md)
distinguishes observations, inferences and remaining unknowns.

The [MIT license](LICENSE) covers original project contributions only. ares,
SDL, Rust dependencies and redistributable fonts retain their own licenses; see
[third-party notices](THIRD-PARTY-NOTICES.md). No license here grants rights to
Nintendo content or trademarks. Do not distribute a locally asset-enabled build
without rights to every included asset.
