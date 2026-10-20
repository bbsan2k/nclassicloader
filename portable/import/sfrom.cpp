#include "sfrom.hpp"

#include <algorithm>
#include <array>
#include <cstdlib>
#include <limits>
#include <stdexcept>

namespace clover::importer {
namespace {

[[noreturn]] void malformed(const std::string& reason) {
  throw std::runtime_error("SFROM malformed: " + reason);
}

[[noreturn]] void unsupported(const std::string& reason) {
  throw std::runtime_error("SFROM unsupported: " + reason);
}

// All callers establish the enclosing range before using these readers. Avoid
// casts to packed structs (alignment, endianness and integer overflow matter).
uint32_t le32(const std::vector<uint8_t>& b, size_t p) {
  return uint32_t(b[p]) | (uint32_t(b[p + 1]) << 8) |
         (uint32_t(b[p + 2]) << 16) | (uint32_t(b[p + 3]) << 24);
}

uint16_t le16(const std::vector<uint8_t>& b, size_t p) {
  return uint16_t(b[p]) | (uint16_t(b[p + 1]) << 8);
}

bool printable(const std::vector<uint8_t>& b, size_t begin, size_t end) {
  return std::all_of(b.begin() + begin, b.begin() + end,
                     [](uint8_t c) { return c >= 0x20 && c <= 0x7e; });
}

bool headerValid(const std::vector<uint8_t>& b, size_t start, size_t size,
                  size_t h, uint16_t checksum, bool checkSum = true) {
  if (h > size || size - h < 64) return false;
  h += start;
  if (!std::all_of(b.begin() + h, b.begin() + h + 21,
      [](uint8_t c) { return (c >= 0x20 && c <= 0x7e) || (c >= 0xa1 && c <= 0xdf); })) return false;
  const uint8_t map = b[h + 21];
  const bool hirom = map == 0x21 || map == 0x31;
  const bool lorom = map == 0x20 || map == 0x30 || map == 0x23 ||
                     map == 0x33 || map == 0x32;
  if ((!hirom && !lorom) || (hirom != (h - start == 0xffc0))) return false;
  const uint8_t exponent = b[h + 23];
  if (exponent < 5 || exponent > 13 || (size_t(1024) << exponent) < size ||
      (size_t(1024) << exponent) / 2 >= size)
    return false;
  if (le16(b, h + 60) < 0x8000) return false;
  return (le16(b, h + 28) ^ le16(b, h + 30)) == 0xffff &&
         (!checkSum || le16(b, h + 30) == checksum);
}

uint32_t le24(const std::vector<uint8_t>& b, size_t p) {
  return uint32_t(b[p]) | (uint32_t(b[p + 1]) << 8) | (uint32_t(b[p + 2]) << 16);
}

// Arithmetic floor division, including negative inputs; no signed shifts or
// overflowing signed casts are used in this independent DSP-spec decoder.
int floorDiv(int a, int b) { return a >= 0 ? a / b : -((-a + b - 1) / b); }
int decodeNibble(int nibble, unsigned scale, unsigned filter, int p1, int p2) {
  int s = scale <= 12 ? floorDiv(nibble * (1 << scale), 2) : (nibble < 0 ? -2048 : 0);
  if (filter == 1) s += floorDiv(p1, 2) + floorDiv(-p1, 32);
  if (filter == 2) s += p1 - floorDiv(p2, 2) + floorDiv(floorDiv(p2, 2), 16) + floorDiv(-3 * p1, 64);
  if (filter == 3) s += p1 - floorDiv(p2, 2) + floorDiv(-13 * p1, 128) + floorDiv(3 * floorDiv(p2, 2), 16);
  s = std::clamp(s, -32768, 32767) * 2;
  if (s > 32767) s -= 65536;
  if (s < -32768) s += 65536;
  return s;
}

uint16_t romChecksum(const std::vector<uint8_t>& rom) {
  // SNES address mirroring recursively repeats the final partial bank group.
  size_t span = 1; while (span < rom.size()) span *= 2;
  uint32_t sum = 0;
  for (size_t address = 0; address < span; ++address) {
    size_t a = address, base = 0, remaining = rom.size(), mask = span;
    while (a >= remaining) {
      while (mask > a) mask >>= 1;
      a -= mask;
      if (remaining > mask) { remaining -= mask; base += mask; }
    }
    sum += rom[base + a];
  }
  return uint16_t(sum);
}

struct AudioReport {
  size_t blocks = 0, auxiliary = 0, resets = 0;
  size_t inexact = 0;
  uint64_t squaredError = 0;
  int maxError = 0;
  size_t loopBlocks = 0, loopInexact = 0;
  int loopMaxError = 0;
};

AudioReport restoreAudio(const std::vector<uint8_t>& bytes, std::vector<uint8_t>& rom,
                         size_t pcmStart, size_t pcmEnd, size_t footer) {
  if (pcmStart == pcmEnd || ((pcmEnd - pcmStart) & 1) || (footer - pcmEnd) % 6)
    malformed("external PCM / PCMF audio section alignment or missing samples");
  const size_t samples = (pcmEnd - pcmStart) / 2, entries = (footer - pcmEnd) / 6;
  if (entries > 65536 || samples > 16 * 1024 * 1024)
    unsupported("external PCM exceeds stock profile limits");
  auto sample = [&](size_t index) {
    const size_t p = pcmStart + index * 2;
    const unsigned value = (unsigned(bytes[p]) << 8) | bytes[p + 1];
    return value < 32768 ? int(value) : int(value) - 65536;
  };
  AudioReport report;
  size_t previous = rom.size();
  int p1 = 0, p2 = 0;
  bool ended = true;
  size_t chainWork = 0;
  for (size_t p = 0; p + 4 <= rom.size(); ++p) {
    if (rom[p] != 'P' || rom[p + 1] != 'C' || rom[p + 2] != 'M' || rom[p + 3] != 'F') continue;
    if (rom.size() - p < 9) malformed("truncated PCMF record");
    const uint8_t control = rom[p + 7];
    if (control & 0xe0) unsupported("unknown PCMF control bits");
    size_t offset = le24(rom, p + 4);
    size_t auxiliaryIndex = entries;
    if (control & 0x10) {
      // Stock format observation: bit 4 replaces only offset bits 0..15
      // with a table index; offset bits 16..23 are stale, not index bits.
      // Six-byte table entries: identity byte, next index u16, sample u24.
      // Linked entries describe the same BRR block under later loop histories.
      const size_t index = le16(rom, p + 4);
      auxiliaryIndex = index;
      if (index >= entries) malformed("PCMF auxiliary index outside table");
      const size_t entry = pcmEnd + index * 6;
      if (bytes[entry] != rom[p + 8]) malformed("PCMF auxiliary identity mismatch");
      offset = le24(bytes, entry + 3);
      ++report.auxiliary;
    }
    if (offset > samples || samples - offset < 16) malformed("PCMF sample offset outside PCM");
    std::array<int, 16> target{};
    for (size_t i = 0; i < 16; ++i) target[i] = sample(offset + i);
    std::vector<size_t> alternates;
    if (auxiliaryIndex != entries) {
      size_t index = le16(bytes, pcmEnd + auxiliaryIndex * 6 + 1);
      while (index) {
        if (index >= entries || alternates.size() >= entries)
          malformed("PCMF auxiliary chain is cyclic or out of range");
        if (++chainWork > entries * 16) unsupported("PCMF auxiliary reference budget exceeded");
        const size_t alternate = le24(bytes, pcmEnd + index * 6 + 3);
        if (alternate < 2 || alternate > samples || samples - alternate < 16)
          malformed("PCMF alternate sample offset outside PCM");
        alternates.push_back(alternate);
        index = le16(bytes, pcmEnd + index * 6 + 1);
      }
    }
    if (ended || previous + 9 != p) { p1 = p2 = 0; ++report.resets; }
    const unsigned filter = (control >> 2) & 3;
    std::array<uint8_t, 9> best{};
    bool found = false;
    // Exhaustive inversion of the DSP decoder, not an imported BRR encoder.
    // Saturation can hide the original nibble; intersect ALL recorded histories
    // before selecting it. An exact output sample fixes the next predictor state,
    // so choosing any surviving nibble cannot lose a later exact solution.
    for (unsigned scale = 0; scale <= 13; ++scale) {
      std::array<uint8_t, 9> block{};
      block[0] = uint8_t((scale << 4) | (control & 15));
      int a = p1, b = p2;
      bool exact = true;
      for (size_t i = 0; i < 16; ++i) {
        int chosen = 0;
        bool matched = false;
        for (int n = -8; n <= 7; ++n) {
          if (decodeNibble(n, scale, filter, a, b) != target[i]) continue;
          bool matches = true;
          for (size_t alternate : alternates) {
            const int v = decodeNibble(n, scale, filter, sample(alternate + i - 1), sample(alternate + i - 2));
            if (v != sample(alternate + i)) { matches = false; break; }
          }
          if (matches) { chosen = n; matched = true; break; }
        }
        if (!matched) { exact = false; break; }
        block[1 + i / 2] |= uint8_t((chosen & 15) << (i % 2 ? 0 : 4));
        b = a; a = target[i];
      }
      if (exact) { best = block; found = true; break; }
    }
    if (!found) unsupported("PCMF block cannot be restored exactly against first-pass and loop PCM at ROM offset " + std::to_string(p));
    ++report.blocks;
    // Decode the emitted bytes again, rather than declaring success merely
    // because the encoder found a candidate. Preserve actual stream history.
    for (size_t i = 0; i < 16; ++i) {
      int n = (best[1 + i / 2] >> (i % 2 ? 0 : 4)) & 15;
      if (n >= 8) n -= 16;
      const int v = decodeNibble(n, best[0] >> 4, filter, p1, p2);
      if (v != target[i]) malformed("BRR first-pass verification failed");
      p2 = p1; p1 = v;
    }
    for (size_t alternate : alternates) {
      int a = sample(alternate - 1), b = sample(alternate - 2), maximum = 0;
      for (size_t i = 0; i < 16; ++i) {
        int n = (best[1 + i / 2] >> (i % 2 ? 0 : 4)) & 15;
        if (n >= 8) n -= 16;
        const int value = decodeNibble(n, best[0] >> 4, filter, a, b);
        maximum = std::max(maximum, std::abs(value - sample(alternate + i)));
        b = a; a = value;
      }
      ++report.loopBlocks;
      if (maximum) ++report.loopInexact;
      report.loopMaxError = std::max(report.loopMaxError, maximum);
    }
    if (report.loopInexact) unsupported("BRR loop verification failed");
    std::copy(best.begin(), best.end(), rom.begin() + p);
    previous = p; ended = control & 1;
    p += 8;
  }
  if (!report.blocks) unsupported("external PCM / PCMF audio has no records");
  return report;
}

}  // namespace

Conversion convertSFROM(const std::vector<uint8_t>& bytes) {
  constexpr size_t headerSize = 48;
  constexpr size_t footerMinimum = 36;
  if (bytes.size() < headerSize) malformed("truncated 48-byte header");
  if (bytes.size() > std::numeric_limits<uint32_t>::max())
    malformed("container exceeds 32-bit format limits");
  if (le32(bytes, 0) != 0x100) unsupported("unrecognized container version");
  if (le32(bytes, 4) != bytes.size()) malformed("declared file size differs from input size");

  const size_t romStart = le32(bytes, 8);
  const size_t romEnd = le32(bytes, 12);
  const size_t pcmEnd = le32(bytes, 16);
  const size_t footer = le32(bytes, 20);
  const size_t extension = le32(bytes, 24);
  const size_t footerFlag = le32(bytes, 32);
  if (romStart != headerSize)
    unsupported("only the official trailing-footer, 48-byte-header layout is supported");
  if (!(romStart < romEnd && romEnd <= pcmEnd && pcmEnd <= footer && footer <= bytes.size()))
    malformed("ROM / PCM / PCM-footer offsets are unordered or outside the container");
  if (bytes.size() - footer < footerMinimum) malformed("truncated footer");
  if (extension < footer || extension > bytes.size()) malformed("extension offset outside footer");
  if (footerFlag < footer || footerFlag >= bytes.size()) malformed("footer flag offset outside footer");
  if (le32(bytes, 28) != 0 || le32(bytes, 44) != 0)
    unsupported("unknown nonzero header flags");
  if (!printable(bytes, 36, 44)) malformed("non-ASCII Virtual Console game ID");

  const size_t romSize = romEnd - romStart;
  const size_t pcmSize = pcmEnd - romEnd;
  const size_t pcmFooterSize = footer - pcmEnd;
  if (le32(bytes, footer + 1) != romSize ||
      le32(bytes, footer + 5) != pcmSize ||
      le32(bytes, footer + 9) != pcmFooterSize)
    malformed("footer sizes disagree with header section boundaries");

  size_t markers = 0;
  for (size_t p = romStart; romEnd - p >= 4; ++p) {
    if (bytes[p] == 'P' && bytes[p + 1] == 'C' &&
        bytes[p + 2] == 'M' && bytes[p + 3] == 'F') ++markers;
  }
  const bool audio = pcmSize || pcmFooterSize || markers;
  bool emptyExtension = extension + 16 == bytes.size() &&
                        le32(bytes, extension) == bytes.size();
  if (emptyExtension)
    for (size_t p = extension + 4; p < bytes.size(); ++p) emptyExtension &= bytes[p] == 0;
  if (extension != bytes.size() && !emptyExtension)
    unsupported("S-DD1 data or an unknown footer extension requires restoration");
  if (footerFlag != footer + 27)
    unsupported("unknown footer flag layout");
  // Accept the observed stock footer lengths and flags only. Nonempty S-DD1
  // extensions remain unsupported; an EOF pointer plus zeros is not S-DD1 data.
  if ((!audio && bytes.size() - footer != 48) || extension - footer < 32 || extension - footer > 64)
    unsupported("unrecognized non-PCM footer profile");
  const uint16_t preset = le16(bytes, footer + 13);
  const bool alternateFooter = preset == 0x10b0 || preset == 0x10b2 ||
                               preset == 0x10dc || preset == 0x10dd;
  for (size_t p = 19; p < extension - footer; ++p) {
    if (bytes[footer + p] != ((p == 31 || (p == 27 && !alternateFooter)) ? 1 : 0))
      unsupported("unrecognized footer flags");
  }
  if (bytes[footer] != 60 && bytes[footer] != 50)
    unsupported("unrecognized footer frame rate");
  if (bytes[footer + 17] != 0x14 && bytes[footer + 17] != 0x15)
    unsupported("unrecognized footer ROM mapping");
  if (bytes[footer + 18] != 0 && bytes[footer + 18] != 0x0c)
    unsupported("unrecognized footer enhancement-chip flags");
  if (romSize < 32768 || romSize > 8 * 1024 * 1024 || romSize % 32768)
    unsupported("only standard ROM payloads from 32 KiB to 8 MiB are supported");

  std::vector<uint8_t> rom(bytes.begin() + romStart, bytes.begin() + romEnd);
  // The stock Contra III header has a 22-character title occupying the map
  // byte. Normalize only this observed preset/header combination to LoROM.
  const bool contraHeader = audio && le16(bytes, footer + 13) == 0x1036 &&
      romSize == 1048576 && rom[0x7fd5] == 0x53 &&
      std::string(rom.begin() + 0x7fc0, rom.begin() + 0x7fd5) == "CONTRA3 THE ALIEN WAR";
  const bool soccerHeader = !audio && preset == 0x1240 && romSize == 524288 &&
      rom[0x7fd5] == 0x52 &&
      std::string(rom.begin() + 0x7fc0, rom.begin() + 0x7fd5) == "SUPER FORMATION SOCCE";
  if (soccerHeader && romChecksum(rom) != le16(rom, 0x7fde))
    unsupported("Super Formation Soccer payload checksum mismatch");
  const bool headerRepaired = contraHeader || soccerHeader;
  if (headerRepaired) rom[0x7fd5] = 0x20;
  AudioReport audioReport;
  if (audio) audioReport = restoreAudio(bytes, rom, romEnd, pcmEnd, footer);
  if (audio && audioReport.blocks != markers) malformed("overlapping PCMF markers");
  auto checksum = romChecksum(rom);
  size_t header = 0;
  for (size_t candidate : {size_t(0x7fc0), size_t(0xffc0)}) {
    if (!headerValid(rom, 0, romSize, candidate, checksum, !audio && !headerRepaired)) continue;
    if (header) unsupported("ambiguous SNES cartridge headers");
    header = candidate;
  }
  if (!header)
    unsupported("no supported standard SNES header with matching full-ROM checksum; "
                "the payload may be patched, damaged, or use an unsupported mapping");

  std::string title;
  for (size_t p = header; p < header + 21; ++p) {
    const uint8_t c = rom[p];
    if (c < 0x80) title += char(c);
    else {
      // SNES half-width katakana header bytes, not arbitrary invalid UTF-8.
      const unsigned cp = 0xff61 + c - 0xa1;
      title += char(0xe0 | (cp >> 12));
      title += char(0x80 | ((cp >> 6) & 63)); title += char(0x80 | (cp & 63));
    }
  }
  while (!title.empty() && title.back() == ' ') title.pop_back();
  const uint8_t cartridgeType = bytes[romStart + header + 22];
  const bool superFX = cartridgeType == 0x13 || cartridgeType == 0x14 ||
                       cartridgeType == 0x15 || cartridgeType == 0x1a;
  if (superFX != (bytes[footer + 18] == 0x0c))
    unsupported("footer enhancement flags disagree with the cartridge header");

  Conversion result;
  const uint16_t originalChecksum = le16(rom, header + 30);
  const bool originalMatch = checksum == originalChecksum;
  if (audio || headerRepaired) {
    rom[header + 28] = uint8_t(~checksum); rom[header + 29] = uint8_t(uint16_t(~checksum) >> 8);
    rom[header + 30] = uint8_t(checksum); rom[header + 31] = uint8_t(checksum >> 8);
    if (romChecksum(rom) != checksum) malformed("restored checksum is not stable");
  }
  result.rom = std::move(rom);
  result.status = audio ? "converted-restored-pcm" :
                  headerRepaired ? "converted-normalized-header" : "converted-validated-non-pcm";
  result.metadata = {
      {"format", "sfrom"}, {"conversion", result.status},
      {"rom_bytes", romSize}, {"rom_start", romStart}, {"rom_end", romEnd},
      {"pcm_bytes", pcmSize}, {"pcm_footer_bytes", pcmFooterSize}, {"pcmf_markers", markers},
      {"footer_start", footer}, {"preset_id", le16(bytes, footer + 13)},
      {"virtual_console_id", std::string(bytes.begin() + 36, bytes.begin() + 44)},
      {"title", title}, {"map_mode", result.rom[header + 21]},
      {"cartridge_type", cartridgeType}, {"super_fx", superFX},
      {"checksum", checksum}, {"checksum_verified", true}, {"rom_payload_unchanged", !audio && !headerRepaired},
      {"original_checksum", originalChecksum}, {"original_checksum_matches", originalMatch},
      {"pcm_restored", audio}, {"patches_reversed", false}, {"runtime_tested", false},
      {"brr_blocks", audioReport.blocks}, {"brr_inexact_blocks", audioReport.inexact},
      {"brr_max_sample_error", audioReport.maxError}, {"brr_squared_error", audioReport.squaredError},
      {"brr_auxiliary_records", audioReport.auxiliary}, {"brr_streams", audioReport.resets},
      {"brr_loop_blocks", audioReport.loopBlocks}, {"brr_loop_inexact_blocks", audioReport.loopInexact},
      {"brr_loop_max_sample_error", audioReport.loopMaxError}, {"header_map_normalized", headerRepaired},
      {"pcm_signal_exact", audio}, {"retail_byte_exact_verified", false},
      {"non_audio_patches_verified", false},
      {"checksum_recomputed", audio || headerRepaired}, {"empty_extension_validated", emptyExtension},
      {"firmware_policy", "No firmware is extracted or bundled; any coprocessor firmware required by ares must be supplied by the user."},
      {"compatibility", "Validated standard ROM structure, not a claim of runtime or byte-identical retail-ROM verification."}};
  return result;
}

}  // namespace clover::importer
