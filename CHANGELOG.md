# Changelog

## Unreleased

This is an experimental development snapshot, not a production or App Store
release. Platform claims and remaining gates are tracked in
[platform status](docs/platform-builds.md).

### Apple previews

- Add an SDL callback/UIKit host with owner-thread NES/SNES emulation, bounded
  audio/video, input cleanup and explicit lifecycle recovery.
- Reuse the real Rust Classic renderer and imported regional assets without
  starting a second event loop or creating a core in the menu.
- Add native Files-first `.hsqs` import on iOS; no LAN listener or pairing UI.
- Add paired foreground browser transfer on tvOS, with expiring six-digit PINs,
  separate session credentials, bounded uploads and a responsive transfer page.
- Support multiple imported collections, stable focus/return selection and
  ordered rapid-key navigation. Preserve existing profiles during failed imports.
- Add simulator probes for generated core behavior, touch/input, lifecycle,
  imports, menu/game returns and multi-collection navigation.

### Repository and packaging

- Document current platform behavior, build steps, content policy and security
  boundaries; add contribution and issue-reporting guidance.
- Add source/index/history content checks and tests, plus expanded CI for the
  browser, portable Rust and Apple compile/link paths. Pin external workflow
  actions to verified commit IDs with read-only repository permissions.
- Collect full Rust/font/flag/native dependency notices in Apple bundles as well
  as desktop packages. Fail notice collection when a dependency has no notice.
- Apply consistent Apple source formatting without modifying upstream SDL/ares.
- Run shared Apple input/touch and callable core-entry checks in normal native
  CTest builds, including desktop CI.

### Known limitations

- Physical-device signing/install, actual Siri Remote/controller/touch delivery,
  external Files-provider hydration and audible audio quality remain unverified.
- Apple menu audio, suspend slots and complete SNES touch controls are unfinished.
- tvOS profiles and saves are evictable caches; off-device backup/recovery and
  kill/power-loss durability are not implemented.
- Windows/Linux runtime validation and a browser emulator app remain open.
