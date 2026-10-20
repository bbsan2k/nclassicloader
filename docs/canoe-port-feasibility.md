# Canoe versus a native open-source core

## Recommendation

Keep the Clover recreation as the frontend and use an existing SNES core. For a
new integrated macOS implementation, start by evaluating **ares's SNES core**:
its project supports macOS and its primary license is ISC. **bsnes** is an
excellent SNES-focused alternative if GPLv3-or-later is acceptable. **Snes9x** has
a practical libretro interface and macOS support, but its custom non-commercial
terms need explicit consideration; do not treat it as a permissively licensed
OSI-open-source dependency.

This is an engineering recommendation, not an implemented integration or a
benchmark. Check the selected revision, dependency licenses, coprocessor firmware
requirements, and all 21 intended games before committing to a backend. No core
was downloaded, built, installed, or added to this app during the assessment.

## Supplied Canoe artifact

`analysis/eur/usr/bin/canoe-shvc`:

- SHA-256 `30d0768623e875dc8f3de334ccc7193807878c8ad9b50c38bec362deb4245a61`.
- Stripped ELF32 little-endian ARM/Linux executable. The original source needed
  for a conventional recompile/port is not supplied.
- ELF interpreter `/lib/ld-linux-armhf.so.3`.
- `DT_NEEDED` includes SDL2, EGL, GLESv2, GLESv1, **libMali**, and Linux runtime
  libraries. These are parsed file facts in `analysis/canoe-layout.json`, not
  standalone claims based on finding a library name in arbitrary strings.

Apple Silicon being ARM does not make a 32-bit ARM Linux ELF executable into a
64-bit ARM macOS Mach-O executable. Rosetta is not an ARM Linux compatibility
layer. Reusing this artifact would require CPU/binary translation or a substantial
reverse-engineered reconstruction, together with OS/ABI and graphics adaptations.
An emulated Linux environment is a possible preservation research route, not an
established working macOS port.

## Focused REA/Ghidra observations

Complete static records and limitations are retained in
`analysis/canoe-port-evidence.json`. Ghidra 12.1.3 was used, and its owned session
was closed after export. The target was not executed.

| Evidence ID | Observation |
| --- | --- |
| `ev_52a23f5438d2198a19c0ef358895ee1613b5f4a7cc6d0c7796edfad354810705` | Procedure inventory contains SDL window, renderer, audio, controller, thread and GL imports, plus ioctl/mmap and main |
| `ev_f8b5500588244a65c2ef2374eea2109adaf10530b6b8ce33165537235225b778` | Defined strings include graphics libraries, a Mali400 latency option, SFROM handling, and Linux framebuffer diagnostics |
| `ev_e302b8256cf26be9c7bebdb919cd44f2d44a03eac1834bdc3928e6476f072421` | Reference from `0x15e4c8` to `/dev/fb0` |
| `ev_76201fed8acb6276167d8947105b8869a3abe6f7b80de6d6717c8f2fc6d482e4` | That reference lies in function `0x15e4b8` |
| `ev_7e2ac82a54c59de529093f15844f13056e917eec9f4bf440da491f47681e0feb` | Function pseudocode attempts framebuffer open/ioctl/mmap, checks 1280×720×32 with virtual height 1440, and also initializes SDL or GL rendering paths |

The framebuffer evidence is stronger than an unused string, but it **does not
prove every rendering mode requires a working framebuffer**. The recovered
function proceeds to additional SDL/GL setup after some failure paths. No claim
is made that the whole emulator is hardware-exclusive, that all dependencies are
unreplaceable, or that it could never be ported. Static dependencies are not a
complete dynamic execution trace. CPU emulation algorithms, code-generation
strategy, full ABI boundary, and save formats were outside this focused pass.

`usr/bin/clover-canoe-shvc` is a shell wrapper, inspected separately as ordinary
file content. It maps controller-menu arguments to Canoe options for SRAM,
save/resume, recorded inputs, pixel/CRT modes, decorative frames, and rollback.
That illustrates why replacing only the launch command is insufficient to obtain
Clover-compatible suspend/rewind behavior.

## Why reuse a core instead

An open implementation gives us buildable source and a maintained emulation
engine. The remaining project work becomes well-defined frontend integration:

1. A small C ABI around the selected C++ core (or a suitably licensed libretro
   core), exposing load/unload, run-frame, video/audio callbacks, input,
   serialization, and persistent cartridge memory.
2. Metal-backed frame presentation, CoreAudio/AVAudioEngine audio buffering,
   controller/keyboard input, and an emulation thread separated from AppKit.
3. Frontend-managed suspend slots and thumbnails using that core's serialization
   format; optional rewind through a bounded state/input history.
4. Versioned state metadata recording core version and ROM identity. **Canoe
   save states are not interchangeable with another core's states.** Cartridge
   SRAM import needs its own format checks rather than assuming all files copy
   directly.
5. A validated SFROM import/conversion step or user-supplied compatible ROMs.
   SFROM metadata, game-specific patches, and cartridge/coprocessor configuration
   must not be discarded blindly by stripping a guessed fixed-size header.

The current app still opens an explicitly selected external emulator and ROM.
An integrated core would allow suspend points and transitions to live inside our
menu instead of being delegated to another app.

## Primary upstream references checked

Checked during this assessment, 2026-10-19; these are mutable upstream branches,
not pinned build inputs or proof that a particular binary was tested here:

- [ares README](https://github.com/ares-emulator/ares/blob/master/README.md):
  macOS build support; accuracy/preservation focus; separate cores and desktop UI.
- [ares LICENSE](https://github.com/ares-emulator/ares/blob/master/LICENSE):
  ISC project license, with additional bundled-component notices/licenses.
- [bsnes README](https://github.com/bsnes-emu/bsnes/blob/master/README.md):
  multi-platform SNES emulator, macOS builds, save states and rewind features.
- [bsnes LICENSE](https://github.com/bsnes-emu/bsnes/blob/master/LICENSE.txt):
  GPLv3-or-later for bsnes, separate notices for supporting libraries.
- [Snes9x README](https://github.com/snes9xgit/snes9x/blob/master/README.md):
  macOS and libretro build profiles.
- [Snes9x LICENSE](https://github.com/snes9xgit/snes9x/blob/master/LICENSE):
  non-commercial/personal-use restrictions and additional component licenses.

Emulator-code licenses do not grant redistribution rights to Nintendo ROMs,
graphics, or music. Any distributable product needs a separate rights/licensing
review for the assets it includes.
