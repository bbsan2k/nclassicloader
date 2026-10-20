#pragma once
#include <filesystem>
#include <functional>
#include <string>
#include <vector>
#include <json.hpp>

namespace clover::importer {
struct Profile {
  std::filesystem::path directory;
  nlohmann::json manifest;
  std::string label() const;
  std::filesystem::path assets() const { return directory/"assets"; }
  std::filesystem::path data() const { return directory/"data"; }
};
using Progress=std::function<void(const std::string&)>;
// Sources are read only. Boot images require a matching local filesystem sibling.
std::filesystem::path resolveFilesystem(const std::filesystem::path& source);
Profile importImage(const std::filesystem::path& source,const std::filesystem::path& cache,const Progress& progress={});
std::vector<Profile> listProfiles(const std::filesystem::path& cache);
std::vector<std::filesystem::path> discoverImages(const std::vector<std::filesystem::path>& directories);
}
