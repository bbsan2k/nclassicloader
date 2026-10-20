#pragma once
#include "image_reader.hpp"
#include <json.hpp>
#include <filesystem>

namespace clover::importer {
void prepareSkin(const FileMap& files,const std::string& ui,const std::filesystem::path& output);
}
