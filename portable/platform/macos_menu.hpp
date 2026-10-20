#pragma once
#include <SDL3/SDL.h>

namespace clover::ui {
enum class MenuCommand : int { library=1, importImage, settings, chooseROM, firmware, sort, sound, fullscreen, help, quit };
inline Uint32 nativeMenuEvent() {
  static Uint32 event=SDL_RegisterEvents(1);
  return event;
}
#ifdef __APPLE__
void installNativeMenu(bool inLauncher=false, bool launcherFullscreen=false);
#else
inline void installNativeMenu(bool=false, bool=false) {}
#endif
}
