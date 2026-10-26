#pragma once
#include "input_state.h"
#include <cmath>

namespace clover::apple {
// Local UIKit points inside a 150-point, safe-area-pinned control band. Pure
// geometry is also compiled/tested on tvOS; it does not claim a tvOS touch UI.
struct TouchControls {
  static bool supported(float width, float height) {
    return std::isfinite(width) && std::isfinite(height) && width >= 500 && height >= 148;
  }
  static bool rectangle(float x, float y, float left, float top, float width, float height) {
    return x >= left && y >= top && x < left + width && y < top + height;
  }
  static bool circle(float x, float y, float cx, float cy) {
    return (x - cx) * (x - cx) + (y - cy) * (y - cy) <= 30 * 30;
  }
  static bool captures(float width, float height, float x, float y) {
    return supported(width, height) && std::isfinite(x) && std::isfinite(y) &&
           (rectangle(x, y, 0, 4, 132, 132) || hit(width, height, x, y) != 0);
  }
  static uint16_t hit(float width, float height, float x, float y) {
    if (!supported(width, height) || !std::isfinite(x) || !std::isfinite(y) || x < 0 || y < 0 ||
        x >= width || y >= height)
      return 0;
    using S = InputState;
    if (rectangle(x, y, 0, 4, 132, 132)) {
      uint16_t mask = 0;
      if (x < 44)
        mask |= S::Left;
      else if (x >= 88)
        mask |= S::Right;
      if (y < 48)
        mask |= S::Up;
      else if (y >= 92)
        mask |= S::Down;
      return mask;
    }
    if (circle(x, y, width - 46, 50))
      return S::A;
    if (circle(x, y, width - 124, 94))
      return S::B;
    if (rectangle(x, y, width / 2 - 64, 100, 56, 44))
      return S::Select;
    if (rectangle(x, y, width / 2 + 8, 100, 56, 44))
      return S::Start;
    return 0;
  }
};
} // namespace clover::apple
