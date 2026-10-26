# Publication checklist

Prepare a source snapshot without bundling local console content. This checklist
does not itself authorize a commit, push, tag, release or history rewrite.

## Review the source

1. Read [platform status](platform-builds.md), [security](../SECURITY.md) and the
   [content policy](repository-content.md). Keep unsupported features explicit.
2. Review `git status --short`, including new Apple host files and test scripts.
3. Run `python3 scripts/check-repository.py --history`. Resolve every finding;
   do not rewrite history or force-push without an agreed recovery plan.
4. Run the checks in [CONTRIBUTING.md](../CONTRIBUTING.md). For Apple changes, build
   both simulator apps and run relevant generated-content probes. If using
   private images for additional checks, keep their sources and results ignored.
5. Inspect package dependency notices. Desktop and Apple bundles must include
   native notices, Rust package notices, Barlow OFL and flag attribution.

## Review the index

Stage only the changes you intend to publish, then inspect the actual index:

```sh
python3 scripts/check-repository.py --staged
git diff --cached --check
git diff --cached --stat
git diff --cached
```

Ignored files can still be force-added. Never stage console images, ROMs,
firmware, extracted assets, captures, profiles, saves, credentials or build
outputs. The content checker is only a heuristic; manual review is required.

## Before a public release

- Confirm the intended repository visibility and branch. Review the remote
  destination before pushing; do not publish automatically from local scripts.
- Obtain a successful GitHub Actions run. Locally passing tests do not prove
  the new workflow works on hosted Windows/Linux/macOS runners.
- Keep simulator builds labeled experimental. They do not establish device
  signing, App Store readiness, hardware compatibility or save durability.
- Only distribute code-only packages with complete notices and a clear list of
  limitations. Do not attach image-backed test artifacts or local captures.
- Consider enabling GitHub private vulnerability reporting and branch protection.
  These are repository settings and must be configured deliberately by an owner.
- Write release notes from [CHANGELOG.md](../CHANGELOG.md); do not invent a release
  number, hardware result or compatibility guarantee.

## Local preparation evidence — 2026-10-26

These are local checks of the development snapshot, not release certification:

| Check | Result |
|---|---|
| Prospective source and reachable Git history | 239 files and 302 historical blobs checked; no content-policy findings |
| Content-checker / notice-collector / Apple-runner tests | 10 / 3 / 26 tests pass |
| Rust default / no-default-features / native+Apple unit tests | 178 / 177 / 313 pass; integration suites also pass |
| Native CTest | Six targets pass: menu, NES/SNES core, callable entry, shared input and touch |
| Rust/Apple formatting and warnings-denied Rust Clippy | Pass |
| Browser transfer state tests | Pass: pairing, revocation, validation, progress, cancellation and reconnect |
| macOS desktop package | Builds, passes relocation/ad-hoc-signature/system-library checks |
| iOS and tvOS two-collection flows | Pass: import, correct menu identity, navigation, gameplay and return/restart |
| Apple bundle notices | 74 Rust package notice sets, native notices and font/flag attribution verified in each app |
| Workflow configuration | YAML parses; external actions pinned to verified commit IDs; hosted run still required |

Detailed machine-local receipts are ignored under `.build/publication-readiness/`.
They are deliberately not part of the source publication. The checks use owned
test simulators and do not reset existing simulator libraries. Physical devices,
Windows/Linux runtime behavior and App Store distribution remain unverified.
