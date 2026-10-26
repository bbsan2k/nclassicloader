#include "../touch_controls.h"
#include "../input_sdl.h"
#include <limits>
#include <stdexcept>

using S = clover::apple::InputState;
using T = clover::apple::TouchControls;
static void check(bool value) {
  if (!value)
    throw std::runtime_error("touch model check failed");
}
int main() {
  check(T::supported(500, 148));
  check(!T::supported(499, 150) && !T::supported(640, 147));
  check(!T::supported(std::numeric_limits<float>::infinity(), 150));
  for (float width : {500.f, 640.f, 1000.f}) {
    check(T::hit(width, 150, 20, 20) == (S::Up | S::Left));
    check(T::hit(width, 150, 110, 115) == (S::Down | S::Right));
    check(T::hit(width, 150, 66, 70) == 0 && T::captures(width, 150, 66, 70));
    check(T::hit(width, 150, width - 46, 50) == S::A);
    check(T::hit(width, 150, width - 124, 94) == S::B);
    check(T::hit(width, 150, width / 2 - 36, 120) == S::Select);
    check(T::hit(width, 150, width / 2 + 36, 120) == S::Start);
    check(!T::captures(width, 150, width / 2, 30));
  }
  check(T::hit(499, 150, 20, 20) == 0);
  check(T::hit(640, 150, std::numeric_limits<float>::quiet_NaN(), 20) == 0);
  check(T::hit(640, 150, -1, 20) == 0 && T::hit(640, 150, 640, 20) == 0);
  S state;
  state.touch(1, 7, true);
  state.touchMask(1, 7, S::A, true, true);
  check(state.buttons() == (S::A | S::B));
  state.touchMask(1, 7, 0, false, true);
  check(state.buttons() == S::B); // UIKit and SDL identities never alias.
  state.touch(1, 7, false);
  state.touchMask(2, 1, S::Up | S::A, true, true);
  state.touchMask(2, 2, S::A, true, true);
  state.touchMask(2, 1, 0, false, true);
  check(state.buttons() == S::A);
  state.touchMask(2, 2, S::Start, true, true);
  check(state.buttons() == S::Start);
  state.clear();
  check(state.buttons() == 0);
  for (unsigned n = 1; n <= 10; ++n)
    state.touchMask(3, n, S::B, true, true);
  state.touchMask(3, 11, S::A, true, true); // Full contact table rejects overflow.
  for (unsigned n = 1; n <= 10; ++n)
    state.touchMask(3, n, 0, false, true);
  check(state.buttons() == 0);
  state.key(S::Right, true);
  state.touchMask(3, 1, S::Left | S::Up, true, true);
  check(state.buttons() == S::Up); // Opposites neutralize across source kinds.
  state.touchMask(3, 1, 0, false, true);
  check(state.buttons() == S::Right);
  clover::apple::Input input;
  check(input.overlayTouch(1, 1, S::B, true));
  input.block(4, true);
  check(input.buttons() == 0);
  check(!input.overlayTouch(1, 2, S::A, true));
  check(input.overlayTouch(1, 1, 0, false));
  input.block(4, false);
  check(input.buttons() == 0);
  check(input.overlayTouch(1, 3, S::Select, true));
  check(input.buttons() == S::Select);
}
