// Self-authored synthetic fixtures: no commercial ROM bytes are embedded here.
#include "sfrom.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <iterator>
#include <stdexcept>

using clover::importer::convertSFROM;
using Bytes = std::vector<uint8_t>;

namespace {
void require(bool value, const std::string& message) {
  if (!value) throw std::runtime_error(message);
}

void put32(Bytes& b, size_t p, uint32_t v) {
  for (unsigned i = 0; i < 4; ++i) b.at(p + i) = uint8_t(v >> (i * 8));
}

uint32_t get32(const Bytes& b, size_t p) {
  uint32_t v = 0;
  for (unsigned i = 0; i < 4; ++i) v |= uint32_t(b.at(p + i)) << (i * 8);
  return v;
}

unsigned get24(const Bytes& b, size_t p) {
  return b.at(p) | (unsigned(b.at(p + 1)) << 8) | (unsigned(b.at(p + 2)) << 16);
}

void put24(Bytes& b, size_t p, unsigned value) {
  for (unsigned i = 0; i < 3; ++i) b.at(p + i) = uint8_t(value >> (i * 8));
}

uint64_t mirroredSum(const Bytes& b, size_t begin, size_t length) {
  size_t power = 1; while (power * 2 <= length) power *= 2;
  uint64_t sum = 0;
  for (size_t i = 0; i < power; ++i) sum += b.at(begin + i);
  if (length > power) {
    size_t tailSpan = 1; while (tailSpan < length - power) tailSpan *= 2;
    sum += mirroredSum(b, begin + power, length - power) * (power / tailSpan);
  }
  return sum;
}

void verifyChecksum(const Bytes& rom) {
  bool matched = false;
  const uint16_t sum = uint16_t(mirroredSum(rom, 0, rom.size()));
  for (size_t h : {size_t(0x7fc0), size_t(0xffc0)}) {
    if (h + 64 > rom.size()) continue;
    const unsigned stored = rom[h + 30] | (unsigned(rom[h + 31]) << 8);
    const unsigned inverse = rom[h + 28] | (unsigned(rom[h + 29]) << 8);
    if (stored == sum && (stored ^ inverse) == 65535) matched = true;
  }
  require(matched, "independent mirrored ROM checksum mismatch");
}

// Independent test oracle: floating-point floor of exact binary fractions,
// rather than the converter's integer floor-division implementation.
std::array<int, 16> decode(const Bytes& b, size_t p, int& h1, int& h2) {
  std::array<int, 16> out{};
  const unsigned shift = b.at(p) >> 4, filter = (b.at(p) >> 2) & 3;
  for (unsigned i = 0; i < 16; ++i) {
    int n = (b.at(p + 1 + i / 2) >> (i % 2 ? 0 : 4)) & 15;
    if (n > 7) n -= 16;
    double v = shift <= 12 ? std::floor(n * std::pow(2., double(shift) - 1.)) : (n < 0 ? -2048 : 0);
    if (filter == 1) v += std::floor(h1 / 2.) + std::floor(-h1 / 32.);
    if (filter == 2) v += h1 - h2 / 2. + std::floor(h2 / 32.) + std::floor(-3. * h1 / 64.);
    if (filter == 3) v += h1 - h2 / 2. + std::floor(-13. * h1 / 128.) + std::floor(3. * h2 / 32.);
    int s = 2 * std::clamp(int(v), -32768, 32767);
    if (s >= 32768) s -= 65536;
    if (s < -32768) s += 65536;
    out[i] = s; h2 = h1; h1 = s;
  }
  return out;
}

int pcmSample(const Bytes& b, size_t start, size_t index) {
  const unsigned v = (unsigned(b.at(start + index * 2)) << 8) | b.at(start + index * 2 + 1);
  return v < 32768 ? int(v) : int(v) - 65536;
}

void verifyAudio(const Bytes& container, const Bytes& rom) {
  const size_t pcm = get32(container, 12), aux = get32(container, 16);
  int h1 = 0, h2 = 0; size_t previous = rom.size(); bool ended = true;
  for (size_t p = 0; p + 9 <= rom.size(); ++p) {
    if (!std::equal(container.begin() + 48 + p, container.begin() + 48 + p + 4, "PCMF")) continue;
    const unsigned control = container.at(48 + p + 7);
    require((rom[p] & 15) == (control & 15), "filter / END / LOOP changed");
    size_t offset = get24(container, 48 + p + 4), link = 0;
    if (control & 16) {
      const size_t index = offset & 65535;
      offset = get24(container, aux + index * 6 + 3);
      link = get24(container, aux + index * 6) >> 8;
    }
    if (ended || previous + 9 != p) h1 = h2 = 0;
    const auto actual = decode(rom, p, h1, h2);
    for (size_t i = 0; i < 16; ++i)
      require(actual[i] == pcmSample(container, pcm, offset + i), "independent first-pass PCM mismatch");
    while (link) {
      const size_t alternate = get24(container, aux + link * 6 + 3);
      int a = pcmSample(container, pcm, alternate - 1), b = pcmSample(container, pcm, alternate - 2);
      const auto loop = decode(rom, p, a, b);
      for (size_t i = 0; i < 16; ++i)
        require(loop[i] == pcmSample(container, pcm, alternate + i), "independent loop PCM mismatch");
      link = get24(container, aux + link * 6) >> 8;
    }
    previous = p; ended = control & 1; p += 8;
  }
}

Bytes fixture(bool hirom = false) {
  const uint32_t size = hirom ? 65536 : 32768;
  const size_t h = 48 + (hirom ? 0xffc0 : 0x7fc0);
  const uint32_t footer = 48 + size;
  Bytes b(footer + 48, 0);
  put32(b, 0, 0x100);
  put32(b, 4, uint32_t(b.size()));
  put32(b, 8, 48);
  put32(b, 12, footer);
  put32(b, 16, footer);
  put32(b, 20, footer);
  put32(b, 24, uint32_t(b.size()));
  put32(b, 32, footer + 27);
  const std::string id = "WUP-TEST";
  std::copy(id.begin(), id.end(), b.begin() + 36);
  std::fill(b.begin() + h, b.begin() + h + 21, ' ');
  const std::string title = "SELF AUTHORED TEST";
  std::copy(title.begin(), title.end(), b.begin() + h);
  b[h + 21] = hirom ? 0x21 : 0x20;
  b[h + 23] = hirom ? 6 : 5;
  b[h + 61] = 0x80;
  b[h + 28] = b[h + 29] = 0xff;
  uint32_t sum = 0;
  for (size_t p = 48; p < footer; ++p) sum += b[p];
  const uint16_t checksum = uint16_t(sum);
  b[h + 28] = uint8_t(~checksum);
  b[h + 29] = uint8_t((~checksum) >> 8);
  b[h + 30] = uint8_t(checksum);
  b[h + 31] = uint8_t(checksum >> 8);
  b[footer] = 60;
  put32(b, footer + 1, size);
  b[footer + 15] = 2;
  b[footer + 16] = 100;
  b[footer + 17] = hirom ? 0x15 : 0x14;
  b[footer + 27] = b[footer + 31] = 1;
  return b;
}

// Self-authored nonzero DSP histories, all four filters, END/LOOP, saturation,
// a table-index record with stale high address bits and three loop passes.
Bytes audioFixture(unsigned scale = 12) {
  auto b = fixture();
  const size_t romEnd = get32(b, 12);
  Bytes pcm;
  auto append = [&](int s) { pcm.push_back(uint8_t((unsigned(s) & 65535) >> 8)); pcm.push_back(uint8_t(s)); };
  // Padding deliberately isn't a valid predictor for the first stream.
  append(1234); append(-4322);
  Bytes originals(5 * 9);
  const std::array<unsigned, 5> filters{0, 1, 2, 3, 2};
  std::array<size_t, 5> offsets{};
  int h1 = 0, h2 = 0;
  for (size_t k = 0; k < 5; ++k) {
    originals[k * 9] = uint8_t((scale << 4) | (filters[k] << 2) | (k == 4 ? 3 : 2));
    for (size_t i = 1; i < 9; ++i) originals[k * 9 + i] = uint8_t((k * 53 + i * 37) & 255);
    offsets[k] = pcm.size() / 2;
    const auto values = decode(originals, k * 9, h1, h2);
    for (int s : values) append(s);
    const size_t p = 48 + 256 + k * 9;
    std::copy_n("PCMF", 4, b.begin() + p);
    put24(b, p + 4, unsigned(offsets[k]));
    b[p + 7] = originals[k * 9] & 15;
    b[p + 8] = 0xa5;  // identity byte, not a BRR nibble constraint
  }
  Bytes auxiliary(3 * 6, 0);
  auxiliary[0] = 0xa5; auxiliary[1] = 1;
  put24(auxiliary, 3, unsigned(offsets[4]));
  for (size_t pass = 1; pass <= 2; ++pass) {
    put24(auxiliary, pass * 6 + 3, unsigned(pcm.size() / 2));
    if (pass == 1) auxiliary[pass * 6 + 1] = 2;
    const auto values = decode(originals, 4 * 9, h1, h2);
    for (int s : values) append(s);
  }
  const size_t special = 48 + 256 + 4 * 9;
  put24(b, special + 4, 0x070000); b[special + 7] |= 16;
  // A filter-one stream needs a real zero-history reset after END. A second
  // copy after a ROM gap then references this PCM backwards, without END on
  // the preceding stream: both reset causes are independently exercised.
  const size_t restart = special + 9;
  const unsigned restartOffset = unsigned(pcm.size() / 2);
  originals[0] = uint8_t((scale << 4) | 4);
  h1 = h2 = 0;
  const auto restarted = decode(originals, 0, h1, h2);
  for (int s : restarted) append(s);
  std::copy_n("PCMF", 4, b.begin() + restart);
  put24(b, restart + 4, restartOffset); b[restart + 7] = 4;
  std::copy_n("PCMF", 4, b.begin() + restart + 16);
  put24(b, restart + 20, restartOffset); b[restart + 23] = 5;
  b.insert(b.begin() + romEnd, pcm.begin(), pcm.end());
  b.insert(b.begin() + romEnd + pcm.size(), auxiliary.begin(), auxiliary.end());
  const size_t footer = romEnd + pcm.size() + auxiliary.size();
  put32(b, 4, unsigned(b.size())); put32(b, 16, unsigned(romEnd + pcm.size()));
  put32(b, 20, unsigned(footer)); put32(b, 24, unsigned(b.size())); put32(b, 32, unsigned(footer + 27));
  put32(b, footer + 5, unsigned(pcm.size())); put32(b, footer + 9, unsigned(auxiliary.size()));
  return b;
}

void audioTests() {
  Bytes golden{0x10, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef};
  int a = 0, b = 0;
  require(decode(golden, 0, a, b) == std::array<int, 16>{0,2,4,6,8,10,12,14,-16,-14,-12,-10,-8,-6,-4,-2}, "golden filter-zero decode");
  golden[0] = 0x04; std::fill(golden.begin() + 1, golden.end(), 0); a = 16; b = 0;
  const auto decay = decode(golden, 0, a, b);
  require(decay[0] == 14 && decay[1] == 12 && decay[7] == 0, "golden predictor rounding");
  for (unsigned scale : {0u, 1u, 5u, 10u, 12u, 13u, 15u}) {
    const auto input = audioFixture(scale);
    const auto result = convertSFROM(input);
    require(result.status == "converted-restored-pcm", "audio conversion status");
    require(result.metadata.at("pcm_signal_exact") == true, "signal verification missing");
    require(result.metadata.at("retail_byte_exact_verified") == false, "unverified byte-exact claim");
    require(result.metadata.at("brr_blocks") == 7 && result.metadata.at("brr_loop_blocks") == 2, "audio counts");
    require(result.metadata.at("brr_streams") == 3, "END / ROM gap must reset predictor");
    verifyAudio(input, result.rom);
    verifyChecksum(result.rom);
  }
}

void rejects(const Bytes& bytes, const std::string& reason) {
  try {
    (void)convertSFROM(bytes);
  } catch (const std::runtime_error& e) {
    require(std::string(e.what()).find(reason) != std::string::npos,
            "wrong rejection: " + std::string(e.what()) + " (wanted " + reason + ")");
    return;
  }
  throw std::runtime_error("unexpected conversion success (wanted " + reason + ")");
}

void syntheticTests() {
  for (bool hirom : {false, true}) {
    const auto b = fixture(hirom);
    const auto result = convertSFROM(b);
    require(result.rom == Bytes(b.begin() + 48, b.end() - 48), "ROM is not an exact payload copy");
    require(result.status == "converted-validated-non-pcm", "wrong success status");
    require(result.metadata.at("checksum_verified") == true, "checksum not verified");
    require(result.metadata.at("runtime_tested") == false, "false runtime claim");
  }
  const auto good = fixture();
  const size_t f = get32(good, 20);
  // Three-bank payload: the last bank is mirrored, not simply summed once.
  auto mirrored = fixture(true);
  const size_t oldFooter = get32(mirrored, 20), h = 48 + 0xffc0;
  mirrored.insert(mirrored.begin() + oldFooter, 32768, 0x35);
  const size_t newFooter = oldFooter + 32768;
  mirrored[h + 23] = 7;
  mirrored[h + 28] = mirrored[h + 29] = 255; mirrored[h + 30] = mirrored[h + 31] = 0;
  const uint16_t sum = uint16_t(mirroredSum(mirrored, 48, 98304));
  mirrored[h + 28] = uint8_t(~sum); mirrored[h + 29] = uint8_t(uint16_t(~sum) >> 8);
  mirrored[h + 30] = uint8_t(sum); mirrored[h + 31] = uint8_t(sum >> 8);
  for (size_t field : {size_t(12), size_t(16), size_t(20)}) put32(mirrored, field, unsigned(newFooter));
  put32(mirrored, 4, unsigned(mirrored.size())); put32(mirrored, 24, unsigned(mirrored.size()));
  put32(mirrored, 32, unsigned(newFooter + 27)); put32(mirrored, newFooter + 1, 98304);
  verifyChecksum(convertSFROM(mirrored).rom);
  mirrored[oldFooter + 1] ^= 1; rejects(mirrored, "checksum");
  for (size_t length = 0; length < 48; ++length)
    rejects(Bytes(good.begin(), good.begin() + length), "truncated");
  auto b = good; put32(b, 4, 0xffffffff); rejects(b, "file size");
  b = good; put32(b, 0, 0); rejects(b, "version");
  b = good; put32(b, 8, 80); rejects(b, "layout");
  for (size_t field : {size_t(12), size_t(16), size_t(20)}) {
    b = good; put32(b, field, 0xffffffff); rejects(b, "offsets");
  }
  b = good; put32(b, 12, 47); rejects(b, "offsets");
  b = good; put32(b, 16, uint32_t(f - 1)); rejects(b, "offsets");
  b = good; put32(b, 24, 0xffffffff); rejects(b, "extension offset");
  b = good; put32(b, 32, 0); rejects(b, "flag offset");
  b = good; b.resize(f + 35); put32(b, 4, uint32_t(b.size())); rejects(b, "truncated footer");
  b = good; put32(b, f + 1, 1); rejects(b, "sizes disagree");
  b = good; b[28] = 1; rejects(b, "header flags");
  b = good; b[36] = 0xff; rejects(b, "game ID");
  b = good; b[f + 19] = 1; rejects(b, "footer flags");
  b = good; b[f] = 1; rejects(b, "frame rate");
  b = good; b[f + 17] = 0; rejects(b, "ROM mapping");
  b = good; b[f + 18] = 1; rejects(b, "enhancement-chip");
  b = good; b[100] ^= 1; rejects(b, "checksum");
  b = good; put32(b, 24, uint32_t(f + 44)); rejects(b, "S-DD1");
  b = good; put32(b, 32, uint32_t(f + 26)); rejects(b, "flag layout");
  b = good; b[f + 18] = 12; rejects(b, "disagree with the cartridge");
  // A marker without external PCM must not be mistaken for an ordinary ROM.
  b = good; b[100] = 'P'; b[101] = 'C'; b[102] = 'M'; b[103] = 'F';
  rejects(b, "PCMF audio");
  // Neither a missing marker nor an empty PCM footer makes external PCM safe.
  b = good;
  b.insert(b.begin() + f, 32, 0);
  put32(b, 4, uint32_t(b.size()));
  put32(b, 16, uint32_t(f + 32)); put32(b, 20, uint32_t(f + 32));
  put32(b, 24, uint32_t(b.size())); put32(b, 32, uint32_t(f + 32 + 27));
  put32(b, f + 32 + 5, 32);
  rejects(b, "has no records");
  // Exercise bounded parsing across truncated samples, with sizes adjusted so
  // the parser reaches the section/footer checks instead of just file size.
  for (size_t length = 48; length < good.size(); length += 131) {
    b.assign(good.begin(), good.begin() + length);
    put32(b, 4, uint32_t(length));
    rejects(b, "SFROM");
  }
  const auto audio = audioFixture();
  const size_t marker = 48 + 256 + 4 * 9, auxiliary = get32(audio, 16), pcm = get32(audio, 12);
  b = audio; b[marker + 7] |= 0x80; rejects(b, "control bits");
  b = audio; b[48 + 0x7fd5] = 0x53; rejects(b, "no supported standard SNES header");
  b = audio; b[48 + 0x7fdc] ^= 1; rejects(b, "no supported standard SNES header");
  b = audio; put24(b, marker + 4, 0xffff); rejects(b, "auxiliary index");
  b = audio; b[marker + 8] ^= 1; rejects(b, "identity mismatch");
  b = audio; put24(b, auxiliary + 3, 0xffffff); rejects(b, "sample offset");
  b = audio; b[auxiliary + 6 + 1] = 1; rejects(b, "cyclic");
  b = audio; b[auxiliary + 1] = 9; rejects(b, "out of range");
  b = audio; put24(b, auxiliary + 6 + 3, 1); rejects(b, "alternate sample offset");
  b = audio; b[pcm + 4 + 1] ^= 1; rejects(b, "cannot be restored exactly");
  b = audio; b.push_back(1); put32(b, 4, unsigned(b.size())); put32(b, 24, unsigned(b.size()));
  rejects(b, "footer flags");
  // Empty EOF-pointing extension is allowed, nonempty extension isn't.
  b = audio; const size_t oldEnd = b.size(); b.resize(oldEnd + 16);
  put32(b, 4, unsigned(b.size())); put32(b, 24, unsigned(oldEnd)); put32(b, oldEnd, unsigned(b.size()));
  require(convertSFROM(b).metadata.at("empty_extension_validated") == true, "empty extension rejected");
  b[oldEnd + 8] = 1; rejects(b, "S-DD1");
  // Japanese half-width header titles must produce valid JSON UTF-8.
  b = audio; b[48 + 0x7fc0] = 0xca;
  require(!convertSFROM(b).metadata.dump().empty(), "katakana header JSON");
  audioTests();
}

void suppliedTest(const std::filesystem::path& path) {
  std::ifstream file(path, std::ios::binary);
  require(bool(file), "cannot read " + path.string());
  Bytes b{std::istreambuf_iterator<char>(file), std::istreambuf_iterator<char>()};
  require(b.size() >= 48, "supplied fixture is truncated");
  const bool pcm = get32(b, 12) != get32(b, 20);
  const auto result = convertSFROM(b);
  verifyChecksum(result.rom);
  if (pcm) {
    verifyAudio(b, result.rom);
    require(result.metadata.at("brr_inexact_blocks") == 0 && result.metadata.at("brr_loop_inexact_blocks") == 0,
            "inexact supplied audio was accepted");
    std::cout << "ACCEPT (independently verified exact PCM) " << path.filename().string() << " "
              << result.metadata.at("brr_blocks") << " blocks, " << result.metadata.at("brr_loop_blocks") << " loop variants\n";
  } else if (result.status == "converted-validated-non-pcm") {
    require(result.rom == Bytes(b.begin() + 48, b.begin() + get32(b, 12)),
            "supplied ROM payload was altered");
    std::cout << "ACCEPT " << path.filename().string() << " " << result.rom.size() << " bytes\n";
  } else {
    require(result.status == "converted-normalized-header" && result.metadata.at("header_map_normalized") == true,
            "unrecognized non-PCM alteration");
    std::cout << "ACCEPT (normalized stock header) " << path.filename().string() << '\n';
  }
}
}  // namespace

int main(int argc, char** argv) {
  try {
    syntheticTests();
    for (int i = 1; i < argc; ++i) suppliedTest(argv[i]);
    std::cout << "SFROM tests passed (synthetic tests and " << argc - 1 << " local files).\n";
    return 0;
  } catch (const std::exception& e) {
    std::cerr << "SFROM tests FAILED: " << e.what() << '\n';
    return 1;
  }
}
