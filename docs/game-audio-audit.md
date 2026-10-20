# Headless imported-game audio/video audit

## 0.5.0 restored-image runtime results

All 126 regional SFROMs converted; **120/126** game/profile combinations passed
nonzero finite PCM and nonblack/changing-video audits. That is **20 games in
each of the six supplied images**. The six remaining combinations are Super
Mario Kart, which fails explicitly for missing user DSP1B firmware; no replacement
firmware was downloaded. Full results: `analysis/regions-verification.json`.

Most runs used 600 frames, Start at 300 and B at 420. Eleven initially quiet
FFIII/Contra runs were retained and retested for 1800 frames with no injected
input. FFIII's music starts later; early Start can skip Contra's intro music.
All eleven extended runs passed. These are startup/intro and buffer-level tests,
not complete gameplay or human listening validation. Across successful runs,
there were zero nonfinite samples; 17 saturated samples were recorded, not a
claim that every soundtrack sample is unclipped.

The packaged frontend's SDL path was independently exercised using SDL's disk
audio driver: Super Mario World produced nonzero output after the stream mixer
resampled the 48 kHz core to a 44.1 kHz stereo S16 device. Capture statistics:
`analysis/frontend-audio-verification.json` (RMS approximately 0.02694).
This tests the actual playback bridge, not only the core's float buffers, but
still does not establish physical speaker/headphone audibility.

A self-authored one-second sine replaced only a test copy's menu music.
Four seconds of SDL output crossed multiple loop boundaries, with a longest
silent run of one sample after startup. The old zero-queue refill is replaced
with pre-drain replenishment. Test: `scripts/test_audio_playback.py`; result:
`analysis/menu-audio-loop-verification.json`. No user image/cache was modified.

Current game smoke audits run through the Rust UI and the same Rust-to-ares
wrapper used by the application. For a prepared profile:

```sh
clover-rust run --assets PROFILE/assets --data-dir PROFILE/data \
  --audit-games .build/game-audio-audit
```

The audit executes each imported supported game for 600 frames, injects Start
and action-button inputs, checks for video/audio output and finite samples, then
save/replays 30 frames. Its SRAM and state files are isolated under
`OUTPUT/runtime/`; the JSON summary is written to `OUTPUT/games.json`. It does
not use an audio device or modify the source image. Blocked entries (such as
Super Mario Kart without DSP1B) are reported rather than supplied with guessed
firmware.

Historical numeric measurements below came from the retired C++ audit helper.
They remain useful evidence for those exact local runs, but are not a current
product dependency or a claim of complete gameplay, accurate music, or human
audibility. The current Rust exercise checks buffering and core output; physical
speaker/headphone quality still needs listening tests.

## Observed supported-profile sanity run

2026-10-19, imported profile
`.build/import-launcher-check/profiles/1519a85cbaa0984c97f6237e8d2659ef694e5dfad0ceefa54af9fbc999f72fed-v2/assets`,
600 frames, Start at 300, B at 420, profile `ipl.rom`, isolated saves under
`analysis/game-audio/`. Initial results:

| Game | Samples (L+R) | Nonzero | RMS | Peak | Nonblack frames |
| --- | ---: | ---: | ---: | ---: | ---: |
| Star Fox (`SADGE`) | 958294 | 958292 | 0.0592500 | 0.703607 | 300 |
| Yoshi's Island (`SADJE`) | 958288 | 958286 | 0.0101816 | 0.365686 | 496 |
| Star Fox 2 (`SADKE`) | 958288 | 958286 | 0.0480469 | 0.356866 | 314 |

All initial runs had zero nonfinite and zero clipped samples. Their original
reports are preserved as `initial-report.json`; current reports, WAVs and PPMs
are alongside them. `analysis/game-audio/summary.json` records commands, ROM
SHA-256s and current comparison metrics. WAV headers and frame counts were
independently checked with Python's `wave` reader. A missing-IPL negative test
is preserved as `analysis/game-audio/missing-firmware.json`.
The repeat run reused only these audit-owned save directories; Star Fox 2 RMS
changed slightly to 0.0480434. It is not a fresh-SRAM determinism comparison.
`one-frame.json` checks a legitimate short boot fails the activity criteria
without a core error; `invalid-frames.json` checks rejected negative duration.

This is buffer-level evidence only, **not human listening verification**. If
these PCM checks pass but frontend playback remains silent, investigate the
playback bridge: float32 versus PCM16 format, stereo interleaving, 48000 Hz
device configuration, sample versus byte counts, paused devices, queue draining
and queue lifetime. No shared-core or playback-bridge changes were made here.
The utility accepts any converted `.sfc`/`.smc` supplied by other regional
profiles; these three runs alone do not establish all supplied-region coverage.

## Super Mario Kart / DSP1 firmware

The analyzer in `third_party/ares/mia/medium/super-famicom.cpp` selects DSP1B
for Super Mario Kart (DSP1 is selected specifically for Pilotwings). The bridge
loads coprocessor ROM memory from appended ROM bytes, or explicit firmware
files named `dsp1b.upd7725.program.rom` (6144 bytes) and
`dsp1b.upd7725.data.rom` (2048 bytes). It also requires the separate 64-byte
`ipl.rom`. SMP IPL is **not** DSP1 firmware.

Read-only hash investigation of `analysis/eur/usr/bin/canoe-shvc`, SHA-256
`30d0768623e875dc8f3de334ccc7193807878c8ad9b50c38bec362deb4245a61`:

* Compared every contiguous 8192-byte window (1,827,821 windows) against the
  published DSP1 and DSP1B `.rom` and `.bin` SHA-256s from
  <https://caitsith2.com/snes/dsp/>. Only the HTML hash reference was fetched;
  **no firmware was downloaded**.
* No exact match. See `analysis/game-audio/dsp1-scan.json` and reproducible
  `analysis/game-audio/scan_dsp1.py`. This excludes only intact, contiguous
  standard representations, not compressed/split/transformed firmware or HLE.
* REA identified the ARM ELF and matching file digest, but Ghidra's overview
  failed with Java heap exhaustion/locked buffers. No decompiled-code finding
  is asserted. The hash scan is ordinary local-file parsing, not REA Evidence.

There is therefore **no verified Canoe byte-range extraction recommendation**
from this scan; do not guess offsets or claim DSP1 is present. If an independently
verified little-endian DSP1B combined dump is supplied by the user, its expected
SHA-256 is
`d789cb3c36b05c0b23b6c6f23be7aa37c6e78b6ee9ceac8d2d2aa9d8c4d35fa9`;
the conventional split is `[0, 6144)` program and `[6144, 8192)` data, named as
above. Verify the full combined hash **before** splitting. This is a layout
recommendation, not a finding of those bytes in Canoe, and does not implement
extraction or change the importer.
