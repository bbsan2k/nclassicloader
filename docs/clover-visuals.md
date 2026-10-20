# Clover visual reconstruction — static asset and scene analysis

## Recovered layout and text observations

The label pass corrects several real differences from the source layout:

- The scene's HUD label resource `9db70803-...` resolves through
  `dependencies.json` to **s.font**, not the previously used m.font. Its horizontal
  positions are now measured using the source 4-pixel icon/label gap and 30-pixel
  group gap, centered as a group instead of using four hard-coded X positions.
- Toolbar captions use their source body Y=122 and label Y=128, with the original
  66-pixel height and dynamically measured width. The old label Y=116 placed text
  12 pixels too high. `sys_menubar_cm.lua` prototype 3 confirms the body width is
  the maximum of its initial width and the label's measured width, and the edges
  are positioned from half-widths.
- `sys_hud.lua` prototype 2 confirms accumulation of measured child widths plus
  gaps, subtraction of the trailing gap, and centered positioning. Native label
  measurement/anchor internals are not fully reverse engineered; BMFont logical
  advances provide the recovered label measurements.
- The BMFont baseline and nominal font size are retained. Missing subset-font
  glyphs (notably some digits in m.font) are borrowed from the other shipped
  fonts, scaled by nominal size, and aligned to the primary baseline rather than
  replaced with differently spaced question marks.
- `lbl_component.lua` applies environment colors at startup. Static decoding of
  `colors_eur.lua` confirms white `font_dialog_text` and black
  `font_home_gametitle`, overriding the scene editor's gray tints. Tooltip text
  now remains readable against the black caption texture.

New decoded modules and their input hashes are retained in `analysis/text-layout/`.
These are file-derived observations, not additional REA Evidence or original
runtime captures. They describe source resources and inferred layout; they do
not establish pixel-identical behavior in either frontend.

For the separate emulator backend question, see
[Canoe port feasibility](canoe-port-feasibility.md).

## Evidence and boundaries

The earlier REA/Ghidra native loader evidence remains in
`analysis/rea-evidence.json`; see [the original ledger](reverse-engineering.md).
This pass primarily inspects structured scene/resource files and statically
decodes LuaJIT bytecode. **These new observations are ordinary file-derived
analysis, not new REA Evidence records and not console runtime observations.**

The independent bytecode investigation, exact input hashes, saved opcode
reference, custom decoder, and disassemblies are retained in
`analysis/style-investigation/`. Its detailed ledger is `FINDINGS.md`. The custom
decoder validates full consumption, operands, and branch targets for the inspected
dumps; it has not been cross-checked with another LuaJIT disassembler. No shipped
native executable, shell script, or LuaJIT module was executed.

## What makes this the Clover menu

### Real sprite atlas, not redrawn approximations

`resources/sprites/packed.json` supplies top-left pixel crop rectangles into
`packed.png` (512×1024). The Rust frontend renders the imported atlas as image
data rather than redrawing the original artwork. Used sprites include:

- `menubar_U.png`, `menubar_L.png`: the grey plastic console bars and built-in logos.
- `game_card.png`, `game_card_active.png`: inactive grey and active blue cards.
- `cursor_1/3/7/9` and `cursor_game_2/4/6/8`: square selection-frame corner/edge tiles,
  as referenced by the scene's `cursor_square` nodes.
- `caption_title.png`, toolbar icons, player icons, controller-button prompts,
  small thumbnail pointers, and dialog-frame components.

The black wallpaper is **not a flat black fill**: `prefab/bg.scn` references the
blurred neon controller texture `layout/asset/sprite/wallpaper/wall.png`. That is
now drawn behind the cards at the scene's 1536×552 dimensions, with repeated
horizontal scrolling. The scene's wallpaper animation has duration
8.533333333333333 and texture X offset 0→1. Lua background code may alter that
  motion; the scene's authored default does not establish complete live wallpaper
  behavior.

### Shipped bitmap fonts

The `.font` files are neither TrueType fonts nor unreadable arbitrary blobs.
Inspection establishes this container for the four used fonts:

```text
u8 version (1)
u32 little-endian BMFont block length
u32 little-endian PNG block length
BMFont v3 binary data
PNG glyph atlas
```

The section lengths exactly consume each file. The decoder reads BMFont common,
character, and kerning blocks; validates glyph rectangles against the PNG size;
and exports JSON metrics plus the embedded PNG without rasterizing new fonts.
The renderer uses glyph offsets, advances, kerning, source alpha, and tint.
Stock `title`, `m`, `s`, and `l` fonts replace system text in the menu and panels.
These subset fonts are not guaranteed to cover arbitrary imported game names.

### Scene dimensions instead of responsive widget layout

The view is one 1280×720 stage, scaled uniformly and letterboxed. Source scene
coordinates are centered with Y upward; rendering converts them with
`screenX=640+x`, `screenY=360-y`.

| Element | Observed source / resulting stage measurement |
| --- | --- |
| Top bar | center (640,36), size 1296×120 |
| Top icon group | center Y=62; scene icon separation 96 |
| Title caption | center (640,178), size 1044×66 |
| Game cards | 252×276; artwork 228×204 at local Y=24 |
| Horizontal spacing | 262, computed from the scene's `narabe1`/`narabe2` marker links |
| Thumbnail row | center (640,551); prefab images 32×32 |
| Controller HUD | center Y=609 |
| Bottom bar | center (640,690), size 1296×120 |

Recovered scene measurements and source hashes are retained alongside imported
profile assets. Some placements, pointer scale, panel composition, and dialog
interaction remain reconstruction choices rather than direct scene replay.

## Recovered movement behavior

Static LuaJIT control-flow inspection found:

- `gui_custom.lua` defines normal horizontal title navigation with initial delay
  0.24 and repeat 0.09. `sys_gametitlelist.lua` reads those GUI values, computes
  spacing from scene markers, and calls a **linear** carousel tween. Partial-gap
  moves proportionally shorten its duration. Its separate demo constants are not
  used for normal input.
- `sys_cursor.lua` captures duration **0.3** and **Ease.outExpo** for independent
  movement/resizing of the cursor's nine pieces. Immediate changes bypass it.
- The cursor uses a repeating **color tint**, not an alpha blink: white over
  0.5, hold 0.75, tint to RGBA (0.75,0.75,1,1) over 0.75, hold 0.25. Nominal total
  2.25. Color interpolation ease is not explicit in the recovered evidence.
- `data_gamelist.lua` explicitly loads `game_code .. "_small.png"` for icons and
  defines sort modes in order: title, players, recent play, play count, release,
  manufacturer. This is a source observation, not a claim about the current
  frontend's exact sort semantics.

These are recovered nominal timing values; their engine time-unit boundary was
not separately traced. Current keyboard/controller repeat and animation timing
are frontend behavior, not proof of native console parity. The thumbnail pointer
uses a 13-entry sprite sequence; interpreting raw 33.333333 duration values as
milliseconds and looping them remains an assumption without native playback.

## Runtime limits

Static scene/resource data and bytecode observations do not reproduce the full
runtime. Original demo sequences, exact script-driven background acceleration,
all transition timings, and pixel parity require runtime evidence. No proprietary
menu program or script is included or executed by this repository.

## Verification

Rust importer and UI tests validate font/container handling and the reconstructed
scene when corresponding user-supplied local resources are available. These
checks validate data handling, not original-console runtime behavior or pixel
parity.

There is still no physical-console screenshot/interaction capture proving a
pixel-identical result. This is an asset-backed visual reconstruction, not a
claim that the full original Reed engine has been ported.
