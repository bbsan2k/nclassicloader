#pragma once

#include <cstdint>
#include <filesystem>
#include <memory>
#include <string>
#include <vector>

namespace clover {

// All operations on a loaded core must occur on the loading thread. Only one
// loaded AresCore may exist at a time (ares's SNES machine is process-global).
class AresCore {
public:
  struct Frame {
    std::vector<uint32_t> pixels;
    unsigned width = 0, height = 0;
  };
  AresCore();
  ~AresCore();
  AresCore(const AresCore&) = delete;
  AresCore& operator=(const AresCore&) = delete;
  bool load(const std::filesystem::path& rom,
            const std::filesystem::path& saveDirectory,
            const std::filesystem::path& firmwareDirectory);
  // Imported system IPL can stay separate from an optional user coprocessor
  // firmware folder. The original overload uses one folder for both.
  bool load(const std::filesystem::path& rom,
            const std::filesystem::path& saveDirectory,
            const std::filesystem::path& firmwareDirectory,
            const std::filesystem::path& systemFirmwareDirectory);
  bool runFrame(uint16_t buttons);
  const Frame& frame() const;
  const std::vector<float>& audio() const;
  unsigned audioRate() const;
  bool saveState(const std::filesystem::path&);
  bool loadState(const std::filesystem::path&);
  void unload();
  bool loaded() const;
  const std::string& error() const;
private:
  struct Impl;
  std::unique_ptr<Impl> impl_;
};

} // namespace clover
