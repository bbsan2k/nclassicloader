cmake_minimum_required(VERSION 3.28)
project(CloverAppleHost LANGUAGES C CXX OBJCXX)

if(NOT CMAKE_SYSTEM_NAME STREQUAL "iOS" AND NOT CMAKE_SYSTEM_NAME STREQUAL "tvOS")
  message(FATAL_ERROR "Configure this app with an iOS or tvOS SDK")
endif()

set(CLOVER_BUILD_APP_SMOKE ON CACHE BOOL "Build app-callable generated-ROM checks" FORCE)
set(CLOVER_BUILD_TESTS OFF CACHE BOOL "Standalone tests cannot run as mobile CLI programs" FORCE)
add_subdirectory(../core "${CMAKE_CURRENT_BINARY_DIR}/core")

find_library(CLOVER_UIKIT UIKit REQUIRED)
find_library(CLOVER_FOUNDATION Foundation REQUIRED)
add_executable(clover_apple_smoke MACOSX_BUNDLE smoke_main.mm)
target_compile_features(clover_apple_smoke PRIVATE cxx_std_20)
target_compile_options(clover_apple_smoke PRIVATE -fobjc-arc)
target_link_libraries(clover_apple_smoke PRIVATE
  clover_core_smoke_support "${CLOVER_UIKIT}" "${CLOVER_FOUNDATION}")
if(CMAKE_SYSTEM_NAME STREQUAL "tvOS")
  target_compile_definitions(clover_apple_smoke PRIVATE CLOVER_APPLE_PLATFORM="tvos")
  set(CLOVER_DEVICE_FAMILY "3")
else()
  target_compile_definitions(clover_apple_smoke PRIVATE CLOVER_APPLE_PLATFORM="ios")
  set(CLOVER_DEVICE_FAMILY "1,2")
endif()
set_target_properties(clover_apple_smoke PROPERTIES
  OUTPUT_NAME "CloverCoreSmoke"
  MACOSX_BUNDLE_GUI_IDENTIFIER "org.cloverproject.core-smoke"
  XCODE_ATTRIBUTE_PRODUCT_BUNDLE_IDENTIFIER "org.cloverproject.core-smoke"
  MACOSX_BUNDLE_INFO_PLIST "${CMAKE_CURRENT_SOURCE_DIR}/SmokeInfo.plist.in"
  XCODE_ATTRIBUTE_TARGETED_DEVICE_FAMILY "${CLOVER_DEVICE_FAMILY}")

option(CLOVER_BUILD_RUST_PREVIEW "Build the SDL callback / Rust shared-scene preview" OFF)
if(CLOVER_BUILD_RUST_PREVIEW)
  set(SDL_SHARED OFF CACHE BOOL "" FORCE)
  set(SDL_STATIC ON CACHE BOOL "" FORCE)
  set(SDL_TESTS OFF CACHE BOOL "" FORCE)
  set(SDL_EXAMPLES OFF CACHE BOOL "" FORCE)
  add_subdirectory(../../third_party/SDL "${CMAKE_CURRENT_BINARY_DIR}/third_party/SDL" EXCLUDE_FROM_ALL)
  find_program(CLOVER_CARGO cargo REQUIRED)
  if(CMAKE_SYSTEM_NAME STREQUAL "tvOS")
    set(CLOVER_RUST_TARGET aarch64-apple-tvos-sim)
    set(CLOVER_SDK appletvsimulator)
    set(CLOVER_PLATFORM tvos)
  else()
    set(CLOVER_RUST_TARGET aarch64-apple-ios-sim)
    set(CLOVER_SDK iphonesimulator)
    set(CLOVER_PLATFORM ios)
  endif()
  if(NOT CMAKE_OSX_SYSROOT MATCHES "[Ss]imulator")
    message(FATAL_ERROR "The first Rust preview host is simulator-only; device signing is a separate gate")
  endif()
  execute_process(COMMAND xcrun --sdk "${CLOVER_SDK}" --show-sdk-path
    OUTPUT_VARIABLE CLOVER_SDKROOT OUTPUT_STRIP_TRAILING_WHITESPACE COMMAND_ERROR_IS_FATAL ANY)
  set(CLOVER_RUST_OUTPUT "${CMAKE_CURRENT_BINARY_DIR}/rust-target")
  set(CLOVER_RUST_ARCHIVE "${CLOVER_RUST_OUTPUT}/${CLOVER_RUST_TARGET}/release/libclover_portable.a")
  file(GLOB_RECURSE CLOVER_RUST_SOURCES CONFIGURE_DEPENDS
    "${CMAKE_CURRENT_SOURCE_DIR}/../../rust/src/*.rs" "${CMAKE_CURRENT_SOURCE_DIR}/../../rust/src/*.html")
  add_custom_command(OUTPUT "${CLOVER_RUST_ARCHIVE}"
    COMMAND "${CMAKE_COMMAND}" -E env
      "SDKROOT=${CLOVER_SDKROOT}" "CARGO_TARGET_DIR=${CLOVER_RUST_OUTPUT}"
      "IPHONEOS_DEPLOYMENT_TARGET=${CMAKE_OSX_DEPLOYMENT_TARGET}"
      "TVOS_DEPLOYMENT_TARGET=${CMAKE_OSX_DEPLOYMENT_TARGET}"
      "CLOVER_NATIVE_BUILD_DIR=${CMAKE_CURRENT_BINARY_DIR}/core/$<CONFIG>-${CLOVER_SDK}"
      "${CLOVER_CARGO}" build --release --locked --lib --no-default-features
      --features apple-game,xz --target "${CLOVER_RUST_TARGET}"
      --manifest-path "${CMAKE_CURRENT_SOURCE_DIR}/../../rust/Cargo.toml"
    DEPENDS clover_ares ${CLOVER_RUST_SOURCES} ../../rust/Cargo.toml ../../rust/Cargo.lock ../../rust/build.rs
    VERBATIM)
  add_custom_target(clover_rust_preview_build DEPENDS "${CLOVER_RUST_ARCHIVE}")
  add_library(clover_rust_preview STATIC IMPORTED)
  set_target_properties(clover_rust_preview PROPERTIES IMPORTED_LOCATION "${CLOVER_RUST_ARCHIVE}")
  add_dependencies(clover_rust_preview clover_rust_preview_build)
  add_executable(clover_apple_preview MACOSX_BUNDLE preview_main.mm document_import.mm touch_overlay.mm profile_launcher.mm)
  target_compile_features(clover_apple_preview PRIVATE cxx_std_20)
  target_compile_options(clover_apple_preview PRIVATE -fobjc-arc)
  target_compile_definitions(clover_apple_preview PRIVATE CLOVER_APPLE_PLATFORM="${CLOVER_PLATFORM}")
  target_link_libraries(clover_apple_preview PRIVATE clover_rust_preview SDL3::SDL3-static
    "${CLOVER_UIKIT}" "${CLOVER_FOUNDATION}" "-framework AVFAudio" "-framework CoreImage" "-framework UniformTypeIdentifiers" "-framework Security" "-framework SystemConfiguration" iconv resolv)
  set_target_properties(clover_apple_preview PROPERTIES OUTPUT_NAME "CloverApplePreview"
    MACOSX_BUNDLE_GUI_IDENTIFIER "org.cloverproject.apple-preview"
    XCODE_ATTRIBUTE_PRODUCT_BUNDLE_IDENTIFIER "org.cloverproject.apple-preview"
    MACOSX_BUNDLE_INFO_PLIST "${CMAKE_CURRENT_SOURCE_DIR}/PreviewInfo.plist.in"
    XCODE_ATTRIBUTE_TARGETED_DEVICE_FAMILY "${CLOVER_DEVICE_FAMILY}")
endif()

file(MAKE_DIRECTORY "${CMAKE_CURRENT_BINARY_DIR}/licenses")
configure_file(../../LICENSE "${CMAKE_CURRENT_BINARY_DIR}/licenses/project-LICENSE.txt" COPYONLY)
configure_file(../../THIRD-PARTY-NOTICES.md "${CMAKE_CURRENT_BINARY_DIR}/licenses/THIRD-PARTY-NOTICES.md" COPYONLY)
configure_file(../core/LICENSE.ares "${CMAKE_CURRENT_BINARY_DIR}/licenses/ares-core-LICENSE.txt" COPYONLY)
configure_file(../../third_party/ares/LICENSE "${CMAKE_CURRENT_BINARY_DIR}/licenses/ares-LICENSE.txt" COPYONLY)
configure_file(../../third_party/SDL/LICENSE.txt "${CMAKE_CURRENT_BINARY_DIR}/licenses/SDL-LICENSE.txt" COPYONLY)
configure_file(../../third_party/SDL/src/hidapi/LICENSE-orig.txt "${CMAKE_CURRENT_BINARY_DIR}/licenses/SDL-HIDAPI-LICENSE.txt" COPYONLY)
configure_file(../../third_party/SDL/src/video/yuv2rgb/LICENSE "${CMAKE_CURRENT_BINARY_DIR}/licenses/SDL-yuv2rgb-LICENSE.txt" COPYONLY)
if(CLOVER_BUILD_RUST_PREVIEW)
  find_package(Python3 REQUIRED COMPONENTS Interpreter)
  execute_process(COMMAND "${Python3_EXECUTABLE}"
    "${CMAKE_CURRENT_SOURCE_DIR}/../../scripts/rust_licenses.py"
    "${CMAKE_CURRENT_BINARY_DIR}/licenses" --features apple-game,xz
    COMMAND_ERROR_IS_FATAL ANY)
  set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS
    "${CMAKE_CURRENT_SOURCE_DIR}/../../rust/Cargo.lock"
    "${CMAKE_CURRENT_SOURCE_DIR}/../../scripts/rust_licenses.py")
endif()
file(GLOB_RECURSE CLOVER_APP_NOTICES LIST_DIRECTORIES false "${CMAKE_CURRENT_BINARY_DIR}/licenses/*")
foreach(notice IN LISTS CLOVER_APP_NOTICES)
  file(RELATIVE_PATH relative "${CMAKE_CURRENT_BINARY_DIR}/licenses" "${notice}")
  get_filename_component(directory "${relative}" DIRECTORY)
  set_source_files_properties("${notice}" PROPERTIES MACOSX_PACKAGE_LOCATION "Resources/licenses/${directory}")
endforeach()
target_sources(clover_apple_smoke PRIVATE ${CLOVER_APP_NOTICES})
if(TARGET clover_apple_preview)
  target_sources(clover_apple_preview PRIVATE ${CLOVER_APP_NOTICES})
endif()
