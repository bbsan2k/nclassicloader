#pragma once
#include <array>
#include <cstdint>

namespace clover::apple {
// SNES-mask ABI shared by the NES bridge. Simultaneous opposite directions are
// neutralized after merging sources; a release affects only its own source.
struct InputState {
  static constexpr uint16_t B = 1, Y = 2, Select = 4, Start = 8, Up = 16, Down = 32, Left = 64,
                            Right = 128, A = 256, X = 512, L = 1024, R = 2048;
  struct Pad {
    uint32_t id = 0;
    uint16_t buttons = 0;
    int x = 0, y = 0;
  };
  struct Finger {
    bool active = false;
    uint64_t device = 0, id = 0;
    uint16_t mask = 0;
    bool overlay = false;
  };
  std::array<Pad, 8> pads{};
  std::array<Finger, 10> fingers{};
  uint16_t keyboard = 0;
  bool connected(uint32_t id) const {
    for (const auto &pad : pads)
      if (id && pad.id == id)
        return true;
    return false;
  }

  bool connect(uint32_t id) {
    if (!id)
      return false;
    for (auto &pad : pads)
      if (pad.id == id)
        return true;
    for (auto &pad : pads)
      if (!pad.id) {
        pad.id = id;
        return true;
      }
    return false;
  }
  void disconnect(uint32_t id) {
    for (auto &pad : pads)
      if (pad.id == id)
        pad = {};
  }
  void remap(uint32_t id) {
    for (auto &pad : pads)
      if (pad.id == id)
        pad = {id, 0, 0, 0};
  }
  void key(uint16_t mask, bool down) {
    keyboard = down ? keyboard | mask : keyboard & ~mask;
  }
  void button(uint32_t id, uint16_t mask, bool down) {
    for (auto &pad : pads)
      if (pad.id == id)
        pad.buttons = down ? pad.buttons | mask : pad.buttons & ~mask;
  }
  static int direction(int previous, int value) {
    if (value >= 16000)
      return 1;
    if (value <= -16000)
      return -1;
    if (previous == 1 && value > 12000)
      return 1;
    if (previous == -1 && value < -12000)
      return -1;
    return 0;
  }
  void axis(uint32_t id, bool horizontal, int value) {
    for (auto &pad : pads)
      if (pad.id == id) {
        auto &axis = horizontal ? pad.x : pad.y;
        axis = direction(axis, value);
      }
  }
  // Preserve the existing single B-touch behavior, but track each finger/device
  // independently. Full on-screen control geometry is a separate presentation task.
  void touch(uint64_t device, uint64_t id, bool down) {
    touchMask(device, id, B, down, false);
  }
  void touchMask(uint64_t device, uint64_t id, uint16_t mask, bool down, bool overlay) {
    for (auto &finger : fingers)
      if (finger.active && finger.device == device && finger.id == id &&
          finger.overlay == overlay) {
        if (!down)
          finger = {};
        else
          finger.mask = mask;
        return;
      }
    if (down)
      for (auto &finger : fingers)
        if (!finger.active) {
          finger = {true, device, id, mask, overlay};
          return;
        }
  }
  void clear() {
    keyboard = 0;
    fingers = {};
    for (auto &pad : pads) {
      pad.buttons = 0;
      pad.x = pad.y = 0;
    }
  }
  uint16_t buttons() const {
    uint16_t mask = keyboard;
    for (const auto &pad : pads)
      if (pad.id) {
        mask |= pad.buttons;
        mask |= pad.x < 0 ? Left : pad.x > 0 ? Right : 0;
        mask |= pad.y < 0 ? Up : pad.y > 0 ? Down : 0;
      }
    for (const auto &finger : fingers)
      if (finger.active)
        mask |= finger.mask;
    if ((mask & (Up | Down)) == (Up | Down))
      mask &= ~(Up | Down);
    if ((mask & (Left | Right)) == (Left | Right))
      mask &= ~(Left | Right);
    return mask;
  }
};
} // namespace clover::apple
