# Contributing

Thanks for helping improve Classic Menu. This is an experimental, unofficial
project; please keep platform claims tied to reproducible evidence.

## Ground rules

- Do not upload console images, ROMs, firmware, extracted assets, profiles, saves
  or screenshots containing console content. Use self-authored fixtures in tests.
- Keep ares and SDL pinned and unmodified. Changes belong in this project's
  adapters, not the ignored upstream checkouts. Respect each upstream project's
  contribution policies; do not submit generated work to SDL.
- Preserve desktop behavior, owner-thread core access and bounded import/audio
  work. Do not start the desktop event loop inside an Apple callback.
- Describe AI assistance honestly. Review generated code like any other change;
  passing tests do not establish hardware compatibility or legal clearance.

## Development

See the [README](README.md) for desktop builds and the
[Apple guide](portable/apple/README.md) for simulator builds. Dependencies are
locked in `dependencies.lock.json` and `rust/Cargo.lock`.

Fast checks, without console content or native dependencies:

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

After building native dependencies:

```sh
ctest --test-dir .build/native --output-on-failure
CLOVER_NATIVE_BUILD_DIR="$PWD/.build/native" cargo test --locked \
  --manifest-path rust/Cargo.toml --all-targets --features native-app,apple-game
CLOVER_NATIVE_BUILD_DIR="$PWD/.build/native" cargo clippy --locked \
  --manifest-path rust/Cargo.toml --all-targets --features native-app,apple-game -- -D warnings
```

The host-side Apple Rust tests require macOS. On Windows/Linux, use `native-app`
instead of `native-app,apple-game`. Apple simulator/device builds need Xcode;
simulator runtime installation and signing-account setup are intentionally not
automated. Image-backed tests are optional local checks, never public CI inputs.

Format Rust with `cargo fmt`. Format Apple Objective-C++ with `clang-format`,
using `portable/apple/.clang-format`. Keep scripts dependency-light and add tests
for behavior changes.

## Pull requests

Explain the problem, scope, platform(s), tests run and remaining limitations.
Separate compile/link checks, injected simulator input and physical-device tests.
Avoid unrelated refactors and generated artifacts. Run the repository-content
check again after staging; see [the content policy](docs/repository-content.md).
Never include pairing codes, session credentials or private paths in reports.

For security-sensitive reports, follow [SECURITY.md](SECURITY.md) rather than
posting exploit details or private content in a public issue.
