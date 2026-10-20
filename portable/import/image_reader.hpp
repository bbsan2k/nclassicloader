#pragma once

#include <cstdint>
#include <filesystem>
#include <map>
#include <string>
#include <vector>

namespace clover::importer {
using Bytes = std::vector<std::uint8_t>;
using FileMap = std::map<std::string, Bytes>;

// Read selected regular files without extracting anything to the filesystem.
// Supports gzip tar and little-endian SquashFS v4 (zlib compression).
// Throws std::runtime_error on invalid, unsupported, or over-limit input.
FileMap readImage(const std::filesystem::path& source);
} // namespace clover::importer
