cmake_minimum_required(VERSION 3.28)
if(CMAKE_HOST_APPLE AND NOT CMAKE_SYSTEM_NAME MATCHES "^(iOS|tvOS)$" AND NOT DEFINED CMAKE_OSX_DEPLOYMENT_TARGET)
  set(CMAKE_OSX_DEPLOYMENT_TARGET "13.0" CACHE STRING "Minimum macOS version")
endif()
if(WIN32 OR CMAKE_HOST_WIN32)
  set(CMAKE_MSVC_RUNTIME_LIBRARY "MultiThreaded$<$<CONFIG:Debug>:Debug>" CACHE STRING "Static compiler runtime")
endif()
project(ClassicMenuNative VERSION 0.5.0 LANGUAGES C CXX)
if(APPLE)
  enable_language(OBJCXX)
  set(CMAKE_OBJCXX_STANDARD 20)
  set(CMAKE_OBJCXX_STANDARD_REQUIRED ON)
endif()
set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
option(CLOVER_BUILD_TESTS "Build native core, menu-adapter and shared Apple input tests" ON)

if(NOT EXISTS "${CMAKE_SOURCE_DIR}/third_party/ares/ares/sfc/sfc.hpp" OR NOT EXISTS "${CMAKE_SOURCE_DIR}/third_party/SDL/CMakeLists.txt")
  message(FATAL_ERROR "Run python3 scripts/bootstrap-portable.py first (pinned build sources)")
endif()

set(SDL_SHARED OFF CACHE BOOL "" FORCE)
set(SDL_STATIC ON CACHE BOOL "" FORCE)
set(SDL_TESTS OFF CACHE BOOL "" FORCE)
set(SDL_EXAMPLES OFF CACHE BOOL "" FORCE)
set(SDL_INSTALL OFF CACHE BOOL "" FORCE)
add_subdirectory(third_party/SDL EXCLUDE_FROM_ALL)

add_subdirectory(portable/core)

# Cargo links these archives outside CMake's dependency graph. Build them on
# every platform, even when no native target (such as AppKit) depends on SDL.
# Keep SDL's unrelated helper targets excluded from the default build.
add_custom_target(clover_native_libraries ALL DEPENDS clover_ares SDL3-static)

if(CLOVER_BUILD_TESTS)
  enable_testing()
  if(NOT CMAKE_SYSTEM_NAME MATCHES "^(iOS|tvOS)$")
    foreach(input_test input touch)
      add_executable(clover_apple_${input_test}_tests portable/apple/tests/${input_test}_tests.cpp)
      target_link_libraries(clover_apple_${input_test}_tests PRIVATE SDL3::SDL3-static)
      add_test(NAME clover_apple_${input_test}_tests COMMAND clover_apple_${input_test}_tests)
      set_tests_properties(clover_apple_${input_test}_tests PROPERTIES
        ENVIRONMENT "SDL_VIDEODRIVER=dummy;SDL_AUDIODRIVER=dummy")
    endforeach()
  endif()
endif()

# Desktop AppKit has no stable C interface for the menu bar. The separate Apple
# mobile host also uses Objective-C++; shared menu state and commands stay Rust.
if(CMAKE_SYSTEM_NAME STREQUAL "Darwin")
  find_library(CLOVER_APPKIT AppKit REQUIRED)
  add_library(clover_native_menu STATIC portable/platform/macos_menu.mm)
  target_include_directories(clover_native_menu PUBLIC portable/platform)
  target_link_libraries(clover_native_menu PUBLIC SDL3::SDL3-static "${CLOVER_APPKIT}")
  target_compile_options(clover_native_menu PRIVATE -fobjc-arc)
  if(CLOVER_BUILD_TESTS)
    add_executable(clover_native_menu_tests portable/platform/macos_menu_tests.mm)
    target_compile_options(clover_native_menu_tests PRIVATE -fobjc-arc)
    target_link_libraries(clover_native_menu_tests PRIVATE clover_native_menu)
    add_test(NAME clover_native_menu_tests COMMAND clover_native_menu_tests)
  endif()
endif()

# The Rust package owns the executable, importer, UI and settings. CMake only
# supplies pinned native libraries and notices for the desktop package. The
# separate portable/apple project owns the experimental mobile app bundles.
install(FILES LICENSE DESTINATION licenses RENAME project-LICENSE)
install(FILES THIRD-PARTY-NOTICES.md dependencies.lock.json DESTINATION licenses)
install(FILES third_party/ares/LICENSE DESTINATION licenses RENAME ares-LICENSE)
install(FILES third_party/SDL/LICENSE.txt DESTINATION licenses RENAME SDL-LICENSE.txt)
install(FILES third_party/SDL/src/hidapi/LICENSE-orig.txt DESTINATION licenses RENAME SDL-HIDAPI-LICENSE.txt)
install(FILES third_party/SDL/src/video/yuv2rgb/LICENSE DESTINATION licenses RENAME SDL-yuv2rgb-LICENSE.txt)
install(FILES portable/core/LICENSE.ares DESTINATION licenses RENAME ares-core-LICENSE)
