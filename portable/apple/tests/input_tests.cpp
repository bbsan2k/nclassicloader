#include "../input_sdl.h"
#include <stdexcept>
#include <iostream>

using clover::apple::Input;
using S = clover::apple::InputState;
static void check(bool value, const char *message) {
  if (!value)
    throw std::runtime_error(message);
}

static void reducerTests() {
  check(Input::key(SDLK_A) == S::Y && Input::key(SDLK_S) == S::X && Input::key(SDLK_Q) == S::L &&
            Input::key(SDLK_W) == S::R,
        "SNES keyboard mappings");
  check(Input::button(SDL_GAMEPAD_BUTTON_WEST) == S::Y &&
            Input::button(SDL_GAMEPAD_BUTTON_NORTH) == S::X &&
            Input::button(SDL_GAMEPAD_BUTTON_LEFT_SHOULDER) == S::L &&
            Input::button(SDL_GAMEPAD_BUTTON_RIGHT_SHOULDER) == S::R,
        "SNES gamepad mappings");
  S state;
  check(state.connect(1) && state.connect(2), "connect sources");
  state.key(S::B, true);
  state.button(1, S::B, true);
  state.button(1, S::B, false);
  check(state.buttons() == S::B, "controller release must retain keyboard");
  state.key(S::B, false);
  state.button(1, S::B, true);
  state.button(2, S::B, true);
  state.button(1, S::B, false);
  check(state.buttons() == S::B, "controller sources must remain independent");
  state.button(2, S::B, false);
  state.key(S::B, true);
  state.touch(1, 42, true);
  state.key(S::B, false);
  state.touch(2, 42, true);
  state.touch(1, 42, false);
  check(state.buttons() == S::B, "touch devices/fingers must be independent");
  state.touch(2, 42, false);
  check(state.buttons() == 0, "all B sources released");
  state.axis(1, true, -20000);
  state.axis(1, true, -13000);
  check(state.buttons() == S::Left, "axis hysteresis hold");
  state.key(S::Right, true);
  check(state.buttons() == 0, "opposite directions neutral");
  state.key(S::Right, false);
  state.axis(1, true, -10000);
  check(state.buttons() == 0, "axis deadzone release");
  state.button(1, S::A, true);
  state.button(2, S::Start, true);
  state.disconnect(1);
  check(state.buttons() == S::Start, "disconnect only clears removed device");
  state.remap(2);
  check(state.buttons() == 0, "remap clears old mapping without removing the device");
  state.button(2, S::Start, true);
  check(state.buttons() == S::Start, "remapped device remains registered");
  state.clear();
  check(state.buttons() == 0, "pause clears every source");
  state.button(1, S::B, true);
  check(state.buttons() == 0, "disconnected input ignored");
  for (unsigned id = 3; id <= 9; ++id)
    state.connect(id);
  check(!state.connect(10), "bounded controller slots");
  for (unsigned id = 0; id < 10; ++id)
    state.touch(7, id, true);
  state.touch(7, 99, true);
  for (unsigned id = 0; id < 10; ++id)
    state.touch(7, id, false);
  check(state.buttons() == 0, "bounded finger slots do not leave phantom presses");
}

static void virtualGamepadTests() {
  check(SDL_Init(SDL_INIT_VIDEO), SDL_GetError());
  Input input;
  check(input.start(), SDL_GetError());
  auto pump = [&] {
    SDL_UpdateJoysticks();
    SDL_Event event;
    while (SDL_PollEvent(&event)) {
      if (event.type == SDL_EVENT_GAMEPAD_ADDED || event.type == SDL_EVENT_GAMEPAD_REMOVED)
        input.rescan();
      input.event(event, true);
    }
  };
  SDL_VirtualJoystickDesc description;
  SDL_INIT_INTERFACE(&description);
  description.type = SDL_JOYSTICK_TYPE_GAMEPAD;
  description.name = "Clover self-authored virtual input test";
  description.nbuttons = 2;
  description.naxes = 2;
  description.button_mask = (1 << SDL_GAMEPAD_BUTTON_SOUTH) | (1 << SDL_GAMEPAD_BUTTON_EAST);
  description.axis_mask = (1 << SDL_GAMEPAD_AXIS_LEFTX) | (1 << SDL_GAMEPAD_AXIS_LEFTY);
  auto id = SDL_AttachVirtualJoystick(&description);
  check(id != 0, SDL_GetError());
  auto joystick = SDL_OpenJoystick(id);
  check(joystick != nullptr, SDL_GetError());
  pump();
  input.rescan();
  check(SDL_SetJoystickVirtualButton(joystick, 0, true), SDL_GetError());
  pump();
  check(input.buttons() == S::B, "real virtual-gamepad event must reach reducer");
  SDL_Event key{};
  key.type = SDL_EVENT_KEY_DOWN;
  key.key.key = SDLK_Z;
  key.key.down = true;
  check(input.event(key, true), "supported key press");
  check(SDL_SetJoystickVirtualButton(joystick, 0, false), SDL_GetError());
  pump();
  check(input.buttons() == S::B, "SDL controller release retains keyboard");
  key.type = SDL_EVENT_KEY_UP;
  key.key.down = false;
  input.event(key, true);
  check(input.buttons() == 0, "keyboard released");
  key.type = SDL_EVENT_KEY_DOWN;
  key.key.down = true;
  key.key.repeat = true;
  check(!input.event(key, true) && input.buttons() == 0,
        "key repeat cannot resume or restore stale input");
  key.key.repeat = false;
  check(!input.event(key, false) && input.buttons() == 0, "blocked input ignored");
  input.block(1, true);
  input.block(2, true);
  input.block(1, false);
  check(!input.event(key, true) && input.buttons() == 0,
        "independent interruption blocker survives foreground");
  input.block(2, false);
  check(input.event(key, true), "fresh press accepted after blockers clear");
  input.clear();
  check(input.buttons() == 0, "cleared input does not reappear");
  SDL_Event touch{};
  touch.type = SDL_EVENT_FINGER_DOWN;
  touch.tfinger.touchID = 7;
  touch.tfinger.fingerID = 1;
  input.event(touch, true);
  touch.tfinger.fingerID = 2;
  input.event(touch, true);
  touch.type = SDL_EVENT_FINGER_CANCELED;
  input.event(touch, true);
  check(input.buttons() == S::B, "cancel clears only the affected finger");
  touch.type = SDL_EVENT_FINGER_UP;
  touch.tfinger.fingerID = 1;
  input.event(touch, true);
  check(input.buttons() == 0, "last touch release clears B");
  check(SDL_SetJoystickVirtualAxis(joystick, 0, -20000), SDL_GetError());
  pump();
  check(input.buttons() == S::Left, "virtual analog stick mapping");
  check(SDL_SetJoystickVirtualButton(joystick, 0, true), SDL_GetError());
  pump();
  check(SDL_DetachVirtualJoystick(id), SDL_GetError());
  pump();
  check(input.buttons() == 0, "real detach events clear held buttons and axes");
  SDL_CloseJoystick(joystick);
  id = SDL_AttachVirtualJoystick(&description);
  check(id != 0, SDL_GetError());
  pump();
  check(input.buttons() == 0, "reconnected controller must start neutral");
  check(SDL_DetachVirtualJoystick(id), SDL_GetError());
  pump();
  input.stop();
  SDL_Quit();
}

int main() {
  try {
    reducerTests();
    virtualGamepadTests();
    std::cout << "Apple source-aware input and SDL virtual-gamepad checks passed\n";
  } catch (const std::exception &error) {
    std::cerr << error.what() << "\n";
    return 1;
  }
}
