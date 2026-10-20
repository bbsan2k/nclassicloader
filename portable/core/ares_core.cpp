#include "ares_core.h"

#include <algorithm>
#include <chrono>
#include <cctype>
#include <condition_variable>
#include <fstream>
#include <mutex>
#include <stdexcept>
#include <thread>
#include <sfc/sfc.hpp>
#include <fc/fc.hpp>
#undef register
#include "nes_cartridge.h"

namespace {
using Bytes = std::vector<uint8_t>;
namespace analyzer {
auto operator+=(nall::string& lhs, const nall::string& rhs) -> nall::string& { return lhs.append(rhs); }
// The upstream analyzer only uses Medium::name for display metadata.
struct Medium { static auto name(const nall::string&) -> nall::string { return "cartridge"; } };
#include "sfc-analyzer.inc"
struct CartridgeAnalyzer : SuperFamicom {
  using SuperFamicom::rom;
  bool validHeader() { return rom.size() >= 32768 && scoreHeader(headerAddress) > 0; }
};
}
constexpr const char boards[] =
#include "sfc-boards.inc"
;
std::mutex sessionMutex;

Bytes readFile(const std::filesystem::path& path, size_t limit = 32 * 1024 * 1024) {
  std::ifstream file(path, std::ios::binary | std::ios::ate);
  if(!file) throw std::runtime_error("Cannot read file: " + path.string());
  auto length = file.tellg();
  if(length < 0 || uint64_t(length) > limit) throw std::runtime_error("Invalid or oversized file: " + path.string());
  Bytes result(static_cast<size_t>(length));
  file.seekg(0);
  if(!result.empty() && !file.read(reinterpret_cast<char*>(result.data()), result.size()))
    throw std::runtime_error("Short read: " + path.string());
  return result;
}

// Write a sibling temporary and rename: a failed write must not truncate SRAM.
void writeFile(const std::filesystem::path& path, std::span<const uint8_t> bytes) {
  auto temporary = path;
  temporary += ".clover-tmp";
  {
    std::ofstream file(temporary, std::ios::binary | std::ios::trunc);
    if(!file || (!bytes.empty() && !file.write(reinterpret_cast<const char*>(bytes.data()), bytes.size())))
      throw std::runtime_error("Cannot write file: " + temporary.string());
    file.flush();
    if(!file) throw std::runtime_error("Cannot flush file: " + temporary.string());
  }
  std::error_code ec;
  std::filesystem::rename(temporary, path, ec);
#ifdef _WIN32
  // Windows rename does not replace an existing destination. Preserve the old
  // file under a backup name until the replacement has succeeded.
  if(ec && std::filesystem::exists(path)) {
    auto backup = path;
    backup += ".clover-backup";
    std::filesystem::rename(path, backup);
    std::filesystem::rename(temporary, path, ec);
    if(ec) std::filesystem::rename(backup, path);
    else std::filesystem::remove(backup);
  }
#endif
  if(ec) throw std::runtime_error("Cannot replace file: " + path.string() + ": " + ec.message());
}

std::string digest(std::span<const uint8_t> bytes) {
  auto hash = nall::Hash::SHA256(bytes).digest();
  return hash.data();
}

std::string compatibility(bool nes) {
  // Native floating-point/core implementation details forbid loose portability.
  std::string result = nes ? "clover-ares-famicom-state-1\n" CLOVER_ARES_REVISION "\nines1;48000;"
                           : "clover-ares-state-2\n" CLOVER_ARES_REVISION "\nperformance;pixel-accurate;48000;";
#if defined(__aarch64__) || defined(_M_ARM64)
  result += "arm64;";
#elif defined(__x86_64__) || defined(_M_X64)
  result += "x86_64;";
#else
  result += "other;";
#endif
#ifdef _WIN32
  result += "windows;";
#elif defined(__APPLE__)
  result += "macos;";
#else
  result += "unix;";
#endif
#ifdef __VERSION__
  result += __VERSION__;
#else
  result += std::to_string(_MSC_VER);
#endif
  return result + ";ptr=" + std::to_string(sizeof(void*)) + ";bridge=2\n";
}
} // namespace

namespace clover {
struct AresCore::Impl final : ares::Platform {
  ares::Node::System root;
  std::shared_ptr<nall::vfs::directory> systemPak, gamePak;
  Frame output;
  std::vector<float> samples;
  std::string lastError, romDigest, imageDigest;
  std::filesystem::path savePath;
  std::vector<std::string> persistentFiles;
  std::thread::id owner;
  uint16_t buttons = 0;
  bool active = false;
  bool nes = false;
  std::mutex videoMutex;
  std::condition_variable videoReady;
  uint64_t videoGeneration = 0;

  bool fail(const std::string& message) { lastError = message; return false; }
  bool requireLoaded() {
    if(!active) return fail("No ROM is loaded");
    if(owner != std::this_thread::get_id()) return fail("AresCore must be used on its loading thread");
    lastError.clear();
    return true;
  }
  auto pak(ares::Node::Object node) -> std::shared_ptr<nall::vfs::directory> override {
    if(node->name() == "Super Famicom" || node->name() == "Famicom") return systemPak;
    if(node->name() == "Super Famicom Cartridge" || node->name() == "Famicom Cartridge") return gamePak;
    return {};
  }
  auto video(ares::Node::Video::Screen, const uint32_t* data, uint32_t pitch,
             uint32_t width, uint32_t height) -> void override {
    std::lock_guard lock(videoMutex);
    output.width = width;
    output.height = height;
    output.pixels.resize(size_t(width) * height);
    for(unsigned y = 0; y < height; ++y) {
      auto row = reinterpret_cast<const uint32_t*>(reinterpret_cast<const uint8_t*>(data) + size_t(y) * pitch);
      for(unsigned x = 0; x < width; ++x) output.pixels[size_t(y) * width + x] = row[x] | 0xff000000u;
    }
    ++videoGeneration;
    videoReady.notify_one();
  }
  auto audio(ares::Node::Audio::Stream stream) -> void override {
    std::vector<double> values(stream->channels());
    while(stream->pending()) {
      auto count = stream->read(values.data());
      if(!count) break;
      samples.push_back(float(std::clamp(values[0], -1.0, 1.0)));
      samples.push_back(float(std::clamp(values[count > 1 ? 1 : 0], -1.0, 1.0)));
    }
  }
  auto input(ares::Node::Input::Input node) -> void override {
    // Shared ABI uses the SNES mask for both systems: NES B is bit 0 and A
    // is bit 8, irrespective of the NES serial register's A-then-B order.
    static constexpr const char* names[] = {"B", "Y", "Select", "Start", "Up", "Down", "Left", "Right", "A", "X", "L", "R"};
    if(auto button = node->cast<ares::Node::Input::Button>()) {
      bool pressed = false;
      for(unsigned i = 0; i < 12; ++i) if(button->name() == names[i]) pressed = buttons & (1u << i);
      button->setValue(pressed);
    }
  }
  void persist() {
    if(!active) return;
    root->save();
    for(const auto& name : persistentFiles) {
      auto file = gamePak->read(name.c_str());
      if(!file) throw std::runtime_error("Missing save memory: " + name);
      Bytes data(file->size());
      file->read(data.data(), data.size());
      writeFile(savePath / name, data);
    }
  }
  void close() {
    std::lock_guard sessionLock(sessionMutex);
    if(ares::platform != this) return;
    lastError.clear(); // unload reports its own persistence result, not a stale state/load error
    try { persist(); } catch(const std::exception& e) { lastError = e.what(); }
    if(root) root->unload(); // joins the video worker before releasing callback data
    root.reset();
    ares::platform = nullptr;
    active = false;
    gamePak.reset();
    systemPak.reset();
    persistentFiles.clear();
    samples.clear();
    output = {};
  }
};

AresCore::AresCore() : impl_(std::make_unique<Impl>()) {}
AresCore::~AresCore() { unload(); }

bool AresCore::load(const std::filesystem::path& rom, const std::filesystem::path& saves,
                    const std::filesystem::path& firmware) {
  return load(rom,saves,firmware,firmware);
}
bool AresCore::load(const std::filesystem::path& rom, const std::filesystem::path& saves,
                    const std::filesystem::path& firmware,const std::filesystem::path& systemFirmware) {
  auto& p = *impl_;
  if(p.active && !p.requireLoaded()) return false;
  p.lastError.clear();
  unload();
  if(!p.lastError.empty()) return false; // don't hide a failed SRAM flush
  std::lock_guard sessionLock(sessionMutex);
  if(ares::platform) return p.fail("Only one ares core session may be loaded at a time");
  try {
    auto extension = rom.extension().string();
    std::transform(extension.begin(), extension.end(), extension.begin(), [](unsigned char c) { return char(std::tolower(c)); });
    if(extension == ".sfrom") return p.fail("SFROM containers require conversion to a raw .sfc ROM before loading");
    if(extension == ".qd" || extension == ".fds") return p.fail("Famicom Disk System images are not supported");
    if(extension != ".sfc" && extension != ".smc" && extension != ".nes") return p.fail("Only raw .sfc, .smc and iNES .nes ROM files are supported");
    p.nes = extension == ".nes";
    nall::string manifest;
    Bytes bytes;
    if(p.nes) {
      bytes = readFile(rom, 16 * 1024 * 1024);
      manifest = nesManifest(bytes);
    } else {
      analyzer::CartridgeAnalyzer analyzer;
      analyzer.rom = readFile(rom, 16 * 1024 * 1024);
      manifest = analyzer.analyze(analyzer.rom); // strips the optional 512-byte copier header
      if(!manifest || !analyzer.validHeader()) return p.fail("ROM is too small or its SNES header could not be analyzed");
      bytes = std::move(analyzer.rom);
    }
    p.romDigest = digest(bytes);
    auto document = nall::BML::unserialize(manifest);
    auto board = document["game/board"].string();
    if(board.beginsWith("GB-") || board.beginsWith("BS-") || board.beginsWith("ST-"))
      return p.fail("Super Game Boy, Satellaview and Sufami Turbo add-on cartridges are not supported");
    bool mapped = p.nes;
    if(!p.nes) for(auto leaf : nall::BML::unserialize(boards).find("board")) {
      auto id = leaf.text();
      if(id == board) mapped = true;
      if(id.match("*(*)*")) {
        auto parts = nall::split(id.transform("()", "||"), "|");
        if(parts.size() == 3) for(auto revision : nall::split(parts[1], ","))
          if(nall::string{parts[0], revision, parts[2]} == board) mapped = true;
      }
    }
    if(!mapped) return p.fail("Unsupported cartridge board: " + std::string(board.data()));
    p.gamePak = std::make_shared<nall::vfs::directory>();
    p.systemPak = std::make_shared<nall::vfs::directory>();
    if(!p.nes) p.systemPak->append("boards.bml", nall::string{boards});
    p.gamePak->setAttribute("title", document["game/title"].string());
    p.gamePak->setAttribute("board", board);
    p.gamePak->setAttribute("region", document["game/region"].string());
    if(p.nes) {
      p.gamePak->setAttribute("mirror", document["game/board/mirror/mode"].string());
      p.gamePak->setAttribute("system", document["game/system"].string());
      p.gamePak->setAttribute("chip", document["game/board/chip/type"].string());
    }
    if(auto frequency = document["game/board/oscillator/frequency"])
      p.gamePak->setAttribute("oscillator", frequency.natural());
    p.gamePak->append("manifest.bml", manifest);
    nall::Hash::SHA256 imageHash;
    std::span<const uint8_t> remaining(bytes);
    for(auto memory : document.find("game/board/memory(type=ROM)")) {
      auto architecture = memory["architecture"].string();
      nall::string name;
      if(architecture) name.append(architecture.downcase(), ".");
      name.append(memory["content"].string().downcase(), ".rom");
      size_t size = memory["size"].natural();
      Bytes external;
      std::span<const uint8_t> data;
      if(remaining.size() >= size) {
        data = remaining.first(size);
        remaining = remaining.subspan(size);
      } else {
        if(!architecture) throw std::runtime_error("Truncated cartridge ROM");
        auto identifier = memory["identifier"].string().downcase();
        auto filename = std::string(identifier.data()) + "." + name.data();
        if(firmware.empty() || !std::filesystem::is_regular_file(firmware/filename)) throw std::runtime_error("Required user firmware: " + filename + " (choose a coprocessor firmware folder)");
        external = readFile(firmware / filename);
        if(external.size() != size) throw std::runtime_error("Incorrect firmware size: " + filename);
        data = external;
      }
      p.gamePak->append(name, data);
      imageHash.input(data);
    }
    if(!remaining.empty()) throw std::runtime_error("Unexpected trailing or incomplete appended firmware in ROM");
    if(!p.nes) {
      if(systemFirmware.empty() || !std::filesystem::is_regular_file(systemFirmware/"ipl.rom"))
        throw std::runtime_error("Required SMP boot firmware: ipl.rom. Import a supported console image or choose a firmware folder.");
      auto ipl=readFile(systemFirmware/"ipl.rom",64);
      if(ipl.size()!=64) throw std::runtime_error("Incorrect SMP boot firmware size: ipl.rom must be 64 bytes");
      p.systemPak->append("ipl.rom",std::span<const uint8_t>(ipl));
      imageHash.input(ipl);
    }
    p.imageDigest = imageHash.digest().data();
    if(saves.empty()) throw std::runtime_error("An explicit save directory is required");
    p.savePath = saves / p.romDigest;
    std::filesystem::create_directories(p.savePath);
    for(auto memory : document.find("game/board/memory")) {
      auto type = memory["type"].string();
      if(type != "RAM" && type != "RTC") continue;
      nall::string name;
      if(auto architecture = memory["architecture"].string()) name.append(architecture.downcase(), ".");
      name.append(memory["content"].string().downcase(), ".", type.downcase());
      Bytes data(memory["size"].natural(), 0xff);
      bool persistent = !memory["volatile"];
      if(persistent) {
        p.persistentFiles.emplace_back(name.data());
        auto path = p.savePath / name.data();
        if(std::filesystem::exists(path)) {
          auto saved = readFile(path);
          if(saved.size() != data.size()) throw std::runtime_error("Incorrect save memory size: " + path.string());
          data = std::move(saved);
        }
      }
      p.gamePak->append(name, std::span<const uint8_t>(data));
    }
    ares::platform = &p;
    p.owner = std::this_thread::get_id();
    bool pal = document["game/region"].string() == "PAL";
    if(p.nes) {
      if(!ares::Famicom::load(p.root, pal ? "[Nintendo] Famicom (PAL)" : "[Nintendo] Famicom (NTSC-U)"))
        throw std::runtime_error("ares could not initialize the Famicom system");
    } else {
      ares::SuperFamicom::option("Pixel Accuracy", "true");
      ares::SuperFamicom::option("Deterministic Entropy", "true");
      if(!ares::SuperFamicom::load(p.root, pal ? "[Nintendo] Super Famicom (PAL)" : "[Nintendo] Super Famicom (NTSC)"))
        throw std::runtime_error("ares could not initialize the Super Famicom system");
    }
    auto slot = p.root->find<ares::Node::Port>("Cartridge Slot");
    if(!slot || !slot->allocate()) throw std::runtime_error("ares cartridge slot allocation failed");
    slot->connect();
    auto controller = p.root->find<ares::Node::Port>("Controller Port 1");
    if(!controller || !controller->allocate("Gamepad")) throw std::runtime_error("ares gamepad allocation failed");
    controller->connect();
    for(auto stream : p.root->find<ares::Node::Audio::Stream>()) stream->setResamplerFrequency(48000);
    // Famicom's default full raster includes horizontal/vertical blanking.
    // Expose the actual 256x240 viewport for both 4:3 and square-pixel modes.
    if(p.nes) for(auto screen : p.root->find<ares::Node::Video::Screen>()) screen->setOverscan(false);
    p.root->power();
    p.buttons = 0;
    p.active = true;
    return true;
  } catch(const std::exception& e) {
    p.lastError = e.what();
    if(ares::platform == &p) {
      if(p.root) p.root->unload();
      p.root.reset();
      ares::platform = nullptr;
    }
    p.gamePak.reset();
    p.systemPak.reset();
    p.persistentFiles.clear();
    return false;
  }
}

bool AresCore::runFrame(uint16_t buttons) {
  auto& p = *impl_;
  if(!p.requireLoaded()) return false;
  try {
    p.buttons = buttons;
    p.samples.clear();
    uint64_t before;
    { std::lock_guard lock(p.videoMutex); before = p.videoGeneration; }
    p.root->run();
    std::unique_lock lock(p.videoMutex);
    if(!p.videoReady.wait_for(lock, std::chrono::seconds(5), [&] { return p.videoGeneration != before; }))
      return p.fail("ares did not produce a video frame");
    return !p.output.pixels.empty() || p.fail("ares returned an empty video frame");
  } catch(const std::exception& e) { return p.fail(e.what()); }
}

bool AresCore::saveState(const std::filesystem::path& path) {
  auto& p = *impl_;
  if(!p.requireLoaded()) return false;
  try {
    auto state = p.root->serialize();
    if(!state) return p.fail("ares serialization failed");
    std::span<const uint8_t> payload(state.data(), state.size());
    auto header = compatibility(p.nes) + p.romDigest + "\n" + p.imageDigest + "\n" + digest(payload) + "\n";
    Bytes file(header.begin(), header.end());
    file.insert(file.end(), payload.begin(), payload.end());
    writeFile(path, file);
    return true;
  } catch(const std::exception& e) { return p.fail(e.what()); }
}

bool AresCore::loadState(const std::filesystem::path& path) {
  auto& p = *impl_;
  if(!p.requireLoaded()) return false;
  try {
    auto file = readFile(path, 64 * 1024 * 1024);
    auto prefix = compatibility(p.nes) + p.romDigest + "\n" + p.imageDigest + "\n";
    size_t offset = prefix.size() + 65;
    if(file.size() <= offset || !std::equal(prefix.begin(), prefix.end(), file.begin()))
      return p.fail("Incompatible state: bridge/core build, ROM, or firmware differs");
    std::span<const uint8_t> payload(file.data() + offset, file.size() - offset);
    auto checksum = digest(payload) + "\n";
    if(!std::equal(checksum.begin(), checksum.end(), file.begin() + prefix.size()))
      return p.fail("State checksum mismatch (corrupt or truncated file)");
    // Upstream's serializer tolerates short input. Enforce the exact size before
    // permitting it to mutate the machine; retain a rollback snapshot as well.
    auto rollback = p.root->serialize();
    if(payload.size() != rollback.size()) return p.fail("Incompatible state payload size");
    p.persist(); // never discard unsaved SRAM when rewinding
    nall::serializer state(payload.data(), payload.size());
    if(!p.root->unserialize(state)) {
      nall::serializer restore(rollback.data(), rollback.size());
      p.root->unserialize(restore);
      return p.fail("ares rejected the state payload");
    }
    p.samples.clear();
    p.buttons = 0;
    return true;
  } catch(const std::exception& e) { return p.fail(e.what()); }
}

void AresCore::unload() { impl_->close(); }
bool AresCore::loaded() const { return impl_->active; }
const AresCore::Frame& AresCore::frame() const { return impl_->output; }
const std::vector<float>& AresCore::audio() const { return impl_->samples; }
unsigned AresCore::audioRate() const { return 48000; }
const std::string& AresCore::error() const { return impl_->lastError; }
} // namespace clover
