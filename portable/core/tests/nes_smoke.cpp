#include "smoke_entry.h"
#include "nes_fixture.h"
#include "ares_core.h"
#include "ares_c_api.h"
#include <algorithm>
#include <chrono>
#include <cmath>
#include <cstdio>
#include <fstream>
#include <iostream>
#include <stdexcept>
#include <thread>

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
  auto s = path.u8string();
  return {reinterpret_cast<const char*>(s.data()), s.size()};
}

extern "C" int clover_nes_smoke(const char* artifact_root, char* error, size_t error_size) {
  if(error && error_size) error[0] = '\0';
  fs::path dir;
  try {
    if(!artifact_root || !*artifact_root) throw std::invalid_argument("artifact root must be non-empty");
    fs::path root(artifact_root);
    if(!root.is_absolute()) throw std::invalid_argument("artifact root must be absolute");
    dir = root / ("ares-nes-smoke-" + std::to_string(std::chrono::steady_clock::now().time_since_epoch().count()));
    fs::create_directories(dir);
    auto path = dir / "test.nes", saves = dir / "saves";
    auto rom = clover::test_fixture::nes_homebrew(1, true);
    write(path, rom);
    clover::AresCore core;
    check(!core.load(dir / "disk.qd", saves, {}), "QD must fail without trying to open it");
    check(core.error().find("Disk System") != std::string::npos, "QD diagnostic");
    check(core.load(path, saves, {}), core.error()); // no BIOS/firmware
    clover::AresCore second;
    check(!second.load(path, saves, {}), "second session must fail");
    bool wrongThread = true;
    std::thread t([&] { wrongThread = core.runFrame(0); }); t.join();
    check(!wrongThread, "wrong-thread execution must fail");
    auto frames = [&](uint16_t buttons) {
      for(int i = 0; i < 4; ++i) check(core.runFrame(buttons), core.error());
      check(core.frame().width == 256 && core.frame().height == 240,
            "NES video geometry: " + std::to_string(core.frame().width) + "x" + std::to_string(core.frame().height));
      check(core.frame().pixels.size() == size_t(core.frame().width) * core.frame().height, "packed pixels");
      return core.frame().pixels;
    };
    auto neutral = frames(0), b = frames(1), a = frames(1 << 8), both = frames(1 | (1 << 8));
    check(neutral != b && neutral != a && a != b && both != a && both != b, "SNES-mask A/B mapping or video failed");
    check(frames(2) == neutral, "SNES Y must not act as NES A/B");
    check(!core.audio().empty() && core.audio().size() % 2 == 0 && core.audioRate() == 48000, "stereo audio");
    check(std::all_of(core.audio().begin(), core.audio().end(), [](float x) { return std::isfinite(x) && std::abs(x) <= 1; }), "audio bounds");
    check(std::any_of(core.audio().begin(), core.audio().end(), [](float x) { return std::abs(x) > 0.001; }), "pulse audio is silent");
    frames(1);
    check(core.saveState(dir / "test.state"), core.error());
    auto replay = frames(1 << 8);
    check(core.saveState(dir / "replay.state"), core.error());
    check(core.loadState(dir / "test.state"), core.error());
    check(frames(1 << 8) == replay, "state video replay");
    check(core.saveState(dir / "replayed.state"), core.error());
    check(read(dir / "replay.state") == read(dir / "replayed.state"), "state replay determinism");
    auto state = read(dir / "test.state");
    check(std::string(state.begin(), state.begin() + 28).find("famicom") != std::string::npos, "NES state identity");
    state.back() ^= 1; write(dir / "corrupt.state", state);
    check(!core.loadState(dir / "corrupt.state"), "corrupt state rejection");
    state.resize(100); write(dir / "short.state", state);
    check(!core.loadState(dir / "short.state"), "short state rejection");
    frames(1);
    core.unload(); check(core.error().empty(), core.error());
    fs::path save;
    for(auto entry : fs::recursive_directory_iterator(saves)) if(entry.path().filename() == "save.ram") save = entry.path();
    check(!save.empty(), "battery save missing");
    auto ram = read(save);
    check(ram.size() == 8192 && ram[0] == 0 && ram[1] == 1, "persisted A/B SRAM");
    check(core.load(path, saves, {}), core.error()); frames(0); core.unload();
    check(read(save)[4] == 1, "battery RAM was not restored at cartridge boot");
    rom[16 + 0x100] ^= 1; write(path, rom);
    check(core.load(path, saves, {}), core.error());
    check(!core.loadState(dir / "test.state"), "wrong cartridge state rejection"); core.unload();
    // Exercise fixed-bank startup and state serialization on every initial mapper.
    for(unsigned mapper : {0u, 1u, 2u, 3u, 4u, 9u, 184u}) {
      write(path, clover::test_fixture::nes_homebrew(mapper, false, mapper == 2));
      check(core.load(path, dir / "volatile", {}), "mapper " + std::to_string(mapper) + ": " + core.error());
      check(frames(0) != frames(1 << 8), "mapper input/video " + std::to_string(mapper));
      check(core.saveState(dir / "mapper.state"), core.error());
      check(core.loadState(dir / "mapper.state"), core.error()); core.unload();
    }
    for(auto entry : fs::recursive_directory_iterator(dir / "volatile"))
      check(entry.path().extension() != ".ram", "volatile RAM must not persist");
    auto reject = [&](Bytes bad, const char* reason) { write(path, bad); check(!core.load(path, saves, {}), reason); };
    reject(Bytes(8), "short header");
    rom = clover::test_fixture::nes_homebrew(0); rom.pop_back(); reject(rom, "truncated ROM");
    rom = clover::test_fixture::nes_homebrew(0); rom.push_back(0); reject(rom, "trailing ROM");
    rom = clover::test_fixture::nes_homebrew(0); rom[6] |= 4; reject(rom, "trainer");
    rom = clover::test_fixture::nes_homebrew(0); rom[7] = 8; reject(rom, "NES 2.0");
    rom = clover::test_fixture::nes_homebrew(0); rom[7] = 1; reject(rom, "Vs console");
    rom = clover::test_fixture::nes_homebrew(0); rom[6] = 0x50; reject(rom, "unsupported mapper");
    rom = clover::test_fixture::nes_homebrew(0); rom[6] |= 8; reject(rom, "unsupported four screen");
    rom = clover::test_fixture::nes_homebrew(3, false, true); reject(rom, "CNROM without CHR ROM");
    rom = clover::test_fixture::nes_homebrew(4); rom[6] |= 8; write(path, rom);
    check(core.load(path, saves, {}), core.error()); frames(0); core.unload();
    rom = clover::test_fixture::nes_homebrew(0); rom[9] = 1; write(path, rom);
    check(core.load(path, saves, {}), core.error()); frames(0); core.unload();
    auto* api = clover_ares_create(); check(api != nullptr, "C ABI create");
    check(clover_ares_load(api, utf8(path).c_str(), utf8(saves).c_str(), "", ""), clover_ares_error(api));
    check(clover_ares_run_frame(api, 1 << 8), clover_ares_error(api));
    check(clover_ares_save_state(api, utf8(dir / "api.state").c_str()), clover_ares_error(api));
    check(clover_ares_load_state(api, utf8(dir / "api.state").c_str()), clover_ares_error(api));
    clover_ares_destroy(api);
    std::cout << "PASS: NES mappers 0,1,2,3,4,9,184; video, A/B, pulse audio, deterministic states, battery/volatile RAM, PAL, C ABI and rejection checks\nArtifacts: " << dir << '\n';
    return 0;
  } catch(const std::exception& e) {
    if(error && error_size) { std::snprintf(error, error_size, "%s", e.what()); error[error_size - 1] = '\0'; }
#ifndef CLOVER_SMOKE_EMBEDDED
    std::cerr << "FAIL: " << (error && error_size ? error : e.what()) << (dir.empty() ? "" : "\nArtifacts: " + dir.string()) << '\n';
#endif
    return 1;
  } catch(...) {
    if(error && error_size) { std::snprintf(error, error_size, "unknown exception"); error[error_size - 1] = '\0'; }
#ifndef CLOVER_SMOKE_EMBEDDED
    std::cerr << "FAIL: " << (error && error_size ? error : "unknown exception") << (dir.empty() ? "" : "\nArtifacts: " + dir.string()) << '\n';
#endif
    return 1;
  }
}
#ifndef CLOVER_SMOKE_EMBEDDED
int main() {
  char error[1024] = {};
  return clover_nes_smoke(fs::current_path().string().c_str(), error, sizeof(error));
}
#endif
