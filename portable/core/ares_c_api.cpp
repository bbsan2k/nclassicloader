#include "ares_c_api.h"
#include "ares_core.h"

#include <filesystem>
#include <memory>
#include <string>

struct CloverAres {
  clover::AresCore core;
  std::string error;
};

namespace {
std::filesystem::path path(const char* value) {
  if(!value || !*value) return {};
  return std::filesystem::u8path(value);
}

int report(CloverAres* handle, bool result) {
  if(!handle) return 0;
  handle->error = handle->core.error();
  return result ? 1 : 0;
}
}  // namespace

extern "C" {
CloverAres* clover_ares_create(void) {
  try {
    return new CloverAres;
  } catch(...) {
    return nullptr;
  }
}

void clover_ares_destroy(CloverAres* core) { delete core; }

int clover_ares_load(CloverAres* core, const char* rom, const char* saves,
                     const char* coprocessorFirmware, const char* systemFirmware) {
  if(!core || !rom || !saves) return 0;
  try {
    return report(core, core->core.load(path(rom), path(saves), path(coprocessorFirmware), path(systemFirmware)));
  } catch(const std::exception& error) {
    core->error = error.what();
  } catch(...) {
    core->error = "Unknown native ares error";
  }
  return 0;
}

int clover_ares_run_frame(CloverAres* core, uint16_t buttons) {
  if(!core) return 0;
  try {
    return report(core, core->core.runFrame(buttons));
  } catch(const std::exception& error) {
    core->error = error.what();
  } catch(...) {
    core->error = "Unknown native ares error";
  }
  return 0;
}

void clover_ares_unload(CloverAres* core) {
  if(!core) return;
  try {
    core->core.unload();
    core->error = core->core.error();
  } catch(const std::exception& error) {
    core->error = error.what();
  } catch(...) {
    core->error = "Unknown native ares error";
  }
}

int clover_ares_loaded(const CloverAres* core) { return core && core->core.loaded() ? 1 : 0; }

const uint32_t* clover_ares_frame(const CloverAres* core, size_t* pixelCount,
                                  uint32_t* width, uint32_t* height) {
  if(pixelCount) *pixelCount = 0;
  if(width) *width = 0;
  if(height) *height = 0;
  if(!core) return nullptr;
  const auto& frame = core->core.frame();
  if(pixelCount) *pixelCount = frame.pixels.size();
  if(width) *width = frame.width;
  if(height) *height = frame.height;
  return frame.pixels.data();
}

const float* clover_ares_audio(const CloverAres* core, size_t* floatCount) {
  if(floatCount) *floatCount = 0;
  if(!core) return nullptr;
  const auto& audio = core->core.audio();
  if(floatCount) *floatCount = audio.size();
  return audio.data();
}

uint32_t clover_ares_audio_rate(const CloverAres* core) { return core ? core->core.audioRate() : 0; }

int clover_ares_save_state(CloverAres* core, const char* filename) {
  if(!core || !filename) return 0;
  try {
    return report(core, core->core.saveState(path(filename)));
  } catch(const std::exception& error) {
    core->error = error.what();
  } catch(...) {
    core->error = "Unknown native ares error";
  }
  return 0;
}

int clover_ares_load_state(CloverAres* core, const char* filename) {
  if(!core || !filename) return 0;
  try {
    return report(core, core->core.loadState(path(filename)));
  } catch(const std::exception& error) {
    core->error = error.what();
  } catch(...) {
    core->error = "Unknown native ares error";
  }
  return 0;
}

const char* clover_ares_error(const CloverAres* core) {
  return core ? core->error.c_str() : "Null ares handle";
}
}  // extern "C"
