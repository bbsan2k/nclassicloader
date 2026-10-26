cmake_minimum_required(VERSION 3.20)
if(CMAKE_SOURCE_DIR STREQUAL CMAKE_CURRENT_SOURCE_DIR)
  project(clover_ares_bridge LANGUAGES C CXX)
endif()
enable_language(C)

set(CLOVER_ARES_SOURCE_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../../third_party/ares" CACHE PATH "Pinned ares source tree")
get_filename_component(ARES_ROOT "${CLOVER_ARES_SOURCE_DIR}" ABSOLUTE)
set(ARES_REVISION "9408cb43d4948fc3ea6e152a307a34348df3fe04")
set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS
  "${ARES_ROOT}/mia/medium/super-famicom.cpp"
  "${ARES_ROOT}/ares/System/Super Famicom/boards.bml")

# Reuse the exact upstream heuristic analyzer, but NOT mia's firmware resources,
# database loader, archive handlers, or desktop dependencies. Keep generated code
# in the build tree and leave the pinned upstream tree untouched.
file(READ "${ARES_ROOT}/mia/medium/super-famicom.cpp" ANALYZER)
string(FIND "${ANALYZER}" "auto SuperFamicom::load(" DECL_END)
string(SUBSTRING "${ANALYZER}" 0 ${DECL_END} DECL)
string(REPLACE " : Cartridge" "" DECL "${DECL}")
string(REPLACE " override" "" DECL "${DECL}")
string(REGEX REPLACE "  auto load\\(string location\\) -> LoadResult;\n" "" DECL "${DECL}")
string(REGEX REPLACE "  auto save\\(string location\\) -> bool;\n" "" DECL "${DECL}")
string(REPLACE "  std::vector<u8> rom;" "  string location = \"cartridge\", sha256;\n  std::vector<u8> rom;" DECL "${DECL}")
string(FIND "${ANALYZER}" "auto SuperFamicom::analyze(" IMPL_START)
string(SUBSTRING "${ANALYZER}" ${IMPL_START} -1 ANALYZER_IMPL)
file(WRITE "${CMAKE_CURRENT_BINARY_DIR}/sfc-analyzer.inc"
  "// Generated from ares mia/medium/super-famicom.cpp; see ares LICENSE.\n${DECL}\n${ANALYZER_IMPL}")
file(READ "${ARES_ROOT}/ares/System/Super Famicom/boards.bml" BOARDS)
file(WRITE "${CMAKE_CURRENT_BINARY_DIR}/sfc-boards.inc" "R\"CLOVER_BOARDS(${BOARDS})CLOVER_BOARDS\"")
set(SPRITES "// Generated from upstream ares sprite PNGs; see ares LICENSE.\n#pragma once\nnamespace Resource::Sprite::SuperFamicom {\n")
foreach(COLOR Green Red)
  string(TOLOWER "${COLOR}" COLOR_LOWER)
  file(READ "${ARES_ROOT}/ares/ares/resource/sprite/sfc/crosshair-${COLOR_LOWER}.png" PNG HEX)
  string(REGEX REPLACE "([0-9a-f][0-9a-f])" "0x\\1," PNG "${PNG}")
  string(APPEND SPRITES "inline const unsigned char Crosshair${COLOR}[] = {${PNG}};\n")
endforeach()
string(APPEND SPRITES "}\n")
file(READ "${ARES_ROOT}/ares/ares/resource/sprite/fc/crosshair.png" PNG HEX)
string(REGEX REPLACE "([0-9a-f][0-9a-f])" "0x\\1," PNG "${PNG}")
string(APPEND SPRITES "namespace Resource::Sprite::Famicom {\ninline const unsigned char Crosshair[] = {${PNG}};\n}\n")
file(WRITE "${CMAKE_CURRENT_BINARY_DIR}/ares/resource/resource.hpp" "${SPRITES}")

set(NALL_SOURCE "${ARES_ROOT}/nall/nall/nall.cpp")
if(CMAKE_SYSTEM_NAME STREQUAL "iOS" OR CMAKE_SYSTEM_NAME STREQUAL "tvOS")
  set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS
    "${ARES_ROOT}/nall/nall/memory.hpp" "${ARES_ROOT}/nall/nall/nall.cpp")
  include("${CMAKE_CURRENT_SOURCE_DIR}/apple_compat.cmake")
  clover_prepare_apple_nall("${ARES_ROOT}" "${CMAKE_CURRENT_BINARY_DIR}/apple-compat" NALL_SOURCE)
endif()

add_library(clover_ares STATIC
  ares_core.cpp ares_runtime.cpp ares_c_api.cpp
  "${NALL_SOURCE}"
  "${ARES_ROOT}/libco/libco.c"
  "${ARES_ROOT}/ares/ares/memory/fixed-allocator.cpp"
  "${ARES_ROOT}/ares/sfc/sfc.cpp"
  "${ARES_ROOT}/ares/fc/fc.cpp")
foreach(COMPONENT wdc65816 spc700 arm7tdmi gsu hg51b upd96050)
  target_sources(clover_ares PRIVATE "${ARES_ROOT}/ares/component/processor/${COMPONENT}/${COMPONENT}.cpp")
endforeach()
foreach(COMPONENT processor/mos6502 audio/ym2149 audio/ym2413 eeprom/m24c flash/sst39sf0x0)
  get_filename_component(NAME "${COMPONENT}" NAME)
  target_sources(clover_ares PRIVATE "${ARES_ROOT}/ares/component/${COMPONENT}/${NAME}.cpp")
endforeach()
# The pinned FC compilation unit also contains EPSM; retain its upstream ymfm
# dependencies even though expansion peripherals are not exposed by this bridge.
foreach(COMPONENT opn adpcm ssg)
  target_sources(clover_ares PRIVATE "${ARES_ROOT}/thirdparty/ymfm/src/ymfm_${COMPONENT}.cpp")
endforeach()
target_compile_features(clover_ares PUBLIC cxx_std_20)
target_include_directories(clover_ares PUBLIC "${CMAKE_CURRENT_SOURCE_DIR}")
target_include_directories(clover_ares SYSTEM PRIVATE
  "${ARES_ROOT}" "${ARES_ROOT}/ares" "${ARES_ROOT}/nall" "${ARES_ROOT}/thirdparty"
  "${ARES_ROOT}/thirdparty/ymfm/src")
target_include_directories(clover_ares PRIVATE "${CMAKE_CURRENT_BINARY_DIR}")
if(CMAKE_SYSTEM_NAME STREQUAL "iOS" OR CMAKE_SYSTEM_NAME STREQUAL "tvOS")
  target_include_directories(clover_ares BEFORE PRIVATE "${CMAKE_CURRENT_BINARY_DIR}/apple-compat")
endif()
target_compile_definitions(clover_ares PRIVATE CORE_SFC CORE_FC PROFILE_PERFORMANCE
  CLOVER_ARES_REVISION="${ARES_REVISION}")
if(MSVC)
  target_compile_options(clover_ares PRIVATE /bigobj /Zc:preprocessor /W0)
else()
  target_compile_options(clover_ares PRIVATE -Wno-deprecated-declarations -Wno-multichar)
endif()
find_package(Threads REQUIRED)
target_link_libraries(clover_ares PUBLIC Threads::Threads ${CMAKE_DL_LIBS})
if(WIN32)
  target_link_libraries(clover_ares PRIVATE ws2_32 winmm ole32 shell32 shlwapi advapi32 user32 uuid)
elseif(APPLE)
  find_library(CLOVER_COREFOUNDATION CoreFoundation REQUIRED)
  target_link_libraries(clover_ares PRIVATE "${CLOVER_COREFOUNDATION}")
endif()

option(CLOVER_BUILD_TESTS "Build the self-authored SNES and NES bridge smoke tests" OFF)
option(CLOVER_BUILD_APP_SMOKE "Build callable smoke tests for a host application" OFF)
if(CLOVER_BUILD_APP_SMOKE)
  add_library(clover_core_smoke_support STATIC tests/smoke.cpp tests/nes_smoke.cpp)
  target_compile_definitions(clover_core_smoke_support PRIVATE CLOVER_SMOKE_EMBEDDED)
  target_include_directories(clover_core_smoke_support PUBLIC "${CMAKE_CURRENT_SOURCE_DIR}/tests")
  target_link_libraries(clover_core_smoke_support PUBLIC clover_ares)
endif()
if(CLOVER_BUILD_TESTS)
  enable_testing()
  add_executable(clover_ares_smoke tests/smoke.cpp)
  target_link_libraries(clover_ares_smoke PRIVATE clover_ares)
  add_test(NAME clover_ares_smoke COMMAND clover_ares_smoke)
  add_executable(clover_ares_nes_smoke tests/nes_smoke.cpp)
  target_link_libraries(clover_ares_nes_smoke PRIVATE clover_ares)
  add_test(NAME clover_ares_nes_smoke COMMAND clover_ares_nes_smoke)
  if(CLOVER_BUILD_APP_SMOKE)
    add_executable(clover_core_smoke_entry_tests tests/smoke_entry_tests.cpp)
    target_link_libraries(clover_core_smoke_entry_tests PRIVATE clover_core_smoke_support)
    add_test(NAME clover_core_smoke_entry_tests COMMAND clover_core_smoke_entry_tests)
  endif()
endif()
