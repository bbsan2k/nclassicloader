# Native stock SFROM import and audio restoration

`portable/import/sfrom.cpp` provides the C++17, memory-only API
`clover::importer::convertSFROM(const std::vector<uint8_t>&)`. It returns
`{rom, status, metadata}` or throws `std::runtime_error`. It neither runs Nintendo
programs nor uses subprocesses, network access, firmware, or replacement ROMs.
Its only library dependency is the existing `third_party/vendor/json.hpp`.

## Verified result

All **126 SFROM files in the six supplied local regional archives** pass:

- 107 PCM containers: **1,694,671 restored BRR blocks**, plus **14,543 recorded
  alternate loop-history blocks**, with **zero sample differences**.
- 18 unchanged non-PCM payloads, with their original checksums verified.
- One Japanese Super Formation Soccer payload with a narrowly recognized,
  malformed cartridge-map byte normalized and its checksum recomputed.

These counts include repeated games across archive versions, not 126 different
games. Every restored first-pass sample and every reachable recorded loop
variant was also compared using an independent test decoder. AddressSanitizer
and UndefinedBehaviorSanitizer passed the synthetic suite and all 126 files.
The supplied EUR set now converts **21/21**, including all 18 previously blocked
PCM titles. No commercial program was executed for this static converter audit.
Runtime audio, gameplay, controllers, and coprocessor compatibility are separate
tests; conversion metadata deliberately says `runtime_tested: false`.

Local audit artifacts (contain identifiers/metrics, not replacement game data):

- `analysis/audio-restoration/fixture-manifest.json`: archive/member identities
  and input SHA-256 fingerprints.
- `analysis/audio-restoration/region-audit.jsonl`: per-file conversion metadata.
- `analysis/audio-restoration/sanitizer-regions.txt`: independent-decoder tests.
- `.build/sfrom-restoration/extract-fixtures.py`: reads only SFROM tar members
  into a flat fixture directory; never executes supplied programs or scripts.

## Container layout

Supported containers have the official 48-byte header and trailing footer.
Little-endian `u32` fields:

| Offset | Meaning |
|---|---|
| `0x00` | Version/profile `0x100` |
| `0x04` | Entire file size |
| `0x08` | ROM start, `48` |
| `0x0c` | ROM end / PCM start |
| `0x10` | PCM end / auxiliary table start |
| `0x14` | Main footer start |
| `0x18` | Extension location, or EOF |
| `0x20` | Main-footer flag location, footer + 27 |
| `0x24` | Eight ASCII bytes of Virtual Console ID |

Packed main-footer sizes at offsets 1, 5, and 9 must agree with the bounded,
ordered section ranges. Known stock footer lengths/flags, frame rates,
mapping hints, and enhancement hints are checked. Unknown flags/layouts fail.
Simply removing 48 bytes is **not** conversion: it leaves PCMF records in the
BRR sample banks, or incorrectly includes PCM/footer bytes in the cartridge.

### PCMF and auxiliary records: no offset guesses

Each nine-byte replacement record in the ROM is:

| Bytes | Observed meaning |
|---|---|
| 0–3 | ASCII `PCMF` |
| 4–6 | Little-endian PCM **sample** offset, unless control bit `0x10` is set |
| 7 | BRR END/LOOP bits 0–1, predictor/filter bits 2–3, auxiliary flag bit 4 |
| 8 | Identity byte, also present in the primary auxiliary entry when flagged |

PCM samples are signed **big-endian 16-bit** values. With bit `0x10`, bytes
4–5 instead select a six-byte auxiliary-table entry. Byte 6 retains stale
address-bank bits: it is **not** part of the table index. An auxiliary entry is:

| Bytes | Meaning established by complete supplied-file comparisons |
|---|---|
| 0 | Identity byte (checked against the marker for its primary entry) |
| 1–2 | Next auxiliary index, little-endian; zero terminates |
| 3–5 | Actual PCM sample offset, little-endian |

The chain describes the **same BRR block under additional loop histories**.
For example, an apparently backward marker address of zero can mean auxiliary
entry zero, whose PCM offset is forward and properly bounded. All such records
in all supplied regions resolve without `previous + 16`, alignment repair, or
discarding loop data. Invalid indices, identity mismatches, truncated blocks,
out-of-range samples, cyclic chains, unknown control bits, and excessive table
work are rejected. Unreferenced auxiliary entries are not synthesized into ROM.

## Exact-signal BRR inversion

The implementation is independently authored from these format observations
and the SNES DSP BRR decode specification:

1. Preserve the record's filter, END, and LOOP bits. Reset predictor history
   at a separate ROM sample stream or after an END block; otherwise retain the
   preceding restored block's decoded history.
2. Enumerate the BRR scale and all sixteen possible signed nibbles. For each
   sample require the DSP's arithmetic rounding, filter prediction, saturation,
   and signed 16-bit wrapping to reproduce the supplied PCM **exactly**.
3. Require each nibble to satisfy **every recorded loop history simultaneously**.
   Saturation can conceal several possible nibbles on the first pass. Choosing
   one arbitrarily can reproduce that pass but corrupt the loop badly; the
   auxiliary observations resolve those ambiguities.
4. Decode the emitted bytes again and compare all first-pass and linked loop
   samples. Any mismatch fails closed. There is no approximate fallback.

Scales 13–15 have the same exceptional DSP decode behavior; the encoder uses
13 as their canonical representative. More generally several BRR byte streams
can produce the same PCM. Consequently **signal-exact is not byte-exact retail
ROM recovery**, even though the recorded audio error is zero. The ninth marker
byte is not misinterpreted as a surviving original BRR data-byte constraint.
Arbitrary unrecorded playback histories are not exhaustively verified.

### Cartridge headers, checksums, and extensions

- Validate one unambiguous LoROM/HiROM header, reset vector, ROM-size exponent,
  complementary checksum fields, and recognized mapping. Native half-width
  katakana titles are represented as valid UTF-8 in JSON metadata.
- Support 32 KiB–8 MiB bank-aligned ROMs and SNES recursive address-mirroring
  checksums, including the supplied 1.5, 2.5, and 3 MiB payloads.
- For PCM restoration, do **not** demand equality to the original retail
  checksum: canonical signal-equivalent BRR may differ byte-for-byte. Record
  the original field and whether the restored output matches it, recompute
  checksum/complement, and independently verify the resulting mirrored sum.
  Unchanged non-PCM payloads still require their original computed checksum.
- Two observed overlong stock titles occupy the cartridge-map byte: Contra III
  (`0x1036`, exact 1 MiB/title/map signature) and Japanese Super Formation Soccer
  (`0x1240`, exact 512 KiB/title/map signature). Only these combinations are
  normalized to LoROM `0x20`. Soccer's unmodified payload checksum is verified
  first. Unknown malformed/modified cartridge headers remain unsupported.
- Super Mario RPG uses **SA-1**, not S-DD1. Its supplied extension contains an
  EOF pointer followed by twelve zero bytes: there is no compressed extension
  payload to restore. Validate that complete empty structure. **Nonempty S-DD1
  or unknown extensions remain unsupported**; no recompressor is bundled.
- No game instruction patches are reversed or guessed. Structural acceptance
  is not authentication of all non-audio game code, nor verification of unknown
  Canoe-specific patches. A newly encountered unresolved layout/header or audio
  modification fails rather than producing an unverified partial conversion.

## Status and metadata contract

| Status | Meaning |
|---|---|
| `converted-validated-non-pcm` | Unchanged payload, original checksum verified |
| `converted-restored-pcm` | Restored BRR verified against supplied PCM/loop records |
| `converted-normalized-header` | Recognized non-PCM stock-header normalization |

Existing metadata keys are retained. Additional fields distinguish
`pcm_signal_exact`, `retail_byte_exact_verified` (false),
`original_checksum_matches`, `checksum_recomputed`, `header_map_normalized`,
`empty_extension_validated`, and `non_audio_patches_verified` (false).
`brr_blocks`, `brr_auxiliary_records`, `brr_streams`, and `brr_loop_blocks` record
coverage; inexact/error counts are zero on every successful restoration.
`rom_payload_unchanged` is false when BRR or the header was changed.

No firmware is extracted or bundled. Super FX requires no external firmware;
other coprocessor firmware, if required by ares, must come from the user.

## Supplied EUR title coverage

All these PCM titles pass exact-signal restoration, including recorded loops:

| Game ID | Title | BRR blocks | Loop variants |
|---|---|---:|---:|
| SAAAE | Super Mario World | 3153 | 34 |
| SAABE | F-ZERO | 2728 | 122 |
| SAAEE | The Legend of Zelda: A Link to the Past | 3474 | 94 |
| SAAFE | Super Mario Kart | 3932 | 56 |
| SAAHE | Super Metroid | 49180 | 269 |
| SAAJE | EarthBound | 71080 | 916 |
| SAAKE | Kirby's Dream Course | 3557 | 136 |
| SAALE | Donkey Kong Country | 7937 | 2 |
| SAAQE | Kirby Super Star | 12263 | 162 |
| SAAXE | Super Punch-Out!! | 28767 | 12 |
| SABCE | Mega Man X | 4550 | 24 |
| SABDE | Super Ghouls'n Ghosts | 4713 | 85 |
| SABHE | Street Fighter II Turbo | 23061 | 83 |
| SABQE | Super Mario RPG | 27217 | 302 |
| SABRE | Secret of Mana | 11384 | 56 |
| SABTE | Final Fantasy III | 22180 | 102 |
| SACBE | Super Castlevania IV | 970 | 7 |
| SACCE | Contra III: The Alien Wars | 9404 | 53 |

Star Fox, Yoshi's Island, and Star Fox 2 remain unchanged non-PCM conversions.
Japanese-only PCM titles and their regional variants also passed the full
126-file audit; see the per-file metadata rather than assuming ROM revisions
are interchangeable.

## Tests and reproducibility

No commercial game bytes are embedded in the self-authored synthetic fixtures.

```sh
c++ -std=c++17 -O1 -g -Wall -Wextra -Wpedantic -fsanitize=address,undefined \
  -Ithird_party/vendor portable/import/sfrom.cpp portable/import/sfrom_tests.cpp \
  -o .build/sfrom-restoration/sfrom-tests
.build/sfrom-restoration/sfrom-tests
.build/sfrom-restoration/sfrom-tests analysis/eur/usr/share/games/*/*.sfrom
python3 .build/sfrom-restoration/extract-fixtures.py
.build/sfrom-restoration/sfrom-tests .build/sfrom-restoration/regions/*/*.sfrom
```

Synthetic tests cover golden decoder values, all four filters, nonzero predictor
history, scales 0/1/5/10/12/13/15, saturation, END/LOOP, repeated loop histories,
backward and non-block-aligned direct offsets, stale high index bits, independent
checksums (including mirrored non-power-of-two sizes), exact non-PCM extraction,
malformed sections/records/chains, odd impossible PCM, header/flag errors, empty
and nonempty extensions, and Japanese-title JSON serialization.

## Research and licensing boundary

The earlier conservative importer rejected PCM because it had not established
auxiliary indexing, histories, or loop constraints. That limitation is superseded
by the complete sample comparisons above, not by enabling header stripping.

Research references retained for provenance:

- [anpage's SFROM description](https://gist.github.com/anpage/c1085055db0242ea3c7558dab56712a5).
- [Plombo/vcromclaim `snesrestore.py`](https://github.com/Plombo/vcromclaim/blob/master/snesrestore.py)
  and [`brrencode3.py`](https://github.com/Plombo/vcromclaim/blob/master/brrencode3.py):
  approximate restoration and offset heuristics, not this implementation.
- [AwakeEchidna/wiiu-vc-extractor](https://github.com/AwakeEchidna/wiiu-vc-extractor),
  GPL project, previously researched; its implementation was **not copied**.
- The local ares DSP BRR decoder was used as a decode-spec cross-check; the new
  converter expresses the arithmetic independently using defined integer floor
  division. The independent test oracle uses exact binary-fraction floors.

The new restoration algorithm and synthetic fixtures are original implementation
work. No third-party restoration/encoder source was incorporated, no GPL
restoration dependency is introduced, and no Nintendo executable, audio payload,
ROM, firmware, or replacement patch is distributed by this implementation.
