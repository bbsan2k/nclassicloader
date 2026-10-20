# Clover application icon

`design.json` is an original geometric design made for this project, licensed
under the repository's MIT license. Its four mint leaves and central play
aperture do not use Nintendo imagery, console artwork, or imported assets.

The normalized geometry is shared by `rust/src/ui/icon.rs` (SDL window icon)
and `scripts/generate-app-icon.py` (macOS bundle icon). Only source is tracked.
Generate the complete icon family with:

```sh
python3 scripts/generate-app-icon.py --output .build/icon/ClassicMenu.icns
```

The generator uses only Python's standard library and writes PNG-compressed
ICNS entries under `.build`; it needs neither an image library nor downloaded
artwork. The macOS packaging script runs it automatically.
