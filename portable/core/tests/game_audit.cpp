#include "ares_core.h"
#include "json.hpp"

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <limits>
#include <stdexcept>
#include <string>

namespace {
namespace fs = std::filesystem;
using Json = nlohmann::json;

void parent(const fs::path& path) {
  if(!path.parent_path().empty()) fs::create_directories(path.parent_path());
}
void checked(const std::ofstream& stream) {
  if(!stream) throw std::runtime_error("Artifact write failed");
}
void le(std::ofstream& out, uint32_t value, unsigned bytes) {
  for(unsigned i = 0; i < bytes; ++i) out.put(char(value >> (8 * i)));
}
// Stream PCM16 so memory consumption is independent of the requested duration.
struct Wav {
  std::ofstream out;
  uint32_t bytes = 0;
  unsigned rate = 0;
  void open(const fs::path& path, unsigned sampleRate) {
    parent(path);
    out.open(path, std::ios::binary | std::ios::trunc);
    checked(out);
    rate = sampleRate;
    out.write("RIFF", 4); le(out, 36, 4); out.write("WAVEfmt ", 8);
    le(out, 16, 4); le(out, 1, 2); le(out, 2, 2); le(out, rate, 4);
    le(out, rate * 4, 4); le(out, 4, 2); le(out, 16, 2);
    out.write("data", 4); le(out, 0, 4);
    checked(out);
  }
  void sample(float value) {
    if(!out.is_open()) return;
    if(bytes > std::numeric_limits<uint32_t>::max() - 38)
      throw std::runtime_error("WAV exceeds RIFF size limit");
    // Nonfinite input is counted as a failure, never converted to an integer.
    const auto scaled = std::lround(std::clamp(std::isfinite(value) ? double(value) : 0.0,
                                             -1.0, 1.0) * 32767.0);
    le(out, uint16_t(int16_t(scaled)), 2);
    bytes += 2;
  }
  void finish() {
    if(!out.is_open()) return;
    out.seekp(4); le(out, bytes + 36, 4);
    out.seekp(40); le(out, bytes, 4);
    out.flush(); checked(out); out.close();
  }
};

struct Stats {
  uint64_t samples = 0, finite = 0, nonzero = 0, clipped = 0, signal = 0;
  double sum = 0, squares = 0, peak = 0;
  void add(float value) {
    ++samples;
    if(!std::isfinite(value)) return;
    ++finite;
    if(value != 0) ++nonzero;
    if(std::abs(value) > 1e-5) ++signal;
    if(std::abs(value) >= 1) ++clipped;
    sum += value;
    squares += double(value) * value;
    peak = std::max(peak, std::abs(double(value)));
  }
  Json json() const {
    return {{"samples", samples}, {"finite", finite}, {"nonfinite", samples - finite},
            {"nonzero", nonzero}, {"clipped", clipped}, {"peak", peak},
            {"above_1e-5", signal},
            {"rms", finite ? std::sqrt(squares / finite) : 0},
            {"ac_rms", finite ? std::sqrt(std::max(0.0, squares / finite - (sum / finite) * (sum / finite))) : 0},
            {"mean", finite ? sum / finite : 0}};
  }
};

uint64_t number(const char* text) {
  std::string input(text);
  if(input.empty() || input.find_first_not_of("0123456789") != std::string::npos)
    throw std::runtime_error("Expected nonnegative decimal integer: " + input);
  return std::stoull(input);
}
void screenshot(const fs::path& path, const clover::AresCore::Frame& frame) {
  parent(path);
  std::ofstream out(path, std::ios::binary | std::ios::trunc);
  out << "P6\n" << frame.width << ' ' << frame.height << "\n255\n";
  for(auto pixel : frame.pixels) {
    out.put(char(pixel >> 16)); out.put(char(pixel >> 8)); out.put(char(pixel));
  }
  out.flush(); checked(out);
}
} // namespace

int main(int argc, char** argv) {
  if(argc < 6) {
    std::cerr << "Usage: game_audit ROM SAVE_DIRECTORY FIRMWARE_DIRECTORY FRAMES REPORT.json [AUDIO.wav]"
                 " [--press-start FRAME] [--press-b FRAME] [--hold-frames N]\n";
    return 2;
  }
  Json report = {{"schema", 1}, {"rom", argv[1]}, {"saves", argv[2]},
                 {"firmware", argv[3]}, {"passed", false}, {"errors", Json::array()}};
  fs::path reportPath(argv[5]), wavPath;
  Wav wav;
  try {
    const auto frames = number(argv[4]);
    if(frames == 0 || frames > 10000000) throw std::runtime_error("FRAMES must be 1..10000000");
    uint64_t start = UINT64_MAX, b = UINT64_MAX, hold = std::min<uint64_t>(12, frames);
    int arg = 6;
    if(arg < argc && std::string(argv[arg]).rfind("--", 0) != 0) wavPath = argv[arg++];
    while(arg < argc) {
      std::string option(argv[arg++]);
      if(arg == argc) throw std::runtime_error("Missing option value: " + option);
      auto value = number(argv[arg++]);
      if(option == "--press-start") start = value;
      else if(option == "--press-b") b = value;
      else if(option == "--hold-frames") hold = value;
      else throw std::runtime_error("Unknown option: " + option);
    }
    if(hold == 0 || hold > frames) throw std::runtime_error("Hold duration must be 1..FRAMES");
    report["requested_frames"] = frames;
    report["input"] = {{"indexing", "zero-based"}, {"hold_frames", hold},
                       {"press_start", start == UINT64_MAX ? Json(nullptr) : Json(start)},
                       {"press_b", b == UINT64_MAX ? Json(nullptr) : Json(b)}};
    clover::AresCore core;
    if(!core.load(argv[1], argv[2], argv[3])) throw std::runtime_error(core.error());
    report["audio_rate"] = core.audioRate();
    report["channels"] = 2;
    if(!wavPath.empty()) {
      wav.open(wavPath, core.audioRate());
      report["audio_wav"] = wavPath.string();
      report["wav_encoding"] = "PCM16 little-endian stereo; nonfinite replaced with zero";
    }
    Stats total, left, right;
    uint64_t completed = 0, audioFrames = 0, nonzeroAudioFrames = 0, nonblackFrames = 0;
    uint64_t invalidVideoFrames = 0, oddAudioFrames = 0, changedFrames = 0;
    uint64_t previousHash = 0;
    Json windows = Json::array(), firstAudio = nullptr, firstVideo = nullptr;
    // At most 120 diagnostic windows even during long runs.
    const uint64_t windowFrames = std::max<uint64_t>(60, (frames + 119) / 120);
    report["audio_window_frames"] = windowFrames;
    Stats window;
    auto imagePath = reportPath; imagePath.replace_extension(".ppm");
    fs::path bestPath = reportPath; bestPath.replace_extension(".nonblack.ppm");
    size_t bestPixels = 0;
    for(uint64_t i = 0; i < frames; ++i) {
      uint16_t mask = 0;
      if(i >= start && i - start < hold) mask |= 1u << 3;
      if(i >= b && i - b < hold) mask |= 1u;
      if(!core.runFrame(mask)) { report["errors"].push_back(core.error()); break; }
      ++completed;
      const auto& audio = core.audio();
      if(!audio.empty()) ++audioFrames;
      if(audio.size() % 2) ++oddAudioFrames;
      bool audible = false;
      for(size_t j = 0; j < audio.size(); ++j) {
        float value = audio[j];
        total.add(value); window.add(value); (j % 2 ? right : left).add(value);
        wav.sample(value);
        if(std::isfinite(value) && value != 0) audible = true;
      }
      if(audible) { ++nonzeroAudioFrames; if(firstAudio.is_null()) firstAudio = i; }
      const auto& video = core.frame();
      bool valid = video.width && video.height && video.pixels.size() == uint64_t(video.width) * video.height;
      if(!valid) ++invalidVideoFrames;
      size_t colored = 0;
      uint64_t hash = 14695981039346656037ull;
      for(auto pixel : video.pixels) {
        if(pixel & 0xffffff) ++colored;
        hash = (hash ^ (pixel & 0xffffff)) * 1099511628211ull;
      }
      if(i && hash != previousHash) ++changedFrames;
      previousHash = hash;
      if(valid && colored) {
        ++nonblackFrames;
        if(firstVideo.is_null()) firstVideo = i;
        if(colored > bestPixels) {
          bestPixels = colored; screenshot(bestPath, video);
          report["video"]["nonblack_capture_frame"] = i;
          report["video"]["nonblack_capture"] = bestPath.string();
        }
      }
      if(i + 1 == frames || (i + 1) % windowFrames == 0) {
        windows.push_back({{"through_frame", i}, {"audio", window.json()}});
        window = {};
      }
    }
    if(completed && !core.frame().pixels.empty()) {
      screenshot(imagePath, core.frame());
      report["video"]["final_capture"] = imagePath.string();
    }
    report["completed_frames"] = completed;
    report["audio"] = total.json();
    report["audio"]["left"] = left.json(); report["audio"]["right"] = right.json();
    report["audio"]["frames_with_buffers"] = audioFrames;
    report["audio"]["frames_nonzero"] = nonzeroAudioFrames;
    report["audio"]["first_nonzero_frame"] = firstAudio;
    report["audio"]["odd_buffers"] = oddAudioFrames;
    report["audio_windows"] = windows;
    report["video"].update({{"width", core.frame().width}, {"height", core.frame().height},
                            {"nonblack_frames", nonblackFrames}, {"changed_frames", changedFrames},
                            {"first_nonblack_frame", firstVideo}, {"invalid_frames", invalidVideoFrames}});
    core.unload();
    if(!core.error().empty()) report["errors"].push_back(core.error());
    report["passed"] = completed == frames && total.samples && total.finite == total.samples
      && total.signal && total.squares / total.finite > 1e-10
      && nonblackFrames && !invalidVideoFrames && !oddAudioFrames
      && report["errors"].empty();
    wav.finish();
  } catch(const std::exception& e) {
    report["errors"].push_back(e.what());
    report["passed"] = false;
    try { wav.finish(); } catch(const std::exception& writeError) { report["errors"].push_back(writeError.what()); }
  }
  try {
    parent(reportPath);
    std::ofstream out(reportPath);
    out << report.dump(2) << '\n'; out.flush(); checked(out);
  } catch(const std::exception& e) { std::cerr << e.what() << '\n'; return 2; }
  std::cout << reportPath << ": " << (report["passed"].get<bool>() ? "PASS" : "FAIL") << '\n';
  return report["passed"].get<bool>() ? 0 : 1;
}
