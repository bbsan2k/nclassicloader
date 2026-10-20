#include "ares_core.h"
#include "ares_c_api.h"
#include <algorithm>
#include <chrono>
#include <cmath>
#include <fstream>
#include <iostream>
#include <stdexcept>

namespace fs = std::filesystem;
using Bytes = std::vector<uint8_t>;

static void check(bool value, const std::string& message) {
  if(!value) throw std::runtime_error(message);
}
static void write(const fs::path& path, const Bytes& data) {
  std::ofstream f(path, std::ios::binary);
  f.write(reinterpret_cast<const char*>(data.data()), data.size());
  check(bool(f), "test file write failed");
}
static Bytes read(const fs::path& path) {
  std::ifstream f(path, std::ios::binary);
  return Bytes(std::istreambuf_iterator<char>(f), {});
}
static std::string utf8(const fs::path& path) {
  auto value = path.u8string();
  return {reinterpret_cast<const char*>(value.data()), value.size()};
}

// Entirely self-authored 65816 program: poll B via the serial joypad register,
// paint red/green CGRAM backdrop during vblank, and write B into LoROM SRAM.
// No commercial ROM, firmware, assembler, or external emulator is used.
static Bytes homebrew() {
  Bytes rom(32768, 0xea);
  Bytes code;
  auto emit = [&](std::initializer_list<uint8_t> bytes) { code.insert(code.end(), bytes); };
  auto jump = [&](size_t offset) { emit({0x4c, uint8_t(offset), uint8_t(0x80 + (offset >> 8))}); };
  emit({0x78,                         // sei
        0x9c, 0x00, 0x42,           // stz $4200
        0xa9, 0x0f, 0x8d, 0x00, 0x21}); // lda #$0f; sta $2100
  auto wait = code.size();
  emit({0xad, 0x12, 0x42, 0x10, 0xfb, // wait: lda $4212; bpl wait
        0xa9, 1, 0x8d, 0x16, 0x40,  // latch joypad
        0x9c, 0x16, 0x40,
        0xad, 0x16, 0x40, 0x29, 1,   // read B
        0x8f, 0, 0, 0x70,           // sta $700000
        0x9c, 0x21, 0x21,           // stz $2121 (preserves A)
        0xf0, 0x0d,                 // beq red
        0xa9, 0xe0, 0x8d, 0x22, 0x21,
        0xa9, 0x03, 0x8d, 0x22, 0x21});
  auto skip = code.size();
  emit({0x4c, 0, 0});
  emit({0xa9, 0x1f, 0x8d, 0x22, 0x21, 0x9c, 0x22, 0x21}); // red
  code[skip + 1] = uint8_t(code.size());
  code[skip + 2] = uint8_t(0x80 + (code.size() >> 8));
  emit({0xad, 0x12, 0x42, 0x30, 0xfb}); // wait until vblank ends
  jump(wait);
  std::copy(code.begin(), code.end(), rom.begin());
  const std::string title = "CLOVER BRIDGE TEST    ";
  std::copy_n(title.begin(), 21, rom.begin() + 0x7fc0);
  rom[0x7fd5] = 0x20; // LoROM
  rom[0x7fd6] = 0x02; // ROM + battery RAM
  rom[0x7fd7] = 0x05; // 32 KiB ROM
  rom[0x7fd8] = 0x01; // 2 KiB SRAM
  rom[0x7fd9] = 0x01; // NTSC
  rom[0x7fda] = 0;
  rom[0x7fdb] = 0;
  rom[0x7fdc] = 0xff; rom[0x7fdd] = 0xff;
  rom[0x7fde] = 0; rom[0x7fdf] = 0;
  rom[0x7ffc] = 0; rom[0x7ffd] = 0x80;
  return rom;
}

static bool color(const clover::AresCore::Frame& frame, bool green) {
  return std::any_of(frame.pixels.begin(), frame.pixels.end(), [&](uint32_t c) {
    auto r = (c >> 16) & 255, g = (c >> 8) & 255;
    return green ? g > 200 && r < 30 : r > 200 && g < 30;
  });
}

int main() {
  // Test artifacts stay under this test's working/build directory.
  auto dir = fs::current_path() / ("ares-smoke-" + std::to_string(std::chrono::steady_clock::now().time_since_epoch().count()));
  try {
    fs::create_directories(dir);
    // Self-authored SPC700 BRA $FFC0 idle firmware: no Nintendo boot bytes.
    auto firmware=dir/"firmware";fs::create_directories(firmware);
    Bytes ipl(64,0);ipl[0]=0x2f;ipl[1]=0xfe;ipl[62]=0xc0;ipl[63]=0xff;
    write(firmware/"ipl.rom",ipl);
    auto rom = homebrew();
    write(dir / "test.sfc", rom);
    clover::AresCore core;
    check(!core.runFrame(0), "unloaded run must fail");
    check(!core.load(dir / "test.sfrom", dir / "saves", {}), "SFROM must fail");
    check(core.error().find("conversion") != std::string::npos, "SFROM conversion diagnostic");
    write(dir / "invalid.sfc", Bytes(32768, 0));
    check(!core.load(dir / "invalid.sfc", dir / "saves", {}), "invalid header must fail");
    auto dspRom = rom;
    dspRom[0x7fd6] = 0x03;
    write(dir / "needs-dsp.sfc", dspRom);
    check(!core.load(dir / "needs-dsp.sfc", dir / "saves", {}), "missing firmware must fail");
    check(core.error().find("dsp1b.upd7725.program.rom") != std::string::npos, "missing firmware filename diagnostic");
    check(!core.load(dir / "test.sfc", dir / "saves", {}), "Missing IPL must fail");
    check(core.error().find("ipl.rom")!=std::string::npos,"Missing IPL diagnostic");
    check(core.load(dir / "test.sfc", dir / "saves", {}, firmware), core.error());
    clover::AresCore second;
    check(!second.load(dir / "test.sfc", dir / "saves", {}), "second session must fail");
    for(int i = 0; i < 4; ++i) check(core.runFrame(0), core.error());
    check(core.frame().pixels.size() == size_t(core.frame().width) * core.frame().height, "packed frame");
    check(color(core.frame(), false), "homebrew red backdrop was not rendered");
    check(!core.audio().empty() && core.audio().size() % 2 == 0 && core.audioRate() == 48000, "stereo audio missing");
    check(std::all_of(core.audio().begin(), core.audio().end(), [](float x) { return std::isfinite(x) && std::abs(x) <= 1; }), "invalid audio");
    check(core.saveState(dir / "test.state"), core.error());
    for(int i = 0; i < 3; ++i) check(core.runFrame(1), core.error());
    check(color(core.frame(), true), "B input did not reach controller port 1");
    check(core.loadState(dir / "test.state"), core.error());
    for(int i = 0; i < 3; ++i) check(core.runFrame(0), core.error());
    check(color(core.frame(), false), "state roundtrip failed");
    auto state = read(dir / "test.state");
    state.back() ^= 1;
    write(dir / "bad.state", state);
    check(!core.loadState(dir / "bad.state"), "corrupt state must fail");
    state.resize(100);
    write(dir / "short.state", state);
    check(!core.loadState(dir / "short.state"), "truncated state must fail");
    check(core.runFrame(0), core.error());
    for(int i = 0; i < 3; ++i) check(core.runFrame(1), core.error());
    auto width = core.frame().width, height = core.frame().height;
    auto samples = core.audio().size();
    core.unload();
    check(core.error().empty(), core.error());
    check(!core.loaded(), "unload failed");
    bool foundSave = false;
    for(auto entry : fs::recursive_directory_iterator(dir / "saves")) if(entry.path().filename() == "save.ram") {
      auto sram = read(entry.path());
      check(sram.size() == 2048 && sram[0] == 1, "SRAM was not persisted");
      foundSave = true;
    }
    check(foundSave, "SRAM file missing");
    Bytes headered(512, 0);
    headered.insert(headered.end(), rom.begin(), rom.end());
    write(dir / "test.smc", headered);
    check(core.load(dir / "test.smc", dir / "saves", firmware), core.error());
    check(core.loadState(dir / "test.state"), "copier-header normalization/state identity failed: " + core.error());
    check(core.runFrame(0), core.error());
    core.unload();
    // Different ROM identity must reject an otherwise valid state.
    rom[0x1000] ^= 1;
    write(dir / "other.sfc", rom);
    check(core.load(dir / "other.sfc", dir / "saves", firmware), core.error());
    check(!core.loadState(dir / "test.state"), "wrong ROM state must fail");
    core.unload();
    auto* api = clover_ares_create();
    check(api != nullptr, "C ABI session allocation");
    check(clover_ares_load(api, utf8(dir / "test.sfc").c_str(), utf8(dir / "c-api-saves").c_str(),
                           utf8(firmware).c_str(), utf8(firmware).c_str()), clover_ares_error(api));
    for(int i = 0; i < 4; ++i) check(clover_ares_run_frame(api, 0), clover_ares_error(api));
    size_t pixelCount = 0, floatCount = 0;
    uint32_t frameWidth = 0, frameHeight = 0;
    auto* pixels = clover_ares_frame(api, &pixelCount, &frameWidth, &frameHeight);
    check(pixels && pixelCount == size_t(frameWidth) * frameHeight, "C ABI frame view");
    auto* audio = clover_ares_audio(api, &floatCount);
    check(audio && floatCount > 0 && floatCount % 2 == 0 && clover_ares_audio_rate(api) == 48000,
          "C ABI stereo audio view");
    check(clover_ares_save_state(api, utf8(dir / "c-api.state").c_str()), clover_ares_error(api));
    check(clover_ares_run_frame(api, 1), clover_ares_error(api));
    check(clover_ares_load_state(api, utf8(dir / "c-api.state").c_str()), clover_ares_error(api));
    clover_ares_unload(api);
    check(!clover_ares_loaded(api) && std::string(clover_ares_error(api)).empty(), "C ABI unload");
    clover_ares_destroy(api);
    std::cout << "PASS: " << width << 'x' << height << " ARGB frame, " << samples
              << " stereo floats at 48000 Hz; B input, SRAM, states, SMC, C ABI and rejection checks\n";
    // Deliberately keep uniquely named artifacts for diagnostics; no broad delete.
    std::cout << "Artifacts: " << dir << '\n';
    return 0;
  } catch(const std::exception& e) {
    std::cerr << "FAIL: " << e.what() << "\nArtifacts: " << dir << '\n';
    return 1;
  }
}
