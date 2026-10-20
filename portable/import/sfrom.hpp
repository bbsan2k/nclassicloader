#pragma once

#include <cstdint>
#include <string>
#include <vector>
#include <json.hpp>

namespace clover::importer {

struct Conversion {
  std::vector<uint8_t> rom;
  std::string status;
  nlohmann::json metadata;
};

// Converts the official 48-byte-header, trailing-footer SFROM layout only.
// Restores stock PCMF audio only when every decoded first-pass and recorded
// loop-history sample matches exactly. BRR bytes need not be retail-identical.
// Throws std::runtime_error for malformed/unsupported input or any sample
// mismatch. No partial or approximate restoration is returned. See metadata
// for checksum recomputation, header normalization, and runtime-test limits.
Conversion convertSFROM(const std::vector<uint8_t>& bytes);

}  // namespace clover::importer
