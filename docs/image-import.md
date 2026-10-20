# Image-driven launcher and importer

The Rust application opens the **Game Data Library** at startup. Choose a
verified cached profile, a nearby image, **Browse image**, or a prepared profile
folder. Imports run on a worker thread and show progress/errors in the launcher.
The same library can be reopened with F2 or from Settings. Images are never
modified, extracted programs/scripts are never executed, and network access is
not used to fetch game data.

## Supported images and imported data

The importer supports the six supplied SNES Classic filesystems (Europe 2.0.7
and 2.0.14, USA 2.0.7/2.0.13/2.0.14, Japan 2.0.12), NES Classic 1.0.2/1.0.3/
1.0.7, and Famicom Classic 1.0.5. Select `.hsqs` or `.tar.gz` directly. A boot
`.img` can be paired with a matching local filesystem, but contains no game
filesystem by itself. Unsupported filesystem layouts fail with an explicit
error rather than publishing a partial profile.

Imported profile assets include the recognized catalog, artwork, UI atlases,
bitmap fonts, language tables, menu audio, supported converted ROMs, and verified
SMP boot firmware. SNES SFROM conversion restores stock PCMF audio against the
supplied PCM and recorded loop histories. Conversion does not assert retail
byte-identical ROM recovery or complete gameplay compatibility; see
[`sfrom-import.md`](sfrom-import.md). Super Mario Kart requires user-supplied
DSP1B firmware. NES import supports the documented stock iNES 1.0 boards and
fails closed for unsupported formats/boards. Famicom Disk System games are not
converted.

The import pipeline is implemented in Rust (`image_reader.rs`, `skin.rs`,
`sfrom.rs`, and `profile.rs`). It selects bounded regular files in memory and
never runs the original menu, game binaries, shell scripts, or Lua modules.
See [image reader formats and limits](image-reader.md).

## Profile/cache layout

The default data directory is the platform's application data location. Override
it with `--data-dir /path/to/data` or use Settings → Data Folder.

```text
profiles/<filesystem-sha256>-v7/
  profile.json                 source identity and output hash manifest
  assets/catalog.json
  assets/skin/...
  assets/games/...
  assets/roms/...
  assets/firmware/ipl.rom
  data/preferences.json
  data/saves/...
last-profile.json
```

Each source digest and importer revision has an isolated profile. Output is
staged and published only after import succeeds; published files are checked
against the profile manifest before reuse. Upgrades preserve user settings and
saves in a new profile directory while leaving the previous profile intact.
Offline valid profiles remain usable. SRAM and suspend data are game/profile
local; ares states are not compatible with original console suspend files.

## Command-line use and verification

The app binary supports explicit profile operations for automation:

```sh
clover-rust import-image IMAGE.hsqs CACHE_DIR
clover-rust list-profiles CACHE_DIR
clover-rust run --image IMAGE.hsqs --data-dir DATA_DIR
clover-rust run --assets PROFILE/assets --data-dir PROFILE/data
```

The default desktop build contains only application code and licenses. User
images and imported assets are not bundled. Do not redistribute an asset-enabled
or locally prepared package unless you have rights to every included item.
