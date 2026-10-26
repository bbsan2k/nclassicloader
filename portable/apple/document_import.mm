#import "document_import.h"
#import <UIKit/UIKit.h>
#import <UniformTypeIdentifiers/UniformTypeIdentifiers.h>
#include <TargetConditionals.h>
#include "preview_bridge.h"
#include <atomic>
#include <memory>
#include <vector>
#include <fcntl.h>
#include <sys/stat.h>
#include <unistd.h>

namespace {
constexpr size_t Limit = 1024 * 1024;
struct Job {
  std::atomic<bool> cancelled{false};
};
static NSError *failure(NSString *message) {
  return [NSError errorWithDomain:@"CloverNESImport"
                             code:1
                         userInfo:@{NSLocalizedDescriptionKey : message}];
}
static bool copyBounded(NSURL *source, NSURL *destination, const std::shared_ptr<Job> &job,
                        NSError **error) {
  int input = open(source.fileSystemRepresentation, O_RDONLY | O_NOFOLLOW | O_CLOEXEC);
  if (input < 0) {
    *error = failure(@"Cannot open the selected regular file");
    return false;
  }
  struct stat info{};
  if (fstat(input, &info) || !S_ISREG(info.st_mode) || info.st_size < 0 || info.st_size > Limit) {
    close(input);
    *error = failure(@"Select a regular NES cartridge no larger than 1 MiB");
    return false;
  }
  int output = open(destination.fileSystemRepresentation,
                    O_WRONLY | O_CREAT | O_EXCL | O_NOFOLLOW | O_CLOEXEC, 0600);
  if (output < 0) {
    close(input);
    *error = failure(@"Cannot create private import staging file");
    return false;
  }
  size_t total = 0;
  bool ok = true;
  char buffer[32768];
  while (ok && !job->cancelled.load()) {
    ssize_t count = read(input, buffer, sizeof(buffer));
    if (count == 0)
      break;
    if (count < 0 || total + static_cast<size_t>(count) > Limit) {
      ok = false;
      break;
    }
    total += count;
    ssize_t offset = 0;
    while (offset < count) {
      ssize_t written = write(output, buffer + offset, count - offset);
      if (written <= 0) {
        ok = false;
        break;
      }
      offset += written;
    }
  }
  if (job->cancelled.load())
    ok = false;
  if (ok && fsync(output))
    ok = false;
  close(input);
  if (close(output))
    ok = false;
  if (!ok)
    *error = failure(job->cancelled.load() ? @"Import cancelled"
                                           : @"Cannot stage the complete bounded cartridge");
  return ok;
}
} // namespace

@interface CloverDocumentImport ()
#if TARGET_OS_IOS && !TARGET_OS_TV
    <UIDocumentPickerDelegate>
#endif
{
  std::shared_ptr<Job> _job;
}
@property(nonatomic, weak) UIViewController *presenter;
@property(nonatomic, strong) UIButton *primary;
@property(nonatomic, strong) UIButton *play;
@property(nonatomic, strong) UIButton *cancel;
@property(nonatomic, strong) UILabel *status;
@property(nonatomic, copy) void (^pauseHost)(void);
@property(nonatomic, strong) NSDictionary *command;
@property(nonatomic, strong) NSURL *library;
@property(nonatomic) BOOL active;
@property(nonatomic) BOOL closed;
@property(nonatomic) BOOL pickerOpen;
@property(nonatomic, strong) UIViewController *pickerIdentity;
@property(nonatomic) NSUInteger selected;
@property(nonatomic) NSUInteger cancelled;
@property(nonatomic) NSUInteger completed;
@property(nonatomic) NSUInteger rejected;
@property(nonatomic, strong) NSString *lastError;
@property(nonatomic, strong) NSArray<NSDictionary *> *catalog;
@property(nonatomic, strong) NSString *selectedDigest;
@property(nonatomic) uint64_t libraryRevision;
@property(nonatomic) BOOL libraryDirty;
@property(nonatomic) BOOL scanning;
@property(nonatomic) BOOL refreshAgain;
@property(nonatomic) BOOL limited;
@property(nonatomic) NSUInteger skipped;
@property(nonatomic) NSUInteger opened;
@property(nonatomic, strong) NSString *pendingOrigin;
@property(nonatomic, strong) UIButton *previous;
@property(nonatomic, strong) UIButton *next;
@end

@implementation CloverDocumentImport
- (instancetype)init {
  if ((self = [super init])) {
    NSURL *base = [NSFileManager.defaultManager URLsForDirectory:
#if TARGET_OS_TV
                                                    NSCachesDirectory
#else
                                                    NSApplicationSupportDirectory
#endif
                                                       inDomains:NSUserDomainMask]
                      .firstObject;
    _library = [base URLByAppendingPathComponent:@"CloverRawNES" isDirectory:YES];
    _lastError = @"";
    _catalog = @[];
    _libraryRevision = 1;
    _libraryDirty = YES;
  }
  return self;
}
- (void)install:(SDL_Window *)window pause:(void (^)(void))pause {
  self.pauseHost = pause;
#if TARGET_OS_IOS && !TARGET_OS_TV
  UIWindow *native = (__bridge UIWindow *)SDL_GetPointerProperty(
      SDL_GetWindowProperties(window), SDL_PROP_WINDOW_UIKIT_WINDOW_POINTER, nullptr);
  self.presenter = native.rootViewController;
  UIStackView *bar = [UIStackView new];
  bar.axis = UILayoutConstraintAxisHorizontal;
  bar.spacing = 12;
  bar.translatesAutoresizingMaskIntoConstraints = NO;
  self.primary = [UIButton buttonWithType:UIButtonTypeSystem];
  self.primary.accessibilityIdentifier = @"clover-import-nes";
  [self.primary setTitle:@"Import NES" forState:UIControlStateNormal];
  [self.primary addTarget:self
                   action:@selector(primaryAction)
         forControlEvents:UIControlEventTouchUpInside];
  self.play = [UIButton buttonWithType:UIButtonTypeSystem];
  self.play.accessibilityIdentifier = @"clover-play-resume";
  [self.play setTitle:@"Play / Resume" forState:UIControlStateNormal];
  self.play.enabled = NO;
  [self.play addTarget:self
                action:@selector(playAction)
      forControlEvents:UIControlEventTouchUpInside];
  self.previous = [UIButton buttonWithType:UIButtonTypeSystem];
  [self.previous setTitle:@"Prev" forState:UIControlStateNormal];
  [self.previous addTarget:self
                    action:@selector(previousAction)
          forControlEvents:UIControlEventTouchUpInside];
  self.next = [UIButton buttonWithType:UIButtonTypeSystem];
  [self.next setTitle:@"Next" forState:UIControlStateNormal];
  [self.next addTarget:self
                action:@selector(nextAction)
      forControlEvents:UIControlEventTouchUpInside];
  self.cancel = [UIButton buttonWithType:UIButtonTypeSystem];
  self.cancel.accessibilityIdentifier = @"clover-cancel-import";
  [self.cancel setTitle:@"Cancel" forState:UIControlStateNormal];
  self.cancel.hidden = YES;
  [self.cancel addTarget:self
                  action:@selector(cancelForBackground)
        forControlEvents:UIControlEventTouchUpInside];
  self.status = [UILabel new];
  self.status.text = @"Loading NES library…";
  self.status.textColor = UIColor.whiteColor;
  self.status.accessibilityIdentifier = @"clover-import-status";
  self.status.font = [UIFont systemFontOfSize:13];
  for (UIView *view in
       @[ self.primary, self.previous, self.next, self.play, self.cancel, self.status ])
    [bar addArrangedSubview:view];
  bar.backgroundColor = [UIColor.blackColor colorWithAlphaComponent:0.85];
  [self.presenter.view addSubview:bar];
  [NSLayoutConstraint activateConstraints:@[
    [bar.leadingAnchor constraintEqualToAnchor:self.presenter.view.safeAreaLayoutGuide.leadingAnchor
                                      constant:12],
    [bar.topAnchor constraintEqualToAnchor:self.presenter.view.safeAreaLayoutGuide.topAnchor
                                  constant:8],
    [bar.trailingAnchor
        constraintLessThanOrEqualToAnchor:self.presenter.view.safeAreaLayoutGuide.trailingAnchor
                                 constant:-12],
    [bar.heightAnchor constraintGreaterThanOrEqualToConstant:44]
  ]];
#else
  (void)window;
#endif
}
- (void)primaryAction {
  if (self.closed || _job || self.pickerOpen || self.command)
    return;
  if (self.active) {
    self.command = @{@"action" : @"stop"};
    return;
  }
#if TARGET_OS_IOS && !TARGET_OS_TV
  if (!self.presenter || self.presenter.presentedViewController)
    return;
  self.pauseHost();
  self.pickerOpen = YES;
  UIDocumentPickerViewController *picker =
      [[UIDocumentPickerViewController alloc] initForOpeningContentTypes:@[ UTTypeData ] asCopy:NO];
  picker.delegate = self;
  picker.allowsMultipleSelection = NO;
  self.pickerIdentity = picker;
  [self.presenter presentViewController:picker animated:YES completion:nil];
#endif
}
- (void)updateButtons {
  self.play.enabled = !self.closed && !_job && !self.pickerOpen && !self.command &&
                      (self.active || (!self.scanning && self.selectedDigest.length));
  [self.play setTitle:self.active ? @"Play / Resume" : @"Open Selected"
             forState:UIControlStateNormal];
  self.previous.enabled = self.next.enabled =
      !self.active && !_job && !self.scanning && self.catalog.count > 1;
}
- (void)playAction {
  if (self.closed || self.command || _job || self.pickerOpen)
    return;
  if (self.active)
    self.command = @{@"action" : @"resume"};
  else
    [self openSelection];
}
- (void)markLibraryDirty {
  if (self.libraryRevision == UINT64_MAX) {
    self.lastError = @"Library revision exhausted; restart the app";
    return;
  }
  ++self.libraryRevision;
  self.libraryDirty = YES;
}
- (void)refreshLibrary {
  if (self.closed)
    return;
  if (self.scanning) {
    self.refreshAgain = YES;
    return;
  }
  self.scanning = YES;
  [self updateButtons];
  NSURL *root = self.library;
  __weak CloverDocumentImport *weakSelf = self;
  [NSThread detachNewThreadWithBlock:^{
    @autoreleasepool {
      std::vector<char> json(262144);
      char diagnostic[2048] = {};
      NSDictionary *snapshot = nil;
      NSString *message = @"";
      if (clover_apple_library_scan(root.fileSystemRepresentation, json.data(), json.size(),
                                    diagnostic, sizeof(diagnostic)))
        message = [NSString stringWithUTF8String:diagnostic] ?: @"Cannot scan the NES library";
      else
        snapshot =
            [NSJSONSerialization JSONObjectWithData:[[NSString stringWithUTF8String:json.data()]
                                                        dataUsingEncoding:NSUTF8StringEncoding]
                                            options:0
                                              error:nil];
      dispatch_async(dispatch_get_main_queue(), ^{
        CloverDocumentImport *self = weakSelf;
        if (!self || self.closed)
          return;
        self.scanning = NO;
        if (snapshot) {
          self.catalog = snapshot[@"entries"];
          self.limited = [snapshot[@"truncated"] boolValue];
          self.skipped = [snapshot[@"skipped"] unsignedIntegerValue];
          BOOL retained = NO;
          for (NSDictionary *entry in self.catalog)
            retained |= [entry[@"digest"] isEqual:self.selectedDigest];
          if (!retained)
            self.selectedDigest = self.catalog.firstObject[@"digest"];
          [self markLibraryDirty];
          if (!self.active && !self->_job)
            self.status.text = [NSString
                stringWithFormat:@"%lu cartridges%@", (unsigned long)self.catalog.count,
                                 self.skipped || self.limited ? @" — library warnings" : @""];
        } else {
          self.lastError = message;
          self.status.text = message;
        }
        [self updateButtons];
        if (self.refreshAgain) {
          self.refreshAgain = NO;
          [self refreshLibrary];
        }
      });
    }
  }];
}
- (void)selectDigest:(NSString *)digest {
  if (self.closed || self.active || _job || self.scanning || self.pickerOpen || self.command)
    return;
  for (NSDictionary *entry in self.catalog)
    if ([entry[@"digest"] isEqual:digest]) {
      if (![self.selectedDigest isEqual:digest]) {
        self.selectedDigest = digest;
        [self markLibraryDirty];
      }
      return;
    }
}
- (void)moveSelection:(NSInteger)delta {
  if (!self.catalog.count || self.active || _job || self.scanning || self.command ||
      self.pickerOpen || self.closed)
    return;
  NSInteger index = 0;
  for (NSUInteger n = 0; n < self.catalog.count; ++n)
    if ([self.catalog[n][@"digest"] isEqual:self.selectedDigest])
      index = (NSInteger)n;
  NSInteger count = (NSInteger)self.catalog.count;
  [self selectDigest:self.catalog[(index + delta % count + count) % count][@"digest"]];
}
- (void)previousAction {
  [self moveSelection:-1];
}
- (void)nextAction {
  [self moveSelection:1];
}
- (void)openSelection {
  if (self.closed || self.active || _job || self.scanning || self.command || self.pickerOpen ||
      !self.selectedDigest.length)
    return;
  self.pauseHost();
  auto job = std::make_shared<Job>();
  _job = job;
  NSString *digest = self.selectedDigest;
  NSURL *root = self.library;
  self.status.text = @"Checking stored cartridge…";
  self.primary.enabled = NO;
  self.cancel.hidden = NO;
  [self updateButtons];
  __weak CloverDocumentImport *weakSelf = self;
  [NSThread detachNewThreadWithBlock:^{
    @autoreleasepool {
      char json[4096] = {}, diagnostic[2048] = {};
      NSDictionary *prepared = nil;
      NSString *message = @"";
      if (clover_apple_library_open(root.fileSystemRepresentation, digest.UTF8String, json,
                                    sizeof(json), diagnostic, sizeof(diagnostic)))
        message = [NSString stringWithUTF8String:diagnostic] ?: @"Stored cartridge is unavailable";
      else
        prepared =
            [NSJSONSerialization JSONObjectWithData:[[NSString stringWithUTF8String:json]
                                                        dataUsingEncoding:NSUTF8StringEncoding]
                                            options:0
                                              error:nil];
      dispatch_async(dispatch_get_main_queue(), ^{
        CloverDocumentImport *self = weakSelf;
        if (!self || self.closed || self->_job != job)
          return;
        self->_job.reset();
        self.primary.enabled = YES;
        self.cancel.hidden = YES;
        if (job->cancelled.load()) {
          self.status.text = @"Open cancelled";
          [self updateButtons];
          return;
        }
        self.lastError = message;
        if (!prepared) {
          ++self.rejected;
          self.status.text = message;
          [self refreshLibrary];
          return;
        }
        self.pendingOrigin = @"library";
        self.command = @{
          @"action" : @"load",
          @"rom" : prepared[@"rom"],
          @"saves" : prepared[@"saves"],
          @"digest" : prepared[@"digest"]
        };
        [self updateButtons];
      });
    }
  }];
}
- (void)probeCancel {
  [self cancelForBackground];
}
- (void)probeAction:(NSString *)action url:(NSURL *)url {
  if ([action isEqual:@"show"])
    [self primaryAction];
  else if ([action isEqual:@"cancel"]) {
    UIViewController *retired = self.pickerIdentity;
    [self probeCancel];
#if TARGET_OS_IOS && !TARGET_OS_TV
    // Test-only delayed delegate result after cancellation must be ignored.
    if (retired)
      [self documentPicker:(UIDocumentPickerViewController *)retired
          didPickDocumentsAtURLs:@[ url ]];
#else
    (void)retired;
#endif
  } else if ([action isEqual:@"play"])
    [self playAction];
  else if ([action isEqual:@"stop"]) {
    if (self.active)
      [self primaryAction];
  } else if ([action isEqual:@"select"] || [action isEqual:@"invalid"])
    [self probeSelect:url];
  else if ([action isEqual:@"cancel-import"]) {
    [self probeSelect:url];
    [self probeCancel];
  } else if ([action isEqual:@"refresh"])
    [self refreshLibrary];
  else if ([action isEqual:@"open"])
    [self openSelection];
  else if ([action isEqual:@"next"])
    [self moveSelection:1];
  else if ([action isEqual:@"previous"])
    [self moveSelection:-1];
}
- (void)cancelForBackground {
  if (self.closed)
    return;
  if (_job) {
    _job->cancelled.store(true);
    self.status.text = @"Cancelling import…";
    ++self.cancelled;
  }
  if (self.pickerOpen) {
    self.pickerOpen = NO;
    ++self.cancelled;
    self.pickerIdentity = nil;
    [self.presenter dismissViewControllerAnimated:YES completion:nil];
  }
  // Invalidates a prepared result not yet consumed by the owner callback.
  if ([self.command[@"action"] isEqual:@"load"]) {
    self.command = nil;
    ++self.cancelled;
  }
  [self updateButtons];
}
#if TARGET_OS_IOS && !TARGET_OS_TV
- (void)documentPickerWasCancelled:(UIDocumentPickerViewController *)controller {
  if (controller != self.pickerIdentity || !self.pickerOpen)
    return;
  self.pickerIdentity = nil;
  self.pickerOpen = NO;
  ++self.cancelled;
  self.status.text = @"Import cancelled; gameplay remains paused";
}
- (void)documentPicker:(UIDocumentPickerViewController *)controller
    didPickDocumentsAtURLs:(NSArray<NSURL *> *)urls {
  if (controller != self.pickerIdentity || !self.pickerOpen)
    return;
  self.pickerIdentity = nil;
  self.pickerOpen = NO;
  if (urls.count == 1)
    [self probeSelect:urls.firstObject];
}
#endif
- (void)probeSelect:(NSURL *)url {
  if (self.closed || self.active || _job || self.command)
    return;
  ++self.selected;
  self.pauseHost();
  self.primary.enabled = NO;
  self.cancel.hidden = NO;
  self.status.text = @"Staging and validating NES cartridge…";
  auto job = std::make_shared<Job>();
  _job = job;
  NSURL *library = self.library;
  __weak CloverDocumentImport *weakSelf = self;
  [NSThread detachNewThreadWithBlock:^{
    @autoreleasepool {
      NSError *error = nil;
      NSDictionary *prepared = nil;
      BOOL scoped = [url startAccessingSecurityScopedResource];
      NSURL *cache = [NSFileManager.defaultManager URLsForDirectory:NSCachesDirectory
                                                          inDomains:NSUserDomainMask]
                         .firstObject;
      NSURL *stage =
          [cache URLByAppendingPathComponent:[@"clover-picked-"
                                                 stringByAppendingString:NSUUID.UUID.UUIDString]
                                 isDirectory:YES];
      BOOL owned = NO;
      @try {
        if (!url.isFileURL)
          error = failure(@"Select a file URL, not a network URL");
        else if ((owned = [NSFileManager.defaultManager createDirectoryAtURL:stage
                                                 withIntermediateDirectories:NO
                                                                  attributes:nil
                                                                       error:&error])) {
          NSURL *copy = [stage URLByAppendingPathComponent:@"source.nes"];
          NSFileCoordinator *coordinator = [[NSFileCoordinator alloc] initWithFilePresenter:nil];
          __block NSError *copyError = nil;
          __block BOOL copied = NO;
          [coordinator coordinateReadingItemAtURL:url
                                          options:NSFileCoordinatorReadingWithoutChanges
                                            error:&error
                                       byAccessor:^(NSURL *readURL) {
                                         copied = copyBounded(readURL, copy, job, &copyError);
                                       }];
          if (!error)
            error = copyError;
          if (!copied && !error)
            error = failure(@"File provider did not supply a readable cartridge");
          if (copied && !error && !job->cancelled.load()) {
            char json[4096] = {}, diagnostic[2048] = {};
            if (clover_apple_import_nes(copy.fileSystemRepresentation,
                                        library.fileSystemRepresentation, json, sizeof(json),
                                        diagnostic, sizeof(diagnostic)))
              error = failure([NSString stringWithUTF8String:diagnostic]
                                  ?: @"Cannot validate cartridge");
            else {
              NSData *data =
                  [[NSString stringWithUTF8String:json] dataUsingEncoding:NSUTF8StringEncoding];
              prepared = [NSJSONSerialization JSONObjectWithData:data options:0 error:&error];
            }
          }
        }
      } @catch (NSException *exception) {
        error = failure(exception.reason ?: @"File-provider import failed");
      } @finally {
        if (owned)
          [NSFileManager.defaultManager removeItemAtURL:stage error:nil];
        if (scoped)
          [url stopAccessingSecurityScopedResource];
      }
      dispatch_async(dispatch_get_main_queue(), ^{
        CloverDocumentImport *self = weakSelf;
        if (!self || self.closed || self->_job != job)
          return;
        self->_job.reset();
        self.primary.enabled = YES;
        self.cancel.hidden = YES;
        if (job->cancelled.load()) {
          self.status.text = @"Import cancelled";
          [self updateButtons];
          return;
        }
        self.lastError = error.localizedDescription ?: @"";
        if (!prepared || error) {
          ++self.rejected;
          self.status.text = self.lastError;
          [self updateButtons];
          return;
        }
        ++self.completed;
        self.pendingOrigin = @"import";
        self.selectedDigest = prepared[@"digest"];
        self.command = @{
          @"action" : @"load",
          @"rom" : prepared[@"rom"],
          @"saves" : prepared[@"saves"],
          @"digest" : prepared[@"digest"]
        };
        self.status.text = @"Cartridge imported; preparing session…";
        [self refreshLibrary];
      });
    }
  }];
}
- (NSDictionary *)takeCommand {
  if (self.command) {
    NSDictionary *command = self.command;
    self.command = nil;
    return command;
  }
  if (self.libraryDirty && !self.active && !_job && !self.pickerOpen) {
    self.libraryDirty = NO;
    NSMutableArray *ids = [NSMutableArray new];
    for (NSDictionary *entry in self.catalog)
      [ids addObject:entry[@"digest"]];
    return @{
      @"action" : @"library",
      @"snapshot" : @{
        @"revision" : @(self.libraryRevision),
        @"entries" : ids,
        @"selected" : self.selectedDigest ?: @"",
        @"can_import" : @(!TARGET_OS_TV),
        @"loading" : @(self.scanning),
        @"limited" : @(self.limited)
      }
    };
  }
  return nil;
}
- (void)didApply:(NSString *)action error:(NSString *)error {
  if (error.length) {
    self.status.text = error;
    self.lastError = error;
    ++self.rejected;
    [self updateButtons];
    if ([action isEqual:@"load"])
      [self refreshLibrary];
    return;
  }
  if ([action isEqual:@"load"]) {
    self.active = YES;
    if ([self.pendingOrigin isEqual:@"library"])
      ++self.opened;
    self.pendingOrigin = nil;
    self.status.text = @"Ready — tap Play / Resume";
  }
  if ([action isEqual:@"stop"]) {
    self.active = NO;
    self.status.text = @"NES library";
    [self markLibraryDirty];
    [self refreshLibrary];
  }
  if ([action isEqual:@"resume"])
    self.status.text = @"Playing NES cartridge";
  [self.primary setTitle:self.active ? @"Stop" : @"Import NES" forState:UIControlStateNormal];
  [self updateButtons];
}
- (NSDictionary *)probeStatus {
  BOOL presented = NO;
#if TARGET_OS_IOS && !TARGET_OS_TV
  presented =
      [self.presenter.presentedViewController isKindOfClass:UIDocumentPickerViewController.class];
#endif
  return @{
    @"selected" : @(self.selected),
    @"cancelled" : @(self.cancelled),
    @"completed" : @(self.completed),
    @"rejected" : @(self.rejected),
    @"busy" : @(_job != nullptr),
    @"active" : @(self.active),
    @"error" : self.lastError ?: @"",
    @"picker_supported" : @(!TARGET_OS_TV),
    @"library_count" : @(self.catalog.count),
    @"library_revision" : @(self.libraryRevision),
    @"selected_digest" : self.selectedDigest ?: @"",
    @"scanning" : @(self.scanning),
    @"library_skipped" : @(self.skipped),
    @"library_limited" : @(self.limited),
    @"opened" : @(self.opened),
    @"picker_open" : @(self.pickerOpen),
    @"picker_presented" : @(presented)
  };
}
- (void)shutdown {
  [self cancelForBackground];
  self.closed = YES;
  self.command = nil;
  self.pauseHost = nil;
}
@end
