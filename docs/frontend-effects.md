# Launcher picker request/completion contract

This is the S04a/S04b contract for launcher image selection and its L11
prepared-folder extension, not a general platform framework. The broader
[architecture](frontend-architecture.md) and [task queue](frontend-tasks.md)
still apply.

## Identity and ownership

- A tracker belongs to **one launcher lifetime and its private completion
  channel**. IDs are local to that tracker/channel, not globally unique. Never
  reuse a channel with a fresh tracker. Dropping the launcher drops its receiver;
  late callbacks cannot deliver into a newly created launcher.
- Each accepted begin issues a monotonically increasing opaque request ID. Do
  not reuse IDs after cancellation/context invalidation, and do not wrap on
  counter exhaustion: return a typed error and leave state unchanged.
- Only one launcher picker request may be pending. A second begin is rejected,
  not treated as implicit cancellation/replacement of an open native dialog.
- A completion is eligible only for the currently pending ID. Accept it at most
  once, clearing pending state before applying its outcome. Ignore stale,
  duplicate or out-of-order completions without clearing a newer request.
- Explicit cancellation/invalidation stops accepting a result; it does not
  promise the host dialog itself can be closed. The host still owns callbacks
  and resources until they return. Closing the tracker permanently rejects new
  requests and late results. A stale-ID cancellation cannot cancel a newer ID.

Unique IDs provide generation separation within this first caller. Add explicit
profile/session generations when those callers actually need them; do not
pretend this local tracker is sufficient for a shared cross-window event bus.

## Capabilities and typed outcomes

- A host declares image selection available or unsupported. Unsupported begin
  issues no request ID and leaves no pending operation. This is separate from
  a supported picker subsequently being cancelled, denied or failing.
- Completion outcomes distinguish **selected resource**, **cancelled**,
  **permission denied**, and **failed**. Generic resource/error payloads remain
  host-owned types; the shared tracker knows neither file paths nor SDL handles.
- The desktop adapter supplies `PathBuf` only inside native UI messages. A future
  browser/mobile adapter can supply a capability/handle instead; no universal
  stable-path contract is introduced here. SDL cancellation/empty selection maps
  to cancelled; other unclassified SDL errors map to failed, not guessed denial.

## Lifecycle and production integration

- Focus loss/gain and controller connection changes **do not invalidate** a
  picker request: native pickers normally take focus. They only suppress/reseed
  navigation input using existing controller behavior. Request identity and
  input neutrality are distinct concerns.
- Closing the launcher rejects outstanding results immediately. Any future
  profile/library context replacement must invalidate outstanding requests.
  Entering an import through another accepted source invalidates a pending image
  request before work starts. No stale result may start another import.
- Do not let an uncorrelated queued image command overtake an open picker. A
  drag/drop or library-row image queued earlier in the same event batch is
  accepted for import only when no picker is pending. Otherwise ignore it,
  leaving the picker/request owner intact. This prevents invalidating a live
  dialog while retaining a stuck `picker_pending` flag after an import failure.
  An accepted picker completion clears its flag before enqueueing its image.
- Route the image picker through one completion message containing its ID and
  typed outcome. Its callback must not send an uncorrelated `PickerClosed` which
  could clear a newer picker. The L11 extension below applies the same rule to
  prepared folders; importer worker progress remains outside this contract.
- Only an eligible completion may clear the image picker's busy/input flag,
  update status or enqueue a selected image for import. Ignore rejected
  completions before any of those actions. Do not import when closing/busy.
- Preserve current desktop filters, source immutability, input reseeding and
  persistence behavior. A synchronous native picker-open error must retire its
  request before following the existing error-return path.

## Acceptance

Pure fake-host tests cover begin/duplicate begin, cancel/rebegin/out-of-order
results, duplicate completion, context invalidation, shutdown, counter
exhaustion, unsupported capability and every outcome. Focus/controller events
must leave request eligibility intact. Tests use opaque data, not disk access.
Native adapter tests verify SDL cancellation/empty/error mapping and that rejected
completions neither clear a newer pending state nor enqueue imports. Existing
launcher/UI/suspend tests must keep passing. This contract does not prove
native-dialog behavior on mobile/browser or implement import cancellation.

## Prepared-folder extension (L11)

Image and prepared-folder requests must share one tracker/counter within the
launcher channel. A native owner also records the current picker kind; setting
that kind happens only after begin succeeds. Completion must match **both ID
and kind** before consuming the request or changing input/status/source queues.
An old or wrong-kind completion must not clear a newer dialog. Stale cancellation
must not clear a newer kind either. Invalidation/shutdown clear both consistently.

Both callbacks produce exactly one correlated outcome. The host routes an
accepted image to import and an accepted folder to the existing catalog/assets
validation and isolated external-data path. Folder selection is not image import,
and correlation does not authorize new directory access or upload behavior.
No queued image or folder source may overtake a pending dialog. Native focus
changes and controller reseeding retain eligibility as before.

Unsupported prepared-folder selection must leave no pending request and must
not prevent later image/cached-profile selection. This capability contract is
not evidence of a usable tvOS ingestion flow; that still needs S03c.

## App ROM/firmware extension (L12)

The in-menu App owns a separate tracker/private dialog channel from the startup
launcher. Keep its counter alive across profile switches. Native pending context
captures profile data identity, game ID and (for ROM selection) the requested
auto-start flag. The callback supplies an opaque ID and typed outcome, not an
authoritative game/profile identity.

Before changing overrides, firmware paths, status or game execution, require an
eligible ID/kind and the still-current captured profile/game. A reused game ID in
another profile is not the same target. Invalidate on carousel/direct game
selection changes and on profile-switch attempts, including failed attempts, so
changing away and back cannot revive an earlier result. Sorting while preserving
the same game, focus changes and controller reseeding do not change the target.

When a game actually starts using an existing valid ROM, invalidate outstanding
selection before loading the core. Do not invalidate at the beginning of a
missing-ROM prompt: repeated launch attempts must not continually replace an
already-open picker. On actual quit, close requests before draining queued
completions; App destruction also closes the owner. Frame-limited previews are
not automatically a permanent App shutdown.

After eligibility, the native host retains all current regular-file, safe-ID and
system-extension validation, settings persistence and optional game-start logic.
Firmware-folder selection only chooses a directory; it does not prove firmware
is complete. Existing required-file/core validation remains authoritative, and
imported SMP IPL stays separate from selected coprocessor firmware. No download,
new core ownership or new save-state compatibility is implied.

ROM and firmware adapters share a single App request owner once L12b is wired;
late/wrong-kind results cannot consume the other kind's pending request. A
synchronous dialog-open failure must use the same correlated failure path, not
an unscoped error message which could affect a newer profile. Cancel and failure
remain distinct. App image import is still a separate legacy path until its own
job/service extraction.
