#include "smoke_entry.h"

#include <chrono>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <stdexcept>

namespace fs = std::filesystem;

static void require(bool value, const char* message) {
  if(!value) throw std::runtime_error(message);
}

static void invalid_inputs(int (*smoke)(const char*, char*, size_t), const fs::path& root) {
  char error[64] = {};
  require(smoke(nullptr, error, sizeof(error)) == 1 && error[0], "null root diagnostic");
  error[0] = '\0';
  require(smoke("", error, sizeof(error)) == 1 && error[0], "empty root diagnostic");
  error[0] = '\0';
  require(smoke("relative-root", error, sizeof(error)) == 1 && error[0], "relative root diagnostic");
  auto file = root / "not-a-directory";
  { std::ofstream out(file); out << "x"; }
  error[0] = '\0';
  require(smoke(file.string().c_str(), error, sizeof(error)) == 1 && error[0], "unwritable parent diagnostic");
  char short_error[2] = {'x', 'x'};
  require(smoke(nullptr, short_error, sizeof(short_error)) == 1 && short_error[1] == '\0', "short error buffer termination");
  require(smoke(nullptr, nullptr, 0) == 1, "null error buffer");
  require(smoke(nullptr, short_error, 0) == 1, "zero-sized error buffer");
}

int main() {
  try {
    auto root = fs::temp_directory_path() / ("clover-smoke-entry-" + std::to_string(
      std::chrono::steady_clock::now().time_since_epoch().count()));
    fs::create_directories(root);
    invalid_inputs(clover_snes_smoke, root);
    invalid_inputs(clover_nes_smoke, root);
    std::cout << "PASS: callable smoke entry validation\n";
    return 0;
  } catch(const std::exception& e) {
    std::cerr << "FAIL: " << e.what() << '\n';
    return 1;
  }
}
