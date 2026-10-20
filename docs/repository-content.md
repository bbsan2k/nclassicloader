# Repository content policy

This repository tracks the Rust frontend/importer, its tests and build scripts,
and shared native source/build support needed for the SDL3/ares integration.
The legacy Swift app is not part of this initial source snapshot.

Do not commit console kernel/filesystem images, commercial ROMs, firmware,
extracted console programs, artwork, console fonts, language tables, audio,
generated profiles, save data, screenshots, build outputs, or app bundles.
Supply any required console content locally; the importer reads it at runtime.
Ignore rules are a safeguard, not a substitute for reviewing staged files.

The launcher includes Barlow Regular/SemiBold TTF source resources under the
SIL Open Font License, with the license and attribution in `rust/assets/fonts/`.
These redistributable fonts are not imported Nintendo assets or app binaries.
The vendored JSON/stb headers retain their accompanying MIT/public-domain
license notices. `portable/core/LICENSE.ares` covers upstream-derived bridge
code. Preserve these notices when distributing source or applications.

Large upstream ares, SDL3, and zlib source checkouts are excluded. Acquire their
pinned revisions (and verify vendored headers) with
`python3 scripts/bootstrap-portable.py`, using `dependencies.lock.json`.
Rust dependencies are pinned by `rust/Cargo.lock`.

See `docs/rust-migration.md` for Rust build and validation instructions.
