# Third-party notices

The original project source is licensed under `LICENSE`. Dependencies and
assets retain their own terms; this file is an index, not a relicensing grant.

* ares and its bundled components: `third_party/ares/LICENSE` when acquired
  from the pinned revision; the bridge/generated ares-derived code also has
  `portable/core/LICENSE.ares`.
* SDL3: zlib license in `third_party/SDL/LICENSE.txt`; the build package also
  includes the notices for the SDL HIDAPI and YUV-to-RGB components.
* Rust dependency licenses, Barlow font notices, and flag SVG notices are
  included in desktop packages under `licenses/` and Apple preview bundles under
  their `licenses/` resource directory, alongside native dependency notices. Source notices are
  `rust/assets/fonts/OFL.txt` and `rust/assets/flags/LICENSE.txt`.

Console system images, firmware, game ROMs, extracted artwork, fonts, music,
language data, and other console-derived assets are not included in this
repository's source distribution or the default code-only desktop packages.
Optional/local asset-enabled builds can contain such material. The project
license does not grant rights to those materials, Nintendo trademarks, or
reverse-engineering activities. A local build made with user-supplied assets is
not thereby cleared for redistribution.
