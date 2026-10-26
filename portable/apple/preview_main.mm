#import <Foundation/Foundation.h>
#import <AVFAudio/AVFAudio.h>
#define SDL_MAIN_USE_CALLBACKS
#include <SDL3/SDL.h>
#include <SDL3/SDL_main.h>
#include "preview_bridge.h"
#include "input_sdl.h"
#include "input_probe.h"
#include "touch_probe.h"
#import "profile_launcher.h"
#import "document_import.h"
#include <TargetConditionals.h>
#include <mutex>
#include <array>
#include <atomic>
#include "../core/tests/nes_fixture.h"
#include <fstream>
#include <stdexcept>

static bool reported;
static std::atomic<bool> gameMode{false};
static bool gameProbe;
static bool touchProbeMode;
static CloverTouchOverlay *touchControls;
static CloverProfileLauncher *profileLauncher;
static bool profileMode;
static std::atomic<bool> classicActive{false};
static std::atomic<uint64_t> classicRevision{0};
static bool transferProbeMode;
static uint64_t transferSequence;
static clover::apple::TouchProbe touchProbe;
static bool alphaMode;
static bool importProbeMode;
static bool libraryProbeMode;
static std::atomic<bool> libraryInputActive{false};
static std::atomic<uint64_t> appliedLibraryRevision{0};
struct LibraryPoint {
  uint64_t revision;
  uint32_t window;
  float x, y;
  bool mouse;
};
static std::mutex libraryPointMutex;
static std::array<LibraryPoint, 16> libraryPoints;
static unsigned libraryPointCount;
struct CollectionKey {
  uint64_t revision;
  int direction;
};
static std::mutex collectionKeyMutex;
static std::array<CollectionKey, 32> collectionKeys;
static unsigned collectionKeyCount;
static unsigned importSequence;
static CloverDocumentImport *documents;
static NSURL *importSource;
static NSURL *importInvalid;
static NSString *importedROM;
static bool importProbeSavePersisted() {
  if (!importedROM)
    return false;
  NSURL *root = [NSURL fileURLWithPath:[[importedROM stringByDeletingLastPathComponent]
                                           stringByAppendingPathComponent:@"saves"]
                           isDirectory:YES];
  NSDirectoryEnumerator *entries =
      [NSFileManager.defaultManager enumeratorAtURL:root
                         includingPropertiesForKeys:@[
                           NSURLIsSymbolicLinkKey, NSURLIsRegularFileKey, NSURLFileSizeKey
                         ]
                                            options:NSDirectoryEnumerationSkipsHiddenFiles
                                       errorHandler:nil];
  unsigned count = 0;
  for (NSURL *url in entries) {
    if (++count > 64)
      break;
    if (![url.lastPathComponent isEqual:@"save.ram"])
      continue;
    NSDictionary *values = [url
        resourceValuesForKeys:@[ NSURLIsSymbolicLinkKey, NSURLIsRegularFileKey, NSURLFileSizeKey ]
                        error:nil];
    if ([values[NSURLIsSymbolicLinkKey] boolValue] || ![values[NSURLIsRegularFileKey] boolValue] ||
        [values[NSURLFileSizeKey] unsignedLongLongValue] != 8192)
      continue;
    NSData *data = [NSData dataWithContentsOfURL:url];
    if (data.length == 8192) {
      const uint8_t *bytes = (const uint8_t *)data.bytes;
      return bytes[0] == 0 && bytes[1] == 1;
    }
  }
  return false;
}
static bool inputProbeMode;
static clover::apple::InputProbe inputProbe;
static clover::apple::Input input;
static std::atomic<bool> focused{true};
static unsigned callbacks;
static uint64_t pausedFrames;
static bool lifecyclePassed;
static std::atomic<bool> backgrounded{false};
static std::atomic<uint32_t> pendingEvents{0};
// Main-thread-only ownership gate. Immediate SDL lifecycle delivery can occur
// outside a display callback, or reentrantly while Rust calls SDL.
static void *ownerState;
static bool inRust;
static bool hostFailed;
static bool externalLifecycle;
static bool externalInterruption;
static id interruptionObserver;
static std::atomic<bool> audioInterrupted{false};
static unsigned interruptionBegan;
static unsigned interruptionEnded;
static NSString *instanceID;
static unsigned backgroundEvents;
static unsigned foregroundEvents;
static unsigned commandSequence;
static uint64_t resumeDelta;
static bool servicePendingEvents();

struct RustBoundary {
  RustBoundary() {
    inRust = true;
  }
  ~RustBoundary() {
    inRust = false;
    servicePendingEvents();
  }
};

static NSDictionary *gameReport(void *state) {
  char json[2048] = {}, error[2048] = {};
  if (clover_apple_game_report(state, json, sizeof(json), error, sizeof(error)))
    return nil;
  NSData *data = [[NSString stringWithUTF8String:json] dataUsingEncoding:NSUTF8StringEncoding];
  return [NSJSONSerialization JSONObjectWithData:data options:0 error:nil];
}

static void lifecycleSnapshot(NSString *phase, void *state) {
  if (!externalLifecycle)
    return;
  @autoreleasepool {
    NSDictionary *game = gameReport(state);
    if (!game) {
      hostFailed = true;
      return;
    }
    NSDictionary *snapshot = @{
      @"schema" : @1,
      @"platform" : @CLOVER_APPLE_PLATFORM,
      @"run_id" : NSProcessInfo.processInfo.environment[@"CLOVER_SMOKE_RUN_ID"] ?: @"",
      @"instance_id" : instanceID,
      @"phase" : phase,
      @"callbacks" : @(callbacks),
      @"background_events" : @(backgroundEvents),
      @"foreground_events" : @(foregroundEvents),
      @"command_sequence" : @(commandSequence),
      @"resume_frame_delta" : @(resumeDelta),
      @"game" : game
    };
    NSMutableDictionary *details = [snapshot mutableCopy];
    details[@"interruption_began"] = @(interruptionBegan);
    details[@"interruption_ended"] = @(interruptionEnded);
    NSData *data = [NSJSONSerialization dataWithJSONObject:details options:0 error:nil];
    NSString *cache =
        NSSearchPathForDirectoriesInDomains(NSCachesDirectory, NSUserDomainMask, YES).firstObject;
    NSString *path =
        [cache stringByAppendingPathComponent:[NSString stringWithFormat:@"clover-%@-%@.json",
                                                                         externalInterruption
                                                                             ? @"interruption"
                                                                             : @"lifecycle",
                                                                         phase]];
    if (!data || ![data writeToFile:path options:NSDataWritingAtomic error:nil])
      hostFailed = true;
  }
}

static bool servicePendingEvents() {
  if (!NSThread.isMainThread)
    return false;
  if (!ownerState || inRust)
    return !hostFailed;
  [touchControls setAccepting:!backgrounded.load() && !audioInterrupted.load() && focused.load()];
  // No renderer work here. Pause/clear must happen before UIKit can suspend the
  // display link. Reentrant/off-thread delivery only queues flags for this gate.
  inRust = true;
  const uint32_t events = pendingEvents.exchange(0);
  // A loss and recovery may coalesce before this main-thread gate runs. Clear
  // UIKit identities from the loss flags, not only the latest accepting state,
  // so a later move cannot resurrect a contact cleared by the input blocker.
  if (events & (1 | 8 | 64 | 128))
    [touchControls cancelTouches];
  if ((events & 4096) && !backgrounded.load())
    [profileLauncher returnToLauncher];
  if (events & 65536) {
    std::array<CollectionKey, 32> keys;
    unsigned count;
    {
      std::lock_guard lock(collectionKeyMutex);
      keys = collectionKeys;
      count = collectionKeyCount;
      collectionKeyCount = 0;
    }
    if (!(events & (1 | 8 | 64 | 128)) && !backgrounded.load() && !audioInterrupted.load() &&
        focused.load()) {
      for (unsigned index = 0; index < count; index++) {
        if (keys[index].revision != classicRevision.load())
          continue;
        if (keys[index].direction)
          [profileLauncher moveCollectionFocus:keys[index].direction];
        else
          [profileLauncher activateFocusedCollection];
      }
    }
  }
  if (classicActive.load() && !gameMode && !backgrounded.load() && !audioInterrupted.load() &&
      focused.load()) {
    if (events & 8192)
      hostFailed |= clover_apple_classic_input(ownerState, input.buttons(), 1) != 0;
    if (events & 16384)
      hostFailed |= clover_apple_classic_input(ownerState, input.buttons(), 2) != 0;
    if (events & 32768) {
      std::array<LibraryPoint, 16> points;
      unsigned count;
      {
        std::lock_guard lock(libraryPointMutex);
        points = libraryPoints;
        count = libraryPointCount;
        libraryPointCount = 0;
      }
      for (unsigned n = 0; n < count; ++n) {
        auto p = points[n];
        if (p.revision != classicRevision.load())
          continue;
        if (p.mouse) {
          int w = 0, h = 0;
          SDL_GetWindowSize(SDL_GetWindowFromID(p.window), &w, &h);
          if (w <= 0 || h <= 0)
            continue;
          p.x /= w;
          p.y /= h;
        }
        hostFailed |= clover_apple_classic_point(ownerState, p.window, p.x, p.y) != 0;
      }
    }
  } else if (events & 32768) {
    std::lock_guard lock(libraryPointMutex);
    libraryPointCount = 0;
  }
  if (events & 1)
    [profileLauncher setForeground:NO];
  if ((events & 2) && !backgrounded.load())
    [profileLauncher setForeground:YES];
  if (events & 32)
    input.rescan();
  if (events & 64)
    hostFailed |= clover_apple_preview_event(ownerState, 1) != 0;
  if (events & 128)
    hostFailed |= clover_apple_preview_event(ownerState, 1) != 0;
  if (events & 8) {
    hostFailed |= clover_apple_preview_event(ownerState, 4) != 0;
    ++interruptionBegan;
    lifecycleSnapshot(@"began", ownerState);
  }
  if ((events & 16) || ((events & 8) && !audioInterrupted.load())) {
    // The latest atomic state resolves begin/end notifications coalesced while
    // Rust was borrowed. Never implicitly resume, even with ShouldResume set.
    if (!audioInterrupted.load())
      hostFailed |= clover_apple_preview_event(ownerState, 5) != 0;
    if (events & 16)
      ++interruptionEnded;
    lifecycleSnapshot(@"ended", ownerState);
  }
  if (events & 1) {
    hostFailed |= clover_apple_preview_event(ownerState, 1) != 0;
    ++backgroundEvents;
    lifecycleSnapshot(@"background", ownerState);
  }
  if (events & 2) {
    hostFailed |= clover_apple_preview_event(ownerState, 2) != 0;
    ++foregroundEvents;
    lifecycleSnapshot(@"foreground", ownerState);
  }
  if (libraryInputActive.load() && !backgrounded.load() && !audioInterrupted.load() &&
      focused.load()) {
    if (events & 2048) {
      std::array<LibraryPoint, 16> points;
      unsigned count;
      {
        std::lock_guard lock(libraryPointMutex);
        points = libraryPoints;
        count = libraryPointCount;
        libraryPointCount = 0;
      }
      for (unsigned n = 0; n < count; ++n) {
        auto point = points[n];
        if (point.mouse) {
          int width = 0, height = 0;
          SDL_GetWindowSize(SDL_GetWindowFromID(point.window), &width, &height);
          if (width <= 0 || height <= 0)
            continue;
          point.x /= width;
          point.y /= height;
        }
        char digest[65] = {}, error[256] = {};
        if (!clover_apple_library_pick(ownerState, point.revision, point.window, point.x, point.y,
                                       digest, sizeof(digest), error, sizeof(error)) &&
            digest[0])
          [documents selectDigest:[NSString stringWithUTF8String:digest]];
      }
    }
    if (events & 256)
      [documents moveSelection:-1];
    if (events & 512)
      [documents moveSelection:1];
    if (events & 1024)
      [documents openSelection];
  } else if (events & 2048) {
    std::lock_guard lock(libraryPointMutex);
    libraryPointCount = 0;
  }
  if (!(events & (1 | 8 | 16 | 64 | 128)) && (events & 4) && !backgrounded.load() &&
      !audioInterrupted.load() && focused.load())
    hostFailed |= clover_apple_preview_event(ownerState, 3) != 0;
  inRust = false;
  return !hostFailed;
}

static void queueInterruption(bool began) {
  audioInterrupted.store(began);
  input.block(2, began);
  pendingEvents.fetch_or(began ? 8u : 16u);
  if (NSThread.isMainThread)
    servicePendingEvents();
  else
    dispatch_async(dispatch_get_main_queue(), ^{
      servicePendingEvents();
    });
}

// Sandbox command channel exists only for an explicitly enabled simulator probe.
// It injects explicit resume, never background/foreground lifecycle events.
static bool lifecycleResumeCommand() {
  if (!externalLifecycle || backgrounded.load() || (!externalInterruption && !foregroundEvents))
    return false;
  NSString *cache =
      NSSearchPathForDirectoriesInDomains(NSCachesDirectory, NSUserDomainMask, YES).firstObject;
  NSString *path = [cache stringByAppendingPathComponent:externalInterruption
                                                             ? @"clover-interruption-command.json"
                                                             : @"clover-lifecycle-command.json"];
  NSDictionary *attributes = [NSFileManager.defaultManager attributesOfItemAtPath:path error:nil];
  if (!attributes || [attributes fileSize] > 1024)
    return false;
  NSData *data = [NSData dataWithContentsOfFile:path];
  if (!data)
    return false;
  id command = [NSJSONSerialization JSONObjectWithData:data options:0 error:nil];
  if (![command isKindOfClass:NSDictionary.class] ||
      ![command[@"run_id"] isEqual:NSProcessInfo.processInfo.environment[@"CLOVER_SMOKE_RUN_ID"]] ||
      ![command[@"sequence"] isEqual:@(commandSequence + 1)])
    return false;
  NSString *action = command[@"action"];
  if (![action isEqual:@"resume"] &&
      !(externalInterruption && ([action isEqual:@"begin"] || [action isEqual:@"end"])))
    return false;
  ++commandSequence;
  if ([action isEqual:@"resume"])
    return true;
  const bool began = [action isEqual:@"begin"];
  // Probe-only notification injection exercises our observer and SDL's existing
  // observer. This is not a phone call/Siri or OS-issued interruption.
  [NSNotificationCenter.defaultCenter
      postNotificationName:AVAudioSessionInterruptionNotification
                    object:AVAudioSession.sharedInstance
                  userInfo:@{
                    AVAudioSessionInterruptionTypeKey :
                        @(began ? AVAudioSessionInterruptionTypeBegan
                                : AVAudioSessionInterruptionTypeEnded),
                    AVAudioSessionInterruptionOptionKey :
                        @(AVAudioSessionInterruptionOptionShouldResume)
                  }];
  return false;
}

static void record(bool passed, uint32_t frames, uint64_t checksum, const char *message,
                   void *state = nullptr) {
  @autoreleasepool {
    NSString *cache =
        NSSearchPathForDirectoriesInDomains(NSCachesDirectory, NSUserDomainMask, YES).firstObject;
    NSString *runID = NSProcessInfo.processInfo.environment[@"CLOVER_SMOKE_RUN_ID"] ?: @"manual";
    NSMutableDictionary *result = [@{
      @"schema" : @1,
      @"platform" : @CLOVER_APPLE_PLATFORM,
      @"run_id" : runID,
      @"passed" : @(passed),
      @"frames" : @(frames),
      @"checksum" : [NSString stringWithFormat:@"%llu", (unsigned long long)checksum],
      @"error" : [NSString stringWithUTF8String:message] ?: @"Invalid UTF8 diagnostic"
    } mutableCopy];
    if (gameMode) {
      NSDictionary *game = state ? gameReport(state) : nil;
      result[@"game"] = game ?: @{};
      result[@"lifecycle_probe_passed"] = @(lifecyclePassed);
      if (inputProbeMode) {
        result[@"input_probe_passed"] = @(inputProbe.passed);
        result[@"input_checks"] = @(inputProbe.checks);
      }
      if (touchProbeMode) {
        result[@"touch_probe_passed"] = @(touchProbe.passed);
        result[@"touch_checks"] = @(touchProbe.checks);
        result[@"touch_ui_supported"] = @(touchControls.supported);
      }
      result[@"passed"] =
          @(passed &&
            (inputProbeMode   ? inputProbe.passed && inputProbe.checks == 12
             : touchProbeMode ? touchProbe.passed && touchProbe.checks == 17
                              : lifecyclePassed) &&
            [game[@"emulated_frames"] unsignedLongLongValue] >= 60 &&
            [game[@"video_changes"] unsignedLongLongValue] >= 2 &&
            [game[@"audio_nonzero"] boolValue] && [game[@"audio_started"] boolValue] &&
            [game[@"audio_samples"] unsignedLongLongValue] > 0 &&
            [game[@"audio_drained_bytes"] unsignedLongLongValue] > 0 &&
            [game[@"max_audio_queue_bytes"] unsignedLongLongValue] <=
                [game[@"audio_limit_bytes"] unsignedLongLongValue] &&
            [game[@"max_steps_per_callback"] unsignedIntValue] <= 3);
    }
    NSError *error = nil;
    [NSFileManager.defaultManager createDirectoryAtPath:cache
                            withIntermediateDirectories:YES
                                             attributes:nil
                                                  error:&error];
    NSData *data = [NSJSONSerialization dataWithJSONObject:result
                                                   options:NSJSONWritingPrettyPrinted
                                                     error:&error];
    NSString *path =
        [cache stringByAppendingPathComponent:touchProbeMode   ? @"clover-touch-result.json"
                                              : inputProbeMode ? @"clover-input-result.json"
                                              : gameMode       ? @"clover-game-result.json"
                                                               : @"clover-preview-result.json"];
    if (!data || ![data writeToFile:path options:NSDataWritingAtomic error:&error])
      NSLog(@"Clover preview receipt failed: %@", error);
  }
}

SDL_AppResult SDL_AppInit(void **state, int argc, char **argv) {
  (void)argc;
  (void)argv;
  char error[2048] = {};
  if (!NSThread.isMainThread)
    return SDL_APP_FAILURE;
  RustBoundary boundary;
  instanceID = NSUUID.UUID.UUIDString;
  gameMode = [NSProcessInfo.processInfo.environment[@"CLOVER_APPLE_GAME"] isEqualToString:@"1"];
  touchProbeMode =
      [NSProcessInfo.processInfo.environment[@"CLOVER_APPLE_TOUCH"] isEqualToString:@"1"] &&
      NSProcessInfo.processInfo.environment[@"CLOVER_SMOKE_RUN_ID"] != nil;
  importProbeMode =
      [NSProcessInfo.processInfo.environment[@"CLOVER_APPLE_IMPORT"] isEqualToString:@"1"] &&
      NSProcessInfo.processInfo.environment[@"CLOVER_SMOKE_RUN_ID"] != nil;
  libraryProbeMode =
      [NSProcessInfo.processInfo.environment[@"CLOVER_APPLE_LIBRARY"] isEqualToString:@"1"] &&
      NSProcessInfo.processInfo.environment[@"CLOVER_SMOKE_RUN_ID"] != nil;
  importProbeMode = importProbeMode || libraryProbeMode;
  alphaMode = importProbeMode;
  transferProbeMode =
      [NSProcessInfo.processInfo.environment[@"CLOVER_APPLE_TRANSFER"] isEqualToString:@"1"] &&
      NSProcessInfo.processInfo.environment[@"CLOVER_SMOKE_RUN_ID"] != nil;
  profileMode = transferProbeMode ||
                (!gameMode && NSProcessInfo.processInfo.environment[@"CLOVER_SMOKE_RUN_ID"] == nil);
  if (profileMode)
    SDL_SetHint(SDL_HINT_MAIN_CALLBACK_RATE, "60");
#if TARGET_OS_TV
  if (profileMode) {
    SDL_SetHint(SDL_HINT_APPLE_TV_CONTROLLER_UI_EVENTS, "1");
    SDL_SetHint(SDL_HINT_TV_REMOTE_AS_JOYSTICK, "0");
  }
#endif
  inputProbeMode =
      gameMode &&
      [NSProcessInfo.processInfo.environment[@"CLOVER_APPLE_INPUT"] isEqualToString:@"1"] &&
      NSProcessInfo.processInfo.environment[@"CLOVER_SMOKE_RUN_ID"] != nil;
  externalInterruption =
      gameMode &&
      [NSProcessInfo.processInfo.environment[@"CLOVER_APPLE_INTERRUPTION"] isEqualToString:@"1"] &&
      NSProcessInfo.processInfo.environment[@"CLOVER_SMOKE_RUN_ID"] != nil;
  externalLifecycle =
      externalInterruption ||
      (gameMode &&
       [NSProcessInfo.processInfo.environment[@"CLOVER_APPLE_LIFECYCLE"] isEqualToString:@"1"] &&
       NSProcessInfo.processInfo.environment[@"CLOVER_SMOKE_RUN_ID"] != nil);
  gameProbe = gameMode && !externalLifecycle && !inputProbeMode && !touchProbeMode &&
              NSProcessInfo.processInfo.environment[@"CLOVER_SMOKE_RUN_ID"] != nil;
  if (clover_apple_preview_init(state, error, sizeof(error))) {
    record(false, 0, 0, error);
    return SDL_APP_FAILURE;
  }
  if (gameMode || alphaMode || profileMode) {
    if (!input.start()) {
      record(false, 0, 0, "Cannot initialize SDL gamepads");
      return SDL_APP_FAILURE;
    }
    interruptionObserver = [NSNotificationCenter.defaultCenter
        addObserverForName:AVAudioSessionInterruptionNotification
                    object:AVAudioSession.sharedInstance
                     queue:nil
                usingBlock:^(NSNotification *notification) {
                  NSNumber *type = notification.userInfo[AVAudioSessionInterruptionTypeKey];
                  if ([type isEqual:@(AVAudioSessionInterruptionTypeBegan)])
                    queueInterruption(true);
                  else if ([type isEqual:@(AVAudioSessionInterruptionTypeEnded)])
                    queueInterruption(false);
                }];
  }
  if (gameMode) {
    @autoreleasepool {
      NSString *cache =
          NSSearchPathForDirectoriesInDomains(NSCachesDirectory, NSUserDomainMask, YES).firstObject;
      NSString *root = [cache
          stringByAppendingPathComponent:[@"clover-generated-"
                                             stringByAppendingString:NSUUID.UUID.UUIDString]];
      NSError *failure = nil;
      if (![NSFileManager.defaultManager createDirectoryAtPath:root
                                   withIntermediateDirectories:YES
                                                    attributes:nil
                                                         error:&failure]) {
        record(false, 0, 0, "Cannot create generated-game sandbox");
        return SDL_APP_FAILURE;
      }
      NSString *rom = [root stringByAppendingPathComponent:@"generated.nes"];
      // Same self-authored fixture as the core smoke test; no downloaded content.
      try {
        auto bytes = clover::test_fixture::nes_homebrew(0, true);
        std::ofstream file(rom.fileSystemRepresentation, std::ios::binary);
        file.write(reinterpret_cast<const char *>(bytes.data()), bytes.size());
        file.close();
        if (!file)
          throw std::runtime_error("fixture write failed");
      } catch (...) {
        record(false, 0, 0, "Cannot write generated NES fixture");
        return SDL_APP_FAILURE;
      }
      if (clover_apple_game_start(*state, rom.fileSystemRepresentation,
                                  root.fileSystemRepresentation, error, sizeof(error))) {
        record(false, 0, 0, error);
        return SDL_APP_FAILURE;
      }
    }
  }
  if (inputProbeMode && !inputProbe.start(input)) {
    record(false, 0, 0, "Cannot create virtual SDL gamepad");
    return SDL_APP_FAILURE;
  }
  if (alphaMode) {
    documents = [CloverDocumentImport new];
    int count = 0;
    SDL_Window **windows = SDL_GetWindows(&count);
    [documents install:count ? windows[0] : nullptr
                 pause:^{
                   input.block(8, true);
                   pendingEvents.fetch_or(128);
                   if (NSThread.isMainThread)
                     servicePendingEvents();
                 }];
    SDL_free(windows);
    [documents refreshLibrary];
    if (importProbeMode) {
      NSString *cache =
          NSSearchPathForDirectoriesInDomains(NSCachesDirectory, NSUserDomainMask, YES).firstObject;
      importSource =
          [NSURL fileURLWithPath:[cache stringByAppendingPathComponent:@"import-probe-source.nes"]];
      importInvalid = [NSURL
          fileURLWithPath:[cache stringByAppendingPathComponent:@"import-probe-invalid.nes"]];
      auto bytes = clover::test_fixture::nes_homebrew(0, true);
      if (!libraryProbeMode &&
          (![[NSData dataWithBytes:bytes.data() length:bytes.size()] writeToURL:importSource
                                                                        options:NSDataWritingAtomic
                                                                          error:nil] ||
           ![[NSData dataWithBytes:"bad" length:3] writeToURL:importInvalid
                                                      options:NSDataWritingAtomic
                                                        error:nil]))
        return SDL_APP_FAILURE;
    }
  }
  if (profileMode) {
    profileLauncher = [CloverProfileLauncher new];
    int count = 0;
    SDL_Window **windows = SDL_GetWindows(&count);
    NSString *failure = nil;
    BOOL installed = [profileLauncher install:count ? windows[0] : nullptr error:&failure];
    SDL_free(windows);
    if (!installed) {
      record(false, 0, 0, failure.UTF8String ?: "Cannot create profile launcher");
      return SDL_APP_FAILURE;
    }
  }
  if (alphaMode || touchProbeMode || profileMode) {
    touchControls = [CloverTouchOverlay new];
    int count = 0;
    SDL_Window **windows = SDL_GetWindows(&count);
    [touchControls
        install:count ? windows[0] : nullptr
           emit:^(uint64_t source, uint64_t finger, uint16_t mask, BOOL down, BOOL press) {
             bool accepted = input.overlayTouch(source, finger, mask, down);
             if (accepted && down && press) {
               pendingEvents.fetch_or(4);
               if (NSThread.isMainThread)
                 servicePendingEvents();
               else
                 dispatch_async(dispatch_get_main_queue(), ^{
                   servicePendingEvents();
                 });
             }
           }];
    SDL_free(windows);
  }
  ownerState = *state;
  return SDL_APP_CONTINUE;
}

SDL_AppResult SDL_AppIterate(void *state) {
  char error[2048] = {};
  if (!NSThread.isMainThread || inRust || !servicePendingEvents())
    return SDL_APP_FAILURE;
  if (importProbeMode) {
    NSString *cache =
        NSSearchPathForDirectoriesInDomains(NSCachesDirectory, NSUserDomainMask, YES).firstObject;
    NSString *path = [cache stringByAppendingPathComponent:@"clover-import-command.json"];
    NSDictionary *attributes = [NSFileManager.defaultManager attributesOfItemAtPath:path error:nil];
    if (attributes && [attributes fileSize] <= 1024) {
      NSData *data = [NSData dataWithContentsOfFile:path];
      id command = data ? [NSJSONSerialization JSONObjectWithData:data options:0 error:nil] : nil;
      if ([command isKindOfClass:NSDictionary.class] &&
          [command[@"run_id"]
              isEqual:NSProcessInfo.processInfo.environment[@"CLOVER_SMOKE_RUN_ID"]] &&
          [command[@"sequence"] isEqual:@(importSequence + 1)] && [@[
            @"show", @"cancel", @"invalid", @"select", @"play", @"stop", @"cancel-import", @"seed",
            @"refresh", @"open", @"next", @"previous"
          ] containsObject:command[@"action"]]) {
        ++importSequence;
        NSString *action = command[@"action"];
        if ([action isEqual:@"seed"] && libraryProbeMode) {
          auto bytes = clover::test_fixture::nes_homebrew(0, true);
          if (![[NSData dataWithBytes:bytes.data()
                               length:bytes.size()] writeToURL:importSource
                                                       options:NSDataWritingAtomic
                                                         error:nil])
            return SDL_APP_FAILURE;
          action = @"select";
        }
        [documents probeAction:action
                           url:[action isEqual:@"invalid"] ? importInvalid : importSource];
      }
    }
  }
  if (transferProbeMode) {
    NSString *cache =
        NSSearchPathForDirectoriesInDomains(NSCachesDirectory, NSUserDomainMask, YES).firstObject;
    NSString *path = [cache stringByAppendingPathComponent:@"clover-transfer-command.json"];
    NSDictionary *attrs = [NSFileManager.defaultManager attributesOfItemAtPath:path error:nil];
    if (attrs && [attrs fileSize] <= 2048) {
      NSData *data = [NSData dataWithContentsOfFile:path];
      id command = data ? [NSJSONSerialization JSONObjectWithData:data options:0 error:nil] : nil;
      if ([command isKindOfClass:NSDictionary.class] &&
          [command[@"run_id"]
              isEqual:NSProcessInfo.processInfo.environment[@"CLOVER_SMOKE_RUN_ID"]] &&
          [command[@"sequence"] isEqual:@(transferSequence + 1)] && [@[
            @"open", @"play", @"stop", @"select-file", @"new-code", @"focus-library",
            @"collection-up", @"collection-down", @"collection-burst", @"menu-confirm",
            @"menu-right", @"menu-left", @"menu-back"
          ] containsObject:command[@"action"]]) {
        ++transferSequence;
        [profileLauncher probeAction:command];
      }
    }
  }
  RustBoundary boundary;
  ++callbacks;
  [touchControls setGameActive:gameMode];
  [touchControls setAccepting:!backgrounded.load() && !audioInterrupted.load() && focused.load()];
  if (touchProbeMode)
    touchProbe.step(callbacks, touchControls, input, [gameReport(state)[@"paused"] boolValue]);
  if (alphaMode) {
    NSDictionary *status = documents.probeStatus;
    input.block(8, [status[@"busy"] boolValue] || [status[@"picker_open"] boolValue]);
    NSDictionary *command = backgrounded.load() ? nil : [documents takeCommand];
    if (command) {
      NSString *action = command[@"action"];
      int result = 0;
      if ([action isEqual:@"load"]) {
        result = clover_apple_preview_event(state, 1);
        if (!result)
          result = clover_apple_game_start(state, [command[@"rom"] fileSystemRepresentation],
                                           [command[@"saves"] fileSystemRepresentation], error,
                                           sizeof(error));
        if (!result) {
          gameMode = true;
          importedROM = command[@"rom"];
        }
      } else if ([action isEqual:@"library"]) {
        NSData *data = [NSJSONSerialization dataWithJSONObject:command[@"snapshot"]
                                                       options:0
                                                         error:nil];
        NSString *json = [[NSString alloc] initWithData:data encoding:NSUTF8StringEncoding];
        result = clover_apple_library_scene(state, json.UTF8String, error, sizeof(error));
        if (!result)
          appliedLibraryRevision.store([command[@"snapshot"][@"revision"] unsignedLongLongValue]);
      } else if ([action isEqual:@"stop"]) {
        result = clover_apple_game_stop(state, error, sizeof(error));
        if (!result)
          gameMode = false;
      } else if ([action isEqual:@"resume"]) {
        if (!audioInterrupted.load() && focused.load())
          result = clover_apple_preview_event(state, 3);
        else {
          result = 1;
          SDL_strlcpy(error,
                      "Resume is blocked until focus returns and the audio interruption ends",
                      sizeof(error));
        }
      }
      [documents
          didApply:action
             error:result ? [NSString stringWithUTF8String:error] ?: @"Session operation failed"
                          : @""];
    }
    NSDictionary *latest = documents.probeStatus;
    libraryInputActive.store(!gameMode && ![latest[@"busy"] boolValue] &&
                             ![latest[@"scanning"] boolValue] &&
                             ![latest[@"picker_open"] boolValue]);
    [touchControls setGameActive:gameMode];
  }
  if (profileMode) {
    if (callbacks % 15 == 0)
      [profileLauncher poll];
    NSDictionary *command = backgrounded.load() ? nil : [profileLauncher takeCommand];
    if (command) {
      NSString *action = command[@"action"];
      int result = 0;
      if ([action isEqual:@"open-classic"]) {
        input.clear();
        result = clover_apple_classic_open(state, [command[@"assets"] fileSystemRepresentation],
                                           [command[@"data"] fileSystemRepresentation],
                                           [command[@"cache"] fileSystemRepresentation],
                                           [command[@"home"] fileSystemRepresentation], error,
                                           sizeof(error));
        if (!result) {
          classicActive.store(true);
          classicRevision.fetch_add(1);
        }
      } else if ([action isEqual:@"close-classic"]) {
        result = clover_apple_classic_close(state, error, sizeof(error));
        if (!result) {
          classicActive.store(false);
          classicRevision.fetch_add(1);
          input.clear();
        }
      } else if ([action isEqual:@"classic-key"]) {
        if (!audioInterrupted.load() && focused.load())
          result = clover_apple_classic_input(state, input.buttons(),
                                              [command[@"key"] unsignedIntValue]);
      } else if ([action isEqual:@"classic-error"]) {
        result = clover_apple_classic_error(state, [command[@"error"] UTF8String]);
      } else if ([action isEqual:@"load-profile"]) {
        input.clear();
        [touchControls cancelTouches];
        result = clover_apple_profile_game_start(state, [command[@"rom"] fileSystemRepresentation],
                                                 [command[@"saves"] fileSystemRepresentation],
                                                 [command[@"firmware"] fileSystemRepresentation],
                                                 error, sizeof(error));
        if (!result) {
          gameMode = true;
          if (classicActive.load() && !backgrounded.load() && !audioInterrupted.load() &&
              focused.load())
            result = clover_apple_preview_event(state, 3);
        }
      } else if ([action isEqual:@"stop"]) {
        result = clover_apple_game_stop(state, error, sizeof(error));
        if (!result) {
          gameMode = false;
          input.clear();
        }
      } else if ([action isEqual:@"resume"]) {
        if (!audioInterrupted.load() && focused.load())
          result = clover_apple_preview_event(state, 3);
        else {
          result = 1;
          SDL_strlcpy(error, "Return focus and end the audio interruption before resuming",
                      sizeof(error));
        }
      }
      [profileLauncher didApply:action error:result ? [NSString stringWithUTF8String:error] : @""];
      if (result && classicActive.load())
        clover_apple_classic_error(state, error);
      [profileLauncher poll];
      [touchControls setGameActive:gameMode];
    }
    if (classicActive.load() && !gameMode && !backgrounded.load() && !audioInterrupted.load() &&
        focused.load())
      if (clover_apple_classic_input(state, input.buttons(), 0))
        return SDL_APP_FAILURE;
  }
  if (inputProbeMode)
    inputProbe.step(callbacks, input, [gameReport(state)[@"paused"] boolValue]);
  bool resumed = lifecycleResumeCommand();
  // A probe notification posted while the boundary was active is queued just
  // like any reentrant system notification; do not step before servicing it.
  if (externalInterruption && (pendingEvents.load() & (8 | 16)))
    return SDL_APP_CONTINUE;
  uint64_t beforeResume = 0;
  if (resumed) {
    beforeResume = [gameReport(state)[@"emulated_frames"] unsignedLongLongValue];
    if (clover_apple_preview_event(state, 3))
      return SDL_APP_FAILURE;
  }
  if (externalLifecycle && clover_apple_game_buttons(state, 1))
    return SDL_APP_FAILURE;
  if (gameProbe) {
    // Explicit injected lifecycle/input probe, not evidence of OS interruptions.
    if (callbacks == 60) {
      pausedFrames = [gameReport(state)[@"emulated_frames"] unsignedLongLongValue];
      if (clover_apple_preview_event(state, 1))
        return SDL_APP_FAILURE;
    }
    if (callbacks == 80 && clover_apple_preview_event(state, 2))
      return SDL_APP_FAILURE;
    if (callbacks == 90) {
      lifecyclePassed =
          [gameReport(state)[@"emulated_frames"] unsignedLongLongValue] == pausedFrames &&
          [gameReport(state)[@"audio_queue_bytes"] unsignedLongLongValue] == 0;
      if (clover_apple_preview_event(state, 3))
        return SDL_APP_FAILURE;
    }
    if (clover_apple_game_buttons(state, callbacks >= 220 || (callbacks / 30) % 2 ? 1 : 0))
      return SDL_APP_FAILURE;
  }
  if (gameMode && !gameProbe && !externalLifecycle &&
      clover_apple_game_buttons(state, importProbeMode ? 1 : input.buttons()))
    return SDL_APP_FAILURE;
  // Reentrant lifecycle delivery during asset/core loading is already latched
  // atomically. Apply it before any step, without waiting for the boundary exit.
  if (profileMode && (backgrounded.load() || audioInterrupted.load() || !focused.load()))
    if (clover_apple_preview_event(state, audioInterrupted.load() ? 4 : 1))
      return SDL_APP_FAILURE;
  // UIKit fully covers the collection screen. Do not render/present the
  // synthetic SDL scene underneath native focus animations and scrolling.
  if (!(profileMode && !classicActive.load() && !gameMode) &&
      clover_apple_preview_iterate(state, error, sizeof(error))) {
    record(false, 0, 0, error);
    return SDL_APP_FAILURE;
  }
  if (externalLifecycle) {
    if (resumed) {
      resumeDelta = [gameReport(state)[@"emulated_frames"] unsignedLongLongValue] - beforeResume;
      lifecycleSnapshot(audioInterrupted.load() ? @"blocked" : @"resumed", state);
    }
    if (callbacks == 120)
      lifecycleSnapshot(@"ready", state);
    if (callbacks % 15 == 0)
      lifecycleSnapshot(@"status", state);
    return hostFailed ? SDL_APP_FAILURE : SDL_APP_CONTINUE;
  }
  if (alphaMode) {
    if (importProbeMode && callbacks % 15 == 0) {
      auto bytes = clover::test_fixture::nes_homebrew(0, true);
      BOOL unchanged = [[NSData dataWithContentsOfURL:importSource]
          isEqual:[NSData dataWithBytes:bytes.data() length:bytes.size()]];
      char libraryJSON[16384] = {}, libraryError[512] = {};
      NSDictionary *libraryState = @{};
      if (!clover_apple_library_report(state, libraryJSON, sizeof(libraryJSON), libraryError,
                                       sizeof(libraryError)))
        libraryState =
            [NSJSONSerialization JSONObjectWithData:[[NSString stringWithUTF8String:libraryJSON]
                                                        dataUsingEncoding:NSUTF8StringEncoding]
                                            options:0
                                              error:nil]
                ?: @{};
      NSDictionary *report = @{
        @"schema" : @1,
        @"platform" : @CLOVER_APPLE_PLATFORM,
        @"run_id" : NSProcessInfo.processInfo.environment[@"CLOVER_SMOKE_RUN_ID"],
        @"instance_id" : instanceID,
        @"command_sequence" : @(importSequence),
        @"callbacks" : @(callbacks),
        @"source_unchanged" : @(unchanged),
        @"save_persisted" : @(importProbeSavePersisted()),
        @"source_exists" : @([NSFileManager.defaultManager fileExistsAtPath:importSource.path]),
        @"library_scene" : libraryState,
        @"import" : documents.probeStatus,
        @"rom" : importedROM ?: @"",
        @"game" : gameMode ? gameReport(state) ?: @{} : @{}
      };
      NSData *data = [NSJSONSerialization dataWithJSONObject:report options:0 error:nil];
      NSString *cache =
          NSSearchPathForDirectoriesInDomains(NSCachesDirectory, NSUserDomainMask, YES).firstObject;
      if (![data writeToFile:[cache stringByAppendingPathComponent:@"clover-import-status.json"]
                     options:NSDataWritingAtomic
                       error:nil])
        return SDL_APP_FAILURE;
    }
    return SDL_APP_CONTINUE;
  }
  if (profileMode) {
    if (classicActive.load() && !gameMode && !backgrounded.load() && !audioInterrupted.load() &&
        focused.load()) {
      char actionJSON[8192] = {}, diagnostic[2048] = {};
      if (clover_apple_classic_action(state, actionJSON, sizeof(actionJSON), diagnostic,
                                      sizeof(diagnostic)))
        return SDL_APP_FAILURE;
      id action =
          [NSJSONSerialization JSONObjectWithData:[[NSString stringWithUTF8String:actionJSON]
                                                      dataUsingEncoding:NSUTF8StringEncoding]
                                          options:0
                                            error:nil];
      if ([action isKindOfClass:NSDictionary.class]) {
        if ([action[@"action"] isEqual:@"game"])
          [profileLauncher openClassicGame:action[@"game"]];
        else if ([action[@"action"] isEqual:@"collections"]) {
          if (clover_apple_classic_close(state, diagnostic, sizeof(diagnostic)))
            clover_apple_classic_error(state, diagnostic);
          else {
            classicActive.store(false);
            classicRevision.fetch_add(1);
            input.clear();
            [profileLauncher setClassicActive:NO profile:nil];
            [profileLauncher poll];
          }
        }
      }
    }
    if (transferProbeMode && callbacks % 15 == 0) {
      char classicJSON[8192] = {}, diagnostic[512] = {};
      NSDictionary *classic = @{};
      if (!clover_apple_classic_report(state, classicJSON, sizeof(classicJSON), diagnostic,
                                       sizeof(diagnostic)))
        classic =
            [NSJSONSerialization JSONObjectWithData:[[NSString stringWithUTF8String:classicJSON]
                                                        dataUsingEncoding:NSUTF8StringEncoding]
                                            options:0
                                              error:nil]
                ?: @{};
      NSDictionary *snapshot = @{
        @"schema" : @1,
        @"platform" : @CLOVER_APPLE_PLATFORM,
        @"instance_id" : instanceID,
        @"run_id" : NSProcessInfo.processInfo.environment[@"CLOVER_SMOKE_RUN_ID"],
        @"sequence" : @(transferSequence),
        @"callbacks" : @(callbacks),
        @"active" : @(gameMode.load()),
        @"classic_active" : @(classicActive.load()),
        @"classic" : classic,
        @"transfer" : profileLauncher.snapshot ?: @{},
        @"connection" : profileLauncher.connection ?: @{},
        @"game" : gameMode ? gameReport(state) ?: @{} : @{}
      };
      NSString *cache =
          NSSearchPathForDirectoriesInDomains(NSCachesDirectory, NSUserDomainMask, YES).firstObject;
      [[NSJSONSerialization dataWithJSONObject:snapshot options:0 error:nil]
          writeToFile:[cache stringByAppendingPathComponent:@"clover-transfer-status.json"]
              options:NSDataWritingAtomic
                error:nil];
    }
    return hostFailed ? SDL_APP_FAILURE : SDL_APP_CONTINUE;
  }
  if (!reported) {
    uint32_t frames = 0;
    uint64_t checksum = 0;
    // Allow the launch-screen transition to finish before the runner captures the window.
    if (callbacks >= (gameMode ? 240u : 120u)) {
      int result = clover_apple_preview_probe(state, &frames, &checksum, error, sizeof(error));
      record(result == 0 && frames >= 3 && checksum != 0, frames, checksum, error, state);
      reported = true;
    }
  }
  return SDL_APP_CONTINUE;
}

SDL_AppResult SDL_AppEvent(void *state, SDL_Event *event) {
  (void)state;
  if (event->type == SDL_EVENT_QUIT)
    return SDL_APP_SUCCESS;
  uint32_t kind = 0;
  if (profileMode && !classicActive.load() && !gameMode && event->type == SDL_EVENT_KEY_DOWN) {
    int direction = event->key.key == SDLK_UP ? -1 : event->key.key == SDLK_DOWN ? 1 : 0;
    if (direction || (event->key.key == SDLK_RETURN && !event->key.repeat)) {
      std::lock_guard lock(collectionKeyMutex);
      if (collectionKeyCount < collectionKeys.size())
        collectionKeys[collectionKeyCount++] = {classicRevision.load(), direction};
      kind |= 65536;
    }
  }
  if (profileMode && gameMode && event->type == SDL_EVENT_KEY_DOWN && !event->key.repeat &&
      event->key.key == SDLK_ESCAPE)
    kind |= 4096;
  if (classicActive.load() && !gameMode && event->type == SDL_EVENT_KEY_DOWN &&
      !event->key.repeat) {
    if (event->key.key == SDLK_ESCAPE)
      kind |= 8192;
    if (event->key.key == SDLK_TAB)
      kind |= 16384;
  }
  switch (event->type) {
  case SDL_EVENT_WILL_ENTER_BACKGROUND:
    backgrounded.store(true);
    input.block(1, true);
    kind = 1;
    if (NSThread.isMainThread)
      [documents cancelForBackground];
    else
      dispatch_async(dispatch_get_main_queue(), ^{
        [documents cancelForBackground];
      });
    break;
  case SDL_EVENT_DID_ENTER_FOREGROUND:
    backgrounded.store(false);
    input.block(1, false);
    kind = 2;
    break;
  case SDL_EVENT_WINDOW_FOCUS_LOST:
    focused.store(false);
    input.block(4, true);
    kind = 64;
    break;
  case SDL_EVENT_WINDOW_FOCUS_GAINED:
    focused.store(true);
    input.block(4, false);
    break;
  case SDL_EVENT_GAMEPAD_ADDED:
  case SDL_EVENT_GAMEPAD_REMOVED:
  case SDL_EVENT_GAMEPAD_REMAPPED:
    kind = 32;
    break;
  default:
    break;
  }
  bool accept = !backgrounded.load() && !audioInterrupted.load() && focused.load();
  if (classicActive.load() && !gameMode && accept &&
      (event->type == SDL_EVENT_FINGER_DOWN ||
       (event->type == SDL_EVENT_MOUSE_BUTTON_DOWN && event->button.button == SDL_BUTTON_LEFT &&
        event->button.which != SDL_TOUCH_MOUSEID))) {
    LibraryPoint point{classicRevision.load(), 0, 0, 0, false};
    if (event->type == SDL_EVENT_FINGER_DOWN) {
      point.window = event->tfinger.windowID;
      point.x = event->tfinger.x;
      point.y = event->tfinger.y;
    } else {
      point.window = event->button.windowID;
      point.x = event->button.x;
      point.y = event->button.y;
      point.mouse = true;
    }
    {
      std::lock_guard lock(libraryPointMutex);
      if (libraryPointCount < libraryPoints.size())
        libraryPoints[libraryPointCount++] = point;
    }
    kind |= 32768;
  } else if (libraryInputActive.load() && accept &&
             (event->type == SDL_EVENT_FINGER_DOWN || (event->type == SDL_EVENT_MOUSE_BUTTON_DOWN &&
                                                       event->button.button == SDL_BUTTON_LEFT &&
                                                       event->button.which != SDL_TOUCH_MOUSEID))) {
    LibraryPoint point{appliedLibraryRevision.load(), 0, 0, 0, false};
    if (event->type == SDL_EVENT_FINGER_DOWN) {
      point.window = event->tfinger.windowID;
      point.x = event->tfinger.x;
      point.y = event->tfinger.y;
    } else {
      point.window = event->button.windowID;
      point.x = event->button.x;
      point.y = event->button.y;
      point.mouse = true;
    }
    {
      std::lock_guard lock(libraryPointMutex);
      if (libraryPointCount < libraryPoints.size())
        libraryPoints[libraryPointCount++] = point;
    }
    kind |= 2048;
  } else if (!((alphaMode || touchProbeMode || profileMode) && gameMode &&
               (event->type == SDL_EVENT_FINGER_DOWN || event->type == SDL_EVENT_FINGER_UP ||
                event->type == SDL_EVENT_FINGER_CANCELED)) &&
             input.event(*event, accept)) {
    if (libraryInputActive.load()) {
      uint16_t mask = event->type == SDL_EVENT_KEY_DOWN ? clover::apple::Input::key(event->key.key)
                      : event->type == SDL_EVENT_GAMEPAD_BUTTON_DOWN
                          ? clover::apple::Input::button(event->gbutton.button)
                          : 0;
      if (mask & (clover::apple::InputState::Left | clover::apple::InputState::Up))
        kind |= 256;
      if (mask & (clover::apple::InputState::Right | clover::apple::InputState::Down))
        kind |= 512;
      if (mask & (clover::apple::InputState::A | clover::apple::InputState::B |
                  clover::apple::InputState::Start))
        kind |= 1024;
    } else
      kind |= 4;
  }
  if (kind)
    pendingEvents.fetch_or(kind);
  if (kind) {
    if (NSThread.isMainThread)
      servicePendingEvents();
    else
      dispatch_async(dispatch_get_main_queue(), ^{
        servicePendingEvents();
      });
  }
  return SDL_APP_CONTINUE;
}

void SDL_AppQuit(void *state, SDL_AppResult result) {
  (void)result;
  ownerState = nullptr;
  libraryInputActive.store(false);
  [documents shutdown];
  documents = nil;
  [profileLauncher shutdown];
  profileLauncher = nil;
  [touchControls shutdown];
  touchControls = nil;
  if (interruptionObserver)
    [NSNotificationCenter.defaultCenter removeObserver:interruptionObserver];
  interruptionObserver = nil;
  RustBoundary boundary;
  inputProbe.stop();
  input.stop();
  clover_apple_preview_quit(state);
}
