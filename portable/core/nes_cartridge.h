// Narrow iNES 1.0 cartridge analysis, following the board/pak conventions in
// pinned mia/medium/famicom.cpp. No database, firmware or archive dependency.
#pragma once

namespace {
nall::string nesManifest(const std::vector<uint8_t>& data) {
  if(data.size() < 16 || data[0] != 'N' || data[1] != 'E' || data[2] != 'S' || data[3] != 0x1a)
    throw std::runtime_error("Invalid iNES cartridge header");
  if((data[7] & 0x0c) == 0x08) throw std::runtime_error("NES 2.0 cartridges are not supported; use standard iNES 1.0");
  if(data[7] & 0x0f) throw std::runtime_error("Unsupported iNES console/header variant (only standard NES cartridges are supported)");
  if(data[6] & 4) throw std::runtime_error("iNES trainers are not supported");
  for(unsigned i = 10; i < 16; ++i) if(data[i])
    throw std::runtime_error("Nonstandard iNES reserved header bytes are not supported");
  if(data[9] & 0xfe) throw std::runtime_error("Nonstandard iNES timing flags are not supported");
  unsigned mapper = (data[6] >> 4) | (data[7] & 0xf0);
  const char* board = nullptr;
  const char* chip = nullptr;
  switch(mapper) {
  case 0: board = "HVC-NROM-256"; break;
  case 1: board = "HVC-SXROM"; chip = "MMC1B2"; break;
  case 2: board = "HVC-UOROM"; break;
  case 3: board = "HVC-CNROM"; break;
  case 4: board = "HVC-TLROM"; chip = "MMC3B"; break;
  case 9: board = "HVC-PNROM"; chip = "MMC2"; break;
  case 184: board = "SUNSOFT-1"; break;
  default: throw std::runtime_error("Unsupported NES mapper: " + std::to_string(mapper));
  }
  size_t prg = size_t(data[4]) * 16384, chr = size_t(data[5]) * 8192;
  if(!prg || data.size() != 16 + prg + chr)
    throw std::runtime_error("Truncated or unexpected trailing data in iNES cartridge");
  // Bound the stock board address spaces rather than silently alias oversized
  // mapper variants. MMC1's SXROM supports the standard extended PRG banking.
  size_t maxPrg = mapper == 1 ? 512*1024 : mapper == 2 || mapper == 9 ? 256*1024 : mapper == 4 ? 512*1024 : 32768;
  size_t maxChr = mapper == 3 ? 32768 : mapper == 4 ? 256*1024 : mapper == 1 || mapper == 9 ? 128*1024 : mapper == 184 ? 32768 : 8192;
  if(prg > maxPrg || chr > maxChr || (mapper == 2 && chr) || ((mapper == 3 || mapper == 9 || mapper == 184) && !chr))
    throw std::runtime_error("Unsupported ROM sizes for stock NES mapper " + std::to_string(mapper));
  bool fourScreen = data[6] & 8;
  if(fourScreen && (mapper != 4 || !chr))
    throw std::runtime_error("Four-screen mirroring is supported only for stock MMC3 CHR-ROM cartridges");
  if(fourScreen) board = "HVC-TVROM";
  size_t ram = (mapper == 1 || mapper == 4 || mapper == 9 || (data[6] & 2) || data[8]) ? 8192 : 0;
  if(data[8] > 1) throw std::runtime_error("Extended iNES PRG RAM sizes are not supported");
  if(ram && mapper != 0 && mapper != 1 && mapper != 4 && mapper != 9)
    throw std::runtime_error("Save RAM is not supported by this stock NES board");
  nall::string s = "game\n  title: cartridge\n  system: Regular\n";
  s.append("  region: ", (data[9] & 1) ? "PAL" : "NTSC-U", "\n  board: ", board, "\n");
  s.append("    mirror mode=", (data[6] & 1) ? "vertical" : "horizontal", "\n");
  if(chip) s.append("    chip type=", chip, "\n");
  auto memory = [&](const char* type, const char* content, size_t size, bool volatileMemory = false) {
    if(!size) return;
    s.append("    memory\n      type: ", type, "\n      content: ", content, "\n      size: ", size, "\n");
    if(volatileMemory) s.append("      volatile\n");
  };
  memory("ROM", "iNES", 16);
  memory("ROM", "Program", prg);
  memory("ROM", "Character", chr);
  memory("RAM", "Save", ram, !(data[6] & 2));
  memory("RAM", "Character", fourScreen ? 2048 : !chr ? 8192 : 0, true);
  return s;
}
} // namespace
