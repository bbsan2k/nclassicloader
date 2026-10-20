# NES menu colors and layout audit

## Scope and evidence

Read-only static analysis of the supplied NES 1.0.2 image. The `clover-ui`
launcher selects **ReedPlayer-Clover** with `/usr/share/clover-ui` as its boot
application; `kachikachi` is the game emulator, not the menu renderer.

ReedPlayer identity:

- ELF, ARM; SHA-256 `8ba308927f3e487b7d5b80d8232a8baf30cdcb1c10c1c2201e7e1d04abbc12db`.
- Ghidra 12.1.3, ephemeral/immutable-source REA session.
- `0x10de14`: Lua VisualComponent binding registration, including color, alpha,
  size, filters and wrapping. Evidence:
  `ev_31c91c69353f36175bd3009b57052611d0b1d15d4708d4a512bd90900aff4c5a`.
- `0xe97d4`: render batching forwards per-object blend settings to
  `glBlendFunc`/`glBlendEquation`, and sampler wrapping to `glTexParameteri`.
  Evidence:
  `ev_c3a9b6761751791caf8d25a2572b1dba5020766cb33c89f1119f03e2f5009db2`.

The binary's bindings/rendering do not themselves establish the final palette.
That comes from the shipped LuaJIT color tables and script data flow. The new
`scripts/inspect-luajit.py` reads their bytecode, constants and offsets without
loading a Lua interpreter or executing target code. It includes self-authored
parser tests. The ignored `.build/nes-menu-rea-evidence.json` retains the full
native Evidence records locally; original executables/resources are not tracked.

## Verified palette corrections

Color resources are under `usr/share/clover-ui/resources/scripts/`.
Import requires the already verified exact palette hashes:

- `colors_nes.lua`: `679e8f16ccd77ad74a366da70bfde8ed212303869c9005fc12cd84b9fe185d8b`.
- `colors_hvc.lua`: `70d94dd40873be9e92476958080545338e7196ab2710cec082026b62e40eef0c`.

Normalized doubles are rounded to 8-bit RGBA for the Rust renderer:

| Value | NES | Famicom |
|---|---|---|
| `bg_base` | `#00153D` | `#EBE2D5` |
| `font_home_hud` | `#DB2B00` | `#A70000` |
| `font_home_gametitle` | white | black |
| `font_home_caption` | black | black |
| `font_resume_hud` | `#DB2B00` | black |
| `font_resume_title` | `#DB2B00` | white |
| `font_resume_time` | black | white |
| `font_resume_empty` | black | black |

All listed alpha values are 1. NES table offsets include `bg_base` at `0x3d4`,
home HUD at `0x2ac`, title at `0x2de`, and caption at `0x2fc`. HVC counterparts
are `0x3a6`, `0x288`, `0x2b0`, `0x2ce`. In the color chunk, TDUP table constants
are explicitly assigned to these named fields; this is not string coincidence.

The static scene's gray title-label default is overridden by the palette. The
NES title atlas has a **black** interior, so white title text is correct. Its
tooltip atlas has a **white** interior: the old forced-white tooltip was wrong.

## Background: the grid was not wallpaper

`bg.lua` starts with `bg_base`, alpha 1, and transition pattern 0 (disabled).
Its RGB transitions last 0.25 seconds. The default scene's background is an
untextured 1280x720 sprite at z-index -50.

`sys_wallpaper.lua` initially hides the pixel-art layer. The inspected idle path
waits 60 seconds before a Disk System demo transition, then a further 3 seconds
before enabling its background formations. Exact activation also depends on
GUI/input/demo gates. This is not a permanent repeating dot lattice.

The previous Rust background incorrectly stretched a complete 32x32 texture
into scrolling 96px tiles at alpha 20, over an invented bright blue. This has
been removed: the active menu now uses the verified solid regional base color.

For future idle-animation reconstruction, `pixel_character.lua` uses the
26x26 `Sprite2` subrectangle of `wallpaper-dots.png`, one dot per opaque source
pixel, sampled RGB with alpha 1, pitch 16, and initial sprite size 19.68. Entry/
exit animate scale rather than alpha. The default four 16x16 image crops are
explicitly selected from `wallpaper-hvc.png`. Lua specifies BLEND_MODULATE for
these formations; the native blend preset's exact factor mapping has **not**
been established here. No approximate idle/demo animation is substituted.

## Layout fixes from scene/prefab data

Coordinates are 1280x720, top-left origin. Main resource: `defaultscene.scn`;
card resource: `prefab/sys_game_card.scn`.

- Restore NES header backing `menubar_U_2`: 480x120, center (640,36), above
  the base bar and below the five icons. Its narrow atlas cell is intentionally
  stretched, not tiled. Header/footer base dimensions were already correct.
- NES card backgrounds: 246x270, center (card_x+3,363), spacing 256.
- Selected player indicators use `_active` sprites: 57x33, center
  (card_x+76.5,463.5). Resume markers start at card_x-88, step 24.
- Title backing: 1026x66. Rounded focus outlines use one consistent sprite family.
- Home HUD: center y612; direction glyphs 30x30, SELECT 90x24, START 78x24.
- NES Display: previews center y366, 240x180 (CRT/4:3), 192x180 (square pixels),
  TV frames 318x192. Labels y493, radio controls y540, focus region 360x360
  centered y396. Remove the SNES-only decorative-frame section and its focus row.
- NES suspend previews: 162x122 at y528. Empty-slot number glyphs: 39x39,
  center (slot_x-124.5,475.5). Apply regional heading/HUD/empty-slot colors.
- NES lacks Y/X resume atlas glyphs. Use its A glyph for the actual primary
  save-or-load choice, and show the extra emulator lock shortcut as keyboard X
  instead of silently omitting a nonexistent sprite. This remains a custom
  emulator action row, not a recreation of the original lock/move mode.

## Coverage and limits

These are shipped-byte/static-data observations plus reconstruction choices,
not observation of an original console UI running. Complete animation timing,
idle demo/character behavior, original suspend interactions, and pixel-perfect
runtime parity remain unverified/unimplemented. Cartridge emulation is unchanged.

Importer v7 regenerates corrected theme data and preserves v4/v5/v6 settings and
saves in a new profile directory. Offline older caches remain accepted. Checks
cover exact regional palette values, background/header screenshot samples,
tooltip contrast, all imported languages, display navigation, and suspend flow.
