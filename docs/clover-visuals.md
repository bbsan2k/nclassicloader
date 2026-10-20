# Clover visual reconstruction — second pass

## Follow-up: text offsets (0.2.1)

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
  measurement/anchor internals are not fully reverse engineered; the reconstruction
  uses BMFont logical advances for these scene-anchored labels.
- The BMFont baseline and nominal font size are retained. Missing subset-font
  glyphs (notably some digits in m.font) are borrowed from the other shipped
  fonts, scaled by nominal size, and aligned to the primary baseline rather than
  replaced with differently spaced question marks.
- New reconstruction panels center text by its glyph bounds, including overhangs
  and descenders, rather than centering the entire padded line box and compensating
  with ad hoc offsets. Source scene labels retain line-box anchors and positions.
- The Back icon/label is centered as a complete group. The non-source label on
  the lower console bar is removed; sorting is shown in its existing toast.
- `lbl_component.lua` applies environment colors at startup. Static decoding of
  `colors_eur.lua` confirms white `font_dialog_text` and black
  `font_home_gametitle`, overriding the scene editor's gray tints. Tooltip text
  now remains readable against the black caption texture.

New decoded modules and their input hashes are retained in `analysis/text-layout/`.
These are file-derived observations, not additional REA Evidence or original
runtime captures. Alignment tests cover baselines, subset-font fallback, prompt
group bounds, source label positions, and actual selected-button pixels at 1×/2×.
The native-label implementation and every panel are still not claimed pixel-exact.

For the separate emulator backend question, see
[Canoe port feasibility](canoe-port-feasibility.md).

The first preview was a catalog browser, not a faithful representation of
Clover's art. This pass replaces its system labels, green outline, cream backdrop,
and generic controls with the assets actually shipped in the European firmware.

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
`packed.png` (512×1024). `CloverAssets.swift` crops these using CoreGraphics and
draws pixel art with nearest-neighbour interpolation. Used sprites include:

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
motion; this implementation uses the scene's authored default, not a claim of
complete live wallpaper parity.

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

`scripts/clover_assets.py` derives positions and spacing from the scene rather
than relying on the old preview's layout. Each build includes `Clover/layout.json`
and `Clover/provenance.json`, with SHA-256 identities of its input art/scene/font
resources. Some local placements, pointer scale, panel composition, and dialog
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
  2.25. Color interpolation ease is not explicit; the reconstruction uses linear
  interpolation and a quantized tint cache.
- `data_gamelist.lua` explicitly loads `game_code .. "_small.png"` for icons and
  defines sort modes in order: title, players, recent play, play count, release,
  manufacturer. The preview's cycle order now follows this.

These nominal duration arguments are implemented in `CloverMotion.swift` and
tested. Their native engine time-unit boundary was not separately traced.
Keyboard repeat **scheduling** still follows macOS events, not an emulated
console controller input stack. The thumbnail pointer uses its actual 13-entry
sprite sequence; interpreting raw 33.333333 duration values as milliseconds and
looping them is explicitly an assumption until native playback is traced.

## App changes and limitations

`CloverMenuView.swift` is an AppKit drawing/input surface inside the existing
SwiftUI app shell. It renders the original art at fixed design coordinates;
supports circular navigation, mouse selection, original-looking toolbar/dialog
components, settings, key prompts, and external emulator mapping. Menu audio is
optional and initially off. Only native file choosers remain macOS-styled.

Suspend-point pips are decorative **empty placeholders**, not imported console
saves. The Resume action explains the missing implementation rather than
pretending states exist. Language selection, original demo/Mario sequences,
save/rewind/unlock state machines, exact script-driven background acceleration,
all transition timings, and runtime pixel parity are still incomplete. Custom
rendering exposes the selected game and keyboard help as an accessibility group;
it does not yet provide a complete per-sprite VoiceOver navigation tree.

## Verification

- 7 Swift model/motion tests and 5 native rendering/input/text tests pass.
- 3 Python decoder/scene tests pass, with all four font containers exercised.
- Rendering tests validate 1280×720 output, title/cyan-card pixels, modal scene
  differences, arrow navigation, modal input blocking, source font/anchor data,
  fallback glyph baselines, selected-label pixel centering at 1×/2×, and readable
  tooltip text.
- The actual renderer exports `analysis/clover-menu-preview.png` and
  `analysis/clover-settings-preview.png`, inspected locally without capturing
  other windows or the user's screen. These are reconstruction outputs, **not
  screenshots of the original console**.

There is still no physical-console screenshot/interaction capture proving a
pixel-identical result. This is an asset-backed visual reconstruction, not a
claim that the full original Reed engine has been ported.
