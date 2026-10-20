# Image-specific regional menus

The 0.5.0 importer recognizes all six supplied filesystems, not just the European
ones. Region is detected from the UI root inside the image, not its filename:

| UI root | Region / default table | Supplied releases |
| --- | --- | --- |
| `snes-eur` | Europe / `usa_en` | 2.0.7, 2.0.14 |
| `snes-usa` | USA / `usa_en` | 2.0.7, 2.0.13, 2.0.14 |
| `shvc` | Japan / `jpn_ja` | 2.0.12 |

Full/trimmed SquashFS and gzip/tar carriers remain readable. Boot images still
require a matching local filesystem. Archive equivalence tests are separate
from the regional GUI and game/audio tests.

## What changes when selecting an image

- Console bars/logos/controller-button colors and other sprites come from that
  image's `sprites/packed.png`; no European atlas is reused for USA or Japan.
- Source wallpaper, audio files, prefab layout, game catalog and box art are
  imported together. Japanese box art is portrait (e.g. 114×204), while European
  and US boxes are landscape (228×160). Both preserve their actual proportions.
- Source fonts are decoded from Reed containers. Japanese `s/m/l` resources use
  `.fnt`, not `.font`; the payload format is nevertheless the same. The source
  `copyright.fnt` supplies additional fallback glyphs for custom English controls.
- Titles and player metadata come from the selected catalog. Japanese titles
  stay Japanese; the Japanese catalog includes different games.
- Main HUD prompts, toolbar captions, sort labels, Back labels and original
  panel headings resolve through the image's actual JSON `strings.lng` table.
- Japanese title text is white and HUD text black, unlike the black title and
  white HUD in European/US profiles. These colors are from statically decoded,
  hash-identified environment tables, not inferred from a country label.
- The active region/release remains visible in the window title and image
  library. Native macOS menus and custom emulator/import controls remain English;
  this is not a full port of the original engine's language-selection feature.

The shipped scenes/prefabs are common across these inspected releases. Sharing
renderer geometry does not substitute their regional artwork, strings, or fonts.
No extracted Lua, shell, or ARM/Linux program is executed by the importer.

## Palette observations

Direct static LuaJIT decoding used the existing local `decode_lj.py`; results
and input bytes/hashes are in `analysis/region-investigation/`. These are ordinary
file-derived observations, **not REA Evidence or original-console runtime captures**.

| Table | SHA-256 |
| --- | --- |
| `colors_eur.lua` | `cd85ce84d70c31f83e2a17739e3869eae7790e973851fd4e8c03977ff609073a` |
| `colors_usa.lua` | `1181691260ecb050374b10676c4d4c657dafbd143e886385853607949dd4951d` |
| `colors_jpn.lua` | `8a53966a583c45c115fe4d55aff76a56bb63b032512d1570f3879ba09c24a914` |

The importer checks these known palette identities before applying the recovered
title/HUD colors. Unknown palette revisions fail instead of silently using a
different region's style. All six supplied releases passed this check.

## Cache upgrades and saves

Importer revision 3 creates `<image-sha256>-v3` profiles alongside older ones.
It does not overwrite or delete the older cache. Existing settings/mappings and
save data are copied into the new profile; the old default firmware path is
updated, but explicitly selected external firmware folders are preserved.
Migration rejects symlinks, more than 20,000 entries, or more than 1 GiB of data.

The launcher can upgrade its last-used v1/v2 profile automatically when its
original source still exists and matches the recorded digest. Otherwise select
the source again in Library → Image Library. Restored ROMs have distinct ROM
digests; states from a different ROM build remain incompatible by design.

Imported SMP IPL now stays separate from the user-selected coprocessor firmware
folder. Choosing such a folder no longer requires copying `ipl.rom` into it.
The original three-argument core API still accepts one combined firmware folder;
the frontend uses the new four-argument overload.

## Verification

`scripts/test_rust_importer.py` validates all six SNES profiles, source
immutability, cached-profile reuse, language tables, and profile migration.
`scripts/test-rust-ui.py --assets PATH` checks rendered menu/language snapshots;
those snapshots are not reference screenshots of the original console.

The former C++-only regional importer/render tests have been retired after the
Rust cutover. Historical image/game audit observations are retained in
`analysis/` when available; see [audio audit](game-audio-audit.md) for its
coverage and limitations. The ares game audit remains separate from importer
and GUI validation.
