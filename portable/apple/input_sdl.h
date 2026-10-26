#pragma once
#include "input_state.h"
#include <SDL3/SDL.h>
#include <mutex>

namespace clover::apple {
// Handles are managed only by the Apple owner thread. Event reduction is bounded
// and locked separately: never call SDL or Rust while holding the state mutex.
class Input {
  std::array<SDL_Gamepad *, 8> handles{};
  mutable std::mutex mutex;
  InputState state;
  bool initialized = false;
  unsigned blockers = 0;

public:
  bool start() {
    if (!SDL_InitSubSystem(SDL_INIT_GAMEPAD))
      return false;
    initialized = true;
    rescan();
    return true;
  }
  void stop() {
    for (auto &handle : handles)
      if (handle) {
        SDL_CloseGamepad(handle);
        handle = nullptr;
      }
    {
      std::lock_guard lock(mutex);
      state = {};
      blockers = 0;
    }
    if (initialized)
      SDL_QuitSubSystem(SDL_INIT_GAMEPAD);
    initialized = false;
  }
  void rescan() {
    if (!initialized)
      return;
    for (auto &handle : handles)
      if (handle && !SDL_GamepadConnected(handle)) {
        auto id = SDL_GetGamepadID(handle);
        {
          std::lock_guard lock(mutex);
          state.disconnect(id);
        }
        SDL_CloseGamepad(handle);
        handle = nullptr;
      }
    int count = 0;
    auto ids = SDL_GetGamepads(&count);
    for (int i = 0; ids && i < count; ++i) {
      bool owned = false;
      for (auto handle : handles)
        if (handle && SDL_GetGamepadID(handle) == ids[i])
          owned = true;
      if (owned)
        continue;
      for (auto &handle : handles)
        if (!handle) {
          handle = SDL_OpenGamepad(ids[i]);
          if (handle) {
            std::lock_guard lock(mutex);
            state.connect(ids[i]);
          }
          break;
        }
    }
    SDL_free(ids);
  }
  void clear() {
    std::lock_guard lock(mutex);
    state.clear();
  }
  // Independent reason bits: 1 background, 2 audio interruption, 4 lost focus.
  // Check under the same mutex as event reduction, not a stale caller snapshot.
  void block(unsigned reason, bool blocked) {
    std::lock_guard lock(mutex);
    if (blocked) {
      blockers |= reason;
      state.clear();
    } else
      blockers &= ~reason;
  }
  uint16_t buttons() const {
    std::lock_guard lock(mutex);
    return state.buttons();
  }
  bool overlayTouch(uint64_t source, uint64_t finger, uint16_t mask, bool down) {
    std::lock_guard lock(mutex);
    if (down && blockers)
      return false;
    state.touchMask(source, finger, mask, down, true);
    return true;
  }
  static uint16_t key(SDL_Keycode key) {
    using S = InputState;
    switch (key) {
    case SDLK_Z:
      return S::B;
    case SDLK_X:
      return S::A;
    case SDLK_A:
      return S::Y;
    case SDLK_S:
      return S::X;
    case SDLK_Q:
      return S::L;
    case SDLK_W:
      return S::R;
    case SDLK_RETURN:
      return S::Start;
    case SDLK_RSHIFT:
      return S::Select;
    case SDLK_UP:
      return S::Up;
    case SDLK_DOWN:
      return S::Down;
    case SDLK_LEFT:
      return S::Left;
    case SDLK_RIGHT:
      return S::Right;
    default:
      return 0;
    }
  }
  static uint16_t button(Uint8 button) {
    using S = InputState;
    switch (button) {
    case SDL_GAMEPAD_BUTTON_SOUTH:
      return S::B;
    case SDL_GAMEPAD_BUTTON_EAST:
      return S::A;
    case SDL_GAMEPAD_BUTTON_WEST:
      return S::Y;
    case SDL_GAMEPAD_BUTTON_NORTH:
      return S::X;
    case SDL_GAMEPAD_BUTTON_LEFT_SHOULDER:
      return S::L;
    case SDL_GAMEPAD_BUTTON_RIGHT_SHOULDER:
      return S::R;
    case SDL_GAMEPAD_BUTTON_START:
      return S::Start;
    case SDL_GAMEPAD_BUTTON_BACK:
      return S::Select;
    case SDL_GAMEPAD_BUTTON_DPAD_UP:
      return S::Up;
    case SDL_GAMEPAD_BUTTON_DPAD_DOWN:
      return S::Down;
    case SDL_GAMEPAD_BUTTON_DPAD_LEFT:
      return S::Left;
    case SDL_GAMEPAD_BUTTON_DPAD_RIGHT:
      return S::Right;
    default:
      return 0;
    }
  }
  // Returns true only for a deliberate, supported press (never key repeat).
  bool event(const SDL_Event &event, bool accept) {
    std::lock_guard lock(mutex);
    if (event.type == SDL_EVENT_GAMEPAD_REMOVED) {
      state.disconnect(event.gdevice.which);
      return false;
    }
    if (event.type == SDL_EVENT_GAMEPAD_REMAPPED) {
      state.remap(event.gdevice.which);
      return false;
    }
    if (!accept || blockers)
      return false;
    switch (event.type) {
    case SDL_EVENT_KEY_DOWN:
    case SDL_EVENT_KEY_UP:
      if (event.key.down && event.key.repeat)
        return false;
      state.key(key(event.key.key), event.key.down);
      return event.key.down && key(event.key.key);
    case SDL_EVENT_GAMEPAD_BUTTON_DOWN:
    case SDL_EVENT_GAMEPAD_BUTTON_UP:
      if (!state.connected(event.gbutton.which))
        return false;
      state.button(event.gbutton.which, button(event.gbutton.button), event.gbutton.down);
      return event.gbutton.down && button(event.gbutton.button);
    case SDL_EVENT_GAMEPAD_AXIS_MOTION:
      if (event.gaxis.axis == SDL_GAMEPAD_AXIS_LEFTX || event.gaxis.axis == SDL_GAMEPAD_AXIS_LEFTY)
        state.axis(event.gaxis.which, event.gaxis.axis == SDL_GAMEPAD_AXIS_LEFTX,
                   event.gaxis.value);
      return false;
    case SDL_EVENT_FINGER_DOWN:
    case SDL_EVENT_FINGER_UP:
    case SDL_EVENT_FINGER_CANCELED:
      state.touch(event.tfinger.touchID, event.tfinger.fingerID,
                  event.type == SDL_EVENT_FINGER_DOWN);
      return event.type == SDL_EVENT_FINGER_DOWN;
    default:
      return false;
    }
  }
};
} // namespace clover::apple
