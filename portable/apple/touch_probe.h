#pragma once
#import "touch_overlay.h"
#include "input_sdl.h"

namespace clover::apple {
struct TouchProbe {
  bool passed = true;
  unsigned checks = 0;
  void check(bool value) {
    passed &= value;
    ++checks;
  }
  void event(Uint32 type) {
    SDL_Event event{};
    event.type = type;
    passed &= SDL_PushEvent(&event);
  }
  void key(bool down) {
    SDL_Event event{};
    event.type = down ? SDL_EVENT_KEY_DOWN : SDL_EVENT_KEY_UP;
    event.key.key = SDLK_Z;
    event.key.down = down;
    passed &= SDL_PushEvent(&event);
  }
  void step(unsigned callback, CloverTouchOverlay *overlay, Input &input, bool paused) {
    using S = InputState;
    float width = overlay.probeWidth;
    switch (callback) {
    case 10:
      [overlay probeContact:101 phase:@"begin" x:66 y:26];
      [overlay probeContact:102 phase:@"begin" x:width - 124 y:94];
      break;
    case 20:
      check(input.buttons() == (S::Up | S::B));
      [overlay probeContact:102 phase:@"move" x:width - 46 y:50];
      break;
    case 30:
      check(input.buttons() == (S::Up | S::A));
      [overlay probeContact:101 phase:@"end" x:0 y:0];
      break;
    case 40:
      check(input.buttons() == S::A);
      [overlay probeContact:103 phase:@"begin" x:width - 46 y:50];
      [overlay probeContact:102 phase:@"end" x:0 y:0];
      break;
    case 50:
      check(input.buttons() == S::A);
      [overlay probeContact:103 phase:@"move" x:width / 2 y:30];
      break;
    case 60:
      check(input.buttons() == 0);
      [overlay probeContact:103 phase:@"move" x:width / 2 + 36 y:120];
      break;
    case 70:
      check(input.buttons() == S::Start);
      [overlay probeContact:103 phase:@"move" x:width / 2 - 36 y:120];
      break;
    case 80:
      check(input.buttons() == S::Select);
      [overlay probeContact:103 phase:@"end" x:0 y:0];
      [overlay probeContact:104 phase:@"begin" x:20 y:20];
      break;
    case 90:
      check(input.buttons() == (S::Left | S::Up));
      [overlay cancelTouches];
      [overlay probeContact:104 phase:@"move" x:width - 124 y:94];
      break;
    case 100:
      check(input.buttons() == 0);
      event(SDL_EVENT_WINDOW_FOCUS_LOST);
      break;
    case 110:
      check(paused && input.buttons() == 0);
      [overlay probeContact:105 phase:@"begin" x:width - 124 y:94];
      event(SDL_EVENT_WINDOW_FOCUS_GAINED);
      break;
    case 120:
      check(paused && input.buttons() == 0);
      [overlay probeContact:105 phase:@"move" x:width - 124 y:94];
      break;
    case 130:
      check(paused && input.buttons() == 0);
      [overlay probeContact:106 phase:@"begin" x:width - 124 y:94];
      break;
    case 140:
      check(!paused && input.buttons() == S::B);
      key(true);
      [overlay probeContact:106 phase:@"end" x:0 y:0];
      break;
    case 150:
      check(input.buttons() == S::B);
      key(false);
      [overlay probeContact:107 phase:@"begin" x:width - 124 y:94];
      [overlay probeContact:108 phase:@"begin" x:width - 46 y:50];
      break;
    case 160:
      check(input.buttons() == (S::B | S::A));
      [overlay probeContact:107 phase:@"end" x:0 y:0];
      break;
    case 170:
      check(input.buttons() == S::A);
      [overlay cancelTouches];
      break;
    case 180:
      check(input.buttons() == 0);
      [overlay probeContact:109 phase:@"begin" x:width - 124 y:94];
      break;
    default:
      break;
    }
  }
};
} // namespace clover::apple
