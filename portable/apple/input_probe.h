#pragma once
#include "input_sdl.h"

namespace clover::apple {
// Development-only event injection. No direct reducer mutations: synthesized
// keys/touches and SDL's virtual gamepad traverse the application's event callback.
struct InputProbe {
  SDL_JoystickID id = 0;
  SDL_Joystick *joystick = nullptr;
  bool passed = true;
  unsigned checks = 0;
  bool start(Input &input) {
    SDL_VirtualJoystickDesc description;
    SDL_INIT_INTERFACE(&description);
    description.type = SDL_JOYSTICK_TYPE_GAMEPAD;
    description.name = "Clover generated input probe";
    description.nbuttons = 2;
    description.naxes = 2;
    description.button_mask = (1 << SDL_GAMEPAD_BUTTON_SOUTH) | (1 << SDL_GAMEPAD_BUTTON_EAST);
    description.axis_mask = (1 << SDL_GAMEPAD_AXIS_LEFTX) | (1 << SDL_GAMEPAD_AXIS_LEFTY);
    id = SDL_AttachVirtualJoystick(&description);
    if (!id)
      return false;
    joystick = SDL_OpenJoystick(id);
    input.rescan();
    return joystick != nullptr;
  }
  void stop() {
    if (id) {
      passed &= SDL_DetachVirtualJoystick(id);
      id = 0;
    }
    if (joystick) {
      SDL_CloseJoystick(joystick);
      joystick = nullptr;
    }
  }
  void check(bool value) {
    passed &= value;
    ++checks;
  }
  void push(SDL_Event event) {
    passed &= SDL_PushEvent(&event);
  }
  void key(bool down, bool repeat = false) {
    SDL_Event event{};
    event.type = down ? SDL_EVENT_KEY_DOWN : SDL_EVENT_KEY_UP;
    event.key.key = SDLK_Z;
    event.key.down = down;
    event.key.repeat = repeat;
    push(event);
  }
  void finger(Uint32 type, SDL_FingerID id) {
    SDL_Event event{};
    event.type = type;
    event.tfinger.touchID = 7;
    event.tfinger.fingerID = id;
    push(event);
  }
  void focus(bool gained) {
    SDL_Event event{};
    event.type = gained ? SDL_EVENT_WINDOW_FOCUS_GAINED : SDL_EVENT_WINDOW_FOCUS_LOST;
    push(event);
  }
  void step(unsigned callback, Input &input, bool paused) {
    using S = InputState;
    switch (callback) {
    case 10:
      passed &= SDL_SetJoystickVirtualButton(joystick, 0, true);
      break;
    case 20:
      check(input.buttons() == S::B);
      key(true);
      passed &= SDL_SetJoystickVirtualButton(joystick, 0, false);
      break;
    case 30:
      check(input.buttons() == S::B);
      key(false);
      break;
    case 40:
      check(input.buttons() == 0);
      passed &= SDL_SetJoystickVirtualButton(joystick, 0, true);
      passed &= SDL_SetJoystickVirtualAxis(joystick, 0, -20000);
      break;
    case 50:
      check(input.buttons() == (S::B | S::Left));
      stop();
      break;
    case 60:
      check(input.buttons() == 0);
      focus(false);
      break;
    case 65:
      key(true);
      break;
    case 70:
      check(input.buttons() == 0 && paused);
      focus(true);
      key(true, true);
      break;
    case 80:
      check(input.buttons() == 0 && paused);
      key(true);
      break;
    case 90:
      check(input.buttons() == S::B && !paused);
      key(false);
      finger(SDL_EVENT_FINGER_DOWN, 1);
      finger(SDL_EVENT_FINGER_DOWN, 2);
      break;
    case 100:
      check(input.buttons() == S::B);
      finger(SDL_EVENT_FINGER_UP, 1);
      break;
    case 110:
      check(input.buttons() == S::B);
      finger(SDL_EVENT_FINGER_CANCELED, 2);
      break;
    case 120:
      check(input.buttons() == 0);
      passed &= start(input);
      if (joystick)
        passed &= SDL_SetJoystickVirtualButton(joystick, 1, true);
      break;
    case 130:
      check(input.buttons() == S::A);
      break;
    default:
      break;
    }
  }
};
} // namespace clover::apple
