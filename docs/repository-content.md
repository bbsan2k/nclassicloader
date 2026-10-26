# Repository content policy

This repository tracks the Rust frontend/importer, the ares C++ bridge and
execution core, a small Objective-C++ macOS menu adapter, tests, and build
support for SDL3/ares, plus the Apple UIKit/SDL host and content-free browser
transfer interface.

Do not commit console kernel/filesystem images, commercial ROMs, firmware,
extracted console programs, artwork, console fonts, language tables, audio,
generated profiles, save data, screenshots, build outputs, or app bundles.
Supply any required console content locally; the importer reads it at runtime.
Ignore rules are a safeguard, not a substitute for reviewing staged files.

The launcher includes Barlow Regular/SemiBold TTF source resources under the
SIL Open Font License, with the license and attribution in `rust/assets/fonts/`.
These redistributable fonts are not imported Nintendo assets or app binaries.
`LICENSE` applies to original project contributions only; it does not relicense
third-party code or console-derived assets. `portable/core/LICENSE.ares` covers
upstream-derived bridge code, and the SDL/component notices are indexed in
`THIRD-PARTY-NOTICES.md`. Preserve the notices when distributing source or
applications.

Large upstream ares and SDL3 source checkouts are excluded. Acquire their
pinned revisions with
`python3 scripts/bootstrap-portable.py`, using `dependencies.lock.json`.
Rust dependencies are pinned by `rust/Cargo.lock`.

See `docs/rust-migration.md` for Rust build and validation instructions.

The app icon is an original procedural design in `rust/assets/icon/design.json`.
Japan, EU, and US flags are sourced SVG assets under the MIT license in
`rust/assets/flags/`; their attribution and notice are retained for distribution.
Controller illustrations are authored drawing code. Generated ICNS/PNG and
platform archives stay in ignored build/distribution directories, not Git.

## Before publishing

```sh
python3 scripts/test-repository.py
python3 scripts/check-repository.py --history

# After deliberately staging the reviewed source changes:
python3 scripts/check-repository.py --staged
git diff --cached --check
git diff --cached --stat
```

The checker inspects tracked files plus nonignored source candidates, or the
actual Git index with `--staged`. `--history` also examines blobs reachable from
local refs: deleting a file from the working tree does not remove it from Git
history. It checks disallowed payload paths/extensions, unexpected binaries,
oversized source files, selected credential patterns, conflict markers,
unpinned external workflow actions and relative Markdown links. A shallow Git
checkout is rejected when a history audit is requested. It never reads ignored input images/build trees or
rewrites history, and never prints a matched credential value.

This is a limited safeguard, not a complete secret scan, copyright review or
publication authorization. Inspect staged changes yourself. If a secret or
restricted asset was previously committed, stop publication, rotate affected
credentials and agree on a history-cleanup plan before changing any refs.
