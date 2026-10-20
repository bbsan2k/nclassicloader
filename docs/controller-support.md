# Rust launcher controller support

`rust/src/ui/launcher/controller.rs` is wired into the Rust startup launcher.
It owns one SDL gamepad plus
its subsystem, discovers an already-connected pad on startup, handles hotplug,
and tries another already-connected pad after removal. The active joystick ID is
cached: querying an unplugged handle's ID is not reliable.

## Mapping

| Control | Action |
| --- | --- |
| D-pad up/down or left stick Y | Previous/next visible library entry; focus library |
| D-pad left/right or left stick X | Cycle focus: library, launch, import, folder |
| Left/right shoulder | Previous/next system filter (single edge, no repeat) |
| South / Start | Activate focused button, or launch from library |
| East / Back | Cancel/close launcher (host decides busy/modal policy) |
| West | Import image (launcher action 1) |
| North | Open prepared folder (launcher action 2) |

SDL names are **positions**, not Nintendo printed letters. For Xbox-style labels,
South=A, East=B, West=X, North=Y. Nintendo controllers may print different letters.
Prefer positional names or SDL's device-specific button-label API for UI hints.

Stick engage threshold is 16000; release threshold is 9000 (SDL signed 16-bit
range). Navigation repeats after 350 ms, then every 100 ms. Each axis emits at
most one repeat per call; stalled frames do not replay a backlog. D-pad overrides
the same stick axis; opposite D-pad directions cancel. Both stick axes can emit
on a diagonal. Confirm/cancel/shortcuts/shoulders never repeat. Aliases such as
South+Start count as one logical held button. On connect or suspension, held
buttons need release/repress and navigation needs neutral before moving.

## Launcher integration

The launcher forwards every SDL event to the helper, including while importing,
unfocused, or waiting for a native file picker. A per-frame tick handles bounded
repeat and missed releases. Input is disabled while busy, unfocused, closing, or
inside a native picker; transitions reseed held-button state to prevent stale
confirm presses. Picker completion includes cancellation and errors.

Controller navigation uses the filtered entry count and the same selection/
filter helpers as keyboard and mouse. Filter changes persist in
`launcher.json`, separate from game settings and saves. The launcher borrows
profile entries rather than cloning large integrity manifests on every frame.

The original integration recipe below is retained as implementation reference:

1. `mod controller;` is declared beside `mod view;` in `launcher.rs`.
2. After `sdl3::init`, before polling events, initialize:

   ```rust,ignore
   let mut controller = controller::Controller::new(
       context.gamepad().map_err(|e| e.to_string())?
   );
   ```

   Alternatively degrade gracefully with `Option<Controller>` on subsystem failure
   so keyboard/mouse startup still works. Keep the helper alive until selection
   returns; drop it before initializing the main App's controller owner.

3. Track window focus and whether a native file/folder picker is pending. For
   **every** event, call `controller.event(&event, started.elapsed(), enabled)`
   before moving `event` into the existing match. `enabled` must be
   `!busy && focused && !picker_pending && !closing`. Do not put event forwarding
   behind `if !busy`: releases and disconnects must always be consumed.
4. After draining `events.poll_iter()`, call
   `controller.tick(started.elapsed(), enabled)` **once per frame**, even when
   disabled, and dispatch its returned actions too. This handles held navigation
   without relying on additional SDL axis events.
5. Dispatch returned actions in order:
   - Navigation: `a.navigate(&mut selected, visible_count, &mut focus)`. If the
     filter agent uses a separate visible cursor, pass that cursor, not a raw
     index into unfiltered `entries`. Existing draw/action code must translate it.
   - FilterPrevious/FilterNext: call the same filter-cycle path as keyboard tabs;
     clamp/reset the visible selection and clear `focus` exactly as that path does.
   - Cancel: set `closing = true` according to existing Escape behavior.
   - Confirm/Import/OpenFolder: use `a.activation(focus)` to obtain existing
     launcher indices (0/1/2). Prefer the first activation within a frame; do not
     enqueue extra activations. Retain the existing empty-library fallback from
     launch to import.
6. Call `controller.suspend()` on window focus transitions and immediately before
   opening a native picker. Set `picker_pending` until its asynchronous callback
   reports completion **including cancellation/errors**. Keep forwarding events
   and ticking with `enabled=false` while it is pending. Consume the callback on
   the SDL thread, then suspend again before reenabling input. On busy transitions
   likewise suspend or ensure a disabled tick consumes the current held state.

The helper returns semantic press/repeat actions, not synthetic SDL events. It
tracks both button down and up internally, preserving quick down/up pairs in one
frame. The host should not synthesize release actions. `connected()` can control
launcher-specific hints; it does not establish hardware compatibility.

## Verification without hardware

The file's seven pure tests compile independently of the application and SDL:

```sh
rustc --edition=2024 --test rust/src/ui/launcher/controller.rs -o /path/to/controller-tests
/path/to/controller-tests
```

Tests cover shortcut/positional mappings, edge-only activations, confirm aliases,
repeat delay and stalled-frame bounds, hysteresis and i16 extremes, connection /
busy-state suppression, opposite-direction cancellation, focus cycling and empty
libraries. The SDL adapter was separately type-checked against pinned sdl3 0.19.0
and sdl3-sys 0.7.1 with `no-link`, without changing shared application files.

No physical-controller verification has been performed. Hardware-free tests and
launcher snapshots validate logic/layout, not device compatibility. A host `--exercise` mode can
initialize SDL plus this helper, forward events, log returned `Action` values,
and exit on Quit/Cancel; no console programs or copyrighted assets are needed.
Do not treat such a mode as implemented here.

## Imported menu and gameplay

The main App supports D-pad and left-stick navigation, hysteresis/deadzone,
bounded directional repeats, controller hotplug/fallback and the existing
NES/SNES gameplay button mask. The active joystick ID is cached for reliable
disconnect cleanup. New connections and menu transitions require neutral input
before the pad can activate anything. Unplugging the active controller during
gameplay pauses it and captures a pending suspend point.

Guide or **Back + Start** pauses gameplay; Escape remains the keyboard fallback.
In menus, South confirms and East backs out. Gameplay preserves the standard
SNES-layout mask (South=B, East=A, West=Y, North=X, shoulders=L/R), with NES using
the A/B subset. One primary controller is supported, not multiplayer input.

**F11** toggles fullscreen in both windows, including during gameplay. On macOS,
View → Toggle Full Screen / Control–Command–F also works. These do not pause a
running game. The existing emulator settings fullscreen action remains.

## Limitations

- Only SDL-mapped gamepads are supported; raw joystick-only devices require an SDL
  mapping. One active controller is intentional; there is no controller chooser.
- Native file/folder dialogs can be opened from the pad, but their OS UI is not
  guaranteed navigable with a gamepad. A fully controller-only import experience
  needs an in-app picker or platform accessibility support.
- Hardware mapping, actual hotplug ordering, focus transitions, and native picker
  behavior still need manual testing on each supported platform.
- Device-specific printed button labels can differ from SDL's positional names.
  No controller remapping UI, multiplayer input, or rumble support is added here.
