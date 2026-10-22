# Build-tree adaptations for the pinned interpreter-only SNES/FC composition.
# Never rewrite the upstream checkout or disguise iOS/tvOS as macOS.
function(clover_replace_once output input needle replacement)
  string(LENGTH "${input}" before)
  string(LENGTH "${needle}" expected)
  string(REPLACE "${needle}" "" removed "${input}")
  string(LENGTH "${removed}" after)
  math(EXPR difference "${before} - ${after}")
  if(NOT difference EQUAL expected)
    message(FATAL_ERROR "Pinned ares Apple compatibility pattern changed: ${needle}")
  endif()
  string(REPLACE "${needle}" "${replacement}" result "${input}")
  set(${output} "${result}" PARENT_SCOPE)
endfunction()

function(clover_prepare_apple_nall ares_root output_dir output_source)
  file(READ "${ares_root}/nall/nall/memory.hpp" memory)
  # PLATFORM_MACOS currently covers all __APPLE__ targets in upstream nall.
  # Limit its JIT write-protection helper to actual macOS, as intended by the
  # availability check in that helper. Other targets retain the non-macOS path.
  clover_replace_once(memory "${memory}"
    "#if defined(PLATFORM_MACOS)"
    "#if defined(PLATFORM_MACOS) && TARGET_OS_OSX")
  file(MAKE_DIRECTORY "${output_dir}/nall")
  file(WRITE "${output_dir}/nall/memory.hpp"
    "// Generated from pinned ares nall/memory.hpp; see ares LICENSE.\n#include <TargetConditionals.h>\n${memory}")

  file(READ "${ares_root}/nall/nall/nall.cpp" runtime)
  # The selected execution cores do not launch subprocesses. Do not compile
  # desktop process-launch helpers forbidden by tvOS; unresolved users will
  # fail at link time, rather than receiving a stubbed success implementation.
  clover_replace_once(runtime "${runtime}" "#include <nall/run.cpp>" "")
  set(source "${output_dir}/nall-runtime.cpp")
  file(WRITE "${source}"
    "// Generated from pinned ares nall/nall.cpp; see ares LICENSE.\n${runtime}")
  set(${output_source} "${source}" PARENT_SCOPE)
endfunction()
