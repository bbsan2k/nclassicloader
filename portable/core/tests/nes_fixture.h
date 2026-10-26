#pragma once

#include <algorithm>
#include <cstddef>
#include <cstdint>
#include <initializer_list>
#include <vector>

namespace clover::test_fixture {

// Self-authored 6502 code only. Poll both NES serial buttons, paint the
// universal backdrop, play pulse 1, and record inputs plus a counter in SRAM.
// Code lives in the fixed final 8 KiB common to all tested stock mappers.
inline std::vector<uint8_t> nes_homebrew(unsigned mapper, bool battery = false, bool chrRam = false) {
  size_t chrSize = chrRam ? 0 : mapper == 184 ? 32768 : 8192;
  std::vector<uint8_t> rom(16 + 32768 + chrSize, 0);
  rom[0] = 'N'; rom[1] = 'E'; rom[2] = 'S'; rom[3] = 0x1a;
  rom[4] = 2; rom[5] = uint8_t(chrSize / 8192);
  rom[6] = uint8_t((mapper << 4) | (battery ? 2 : 0));
  rom[7] = uint8_t(mapper & 0xf0);
  std::vector<uint8_t> code;
  auto emit = [&](std::initializer_list<uint8_t> b) { code.insert(code.end(), b); };
  emit({0x78, 0xd8, 0xa2, 0xff, 0x9a, // sei; cld; ldx #255; txs
        0xa9, 0, 0x8d, 0, 0x20, 0x8d, 1, 0x20, // disable NMI/rendering
        0xad, 1, 0x60, 0x8d, 4, 0x60, // previous persisted B -> boot marker
        0xa9, 1, 0x8d, 0x15, 0x40, // pulse enable
        0xa9, 0xbf, 0x8d, 0, 0x40, // duty/constant volume
        0xa9, 0x40, 0x8d, 2, 0x40,
        0xa9, 0x08, 0x8d, 3, 0x40});
  auto loop = code.size();
  emit({0xad, 2, 0x20, 0x10, 0xfb, // wait for vblank
        0xa9, 1, 0x8d, 0x16, 0x40, 0xa9, 0, 0x8d, 0x16, 0x40,
        0xad, 0x16, 0x40, 0x29, 1, 0x85, 0, 0x8d, 0, 0x60, // A
        0xad, 0x16, 0x40, 0x29, 1, 0x85, 1, 0x8d, 1, 0x60, // B
        0xee, 2, 0x60,
        0xa9, 0x3f, 0x8d, 6, 0x20, 0xa9, 0, 0x8d, 6, 0x20,
        0xa5, 0, 0x0a, 0x05, 1, 0xaa}); // x = A*2+B
  auto lookup = code.size();
  emit({0xbd, 0, 0, 0x8d, 7, 0x20, // palette[x] -> $2007
        0xa9, 0, 0x8d, 6, 0x20, 0x8d, 6, 0x20, // reset PPU address
        0x4c, uint8_t(loop), uint8_t(0xe0 + (loop >> 8))});
  code[lookup + 1] = uint8_t(code.size());
  code[lookup + 2] = uint8_t(0xe0 + (code.size() >> 8));
  emit({0x0f, 0x16, 0x21, 0x2a});
  std::copy(code.begin(), code.end(), rom.begin() + 16 + 0x6000);
  for(size_t vector = 0x7ffa; vector <= 0x7ffe; vector += 2) {
    rom[16 + vector] = 0; rom[16 + vector + 1] = 0xe0;
  }
  return rom;
}

}  // namespace clover::test_fixture
