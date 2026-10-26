#import "profile_launcher.h"
#import <UIKit/UIKit.h>
#import <CoreImage/CoreImage.h>
#import <Security/Security.h>
#import <UniformTypeIdentifiers/UniformTypeIdentifiers.h>
#include <TargetConditionals.h>
#include <ifaddrs.h>
#include <arpa/inet.h>
#include "preview_bridge.h"
#include <vector>
#include <fcntl.h>
#if TARGET_OS_TV
#import "launcher_style.h"
#endif

namespace {
static NSString *localAddress() {
  ifaddrs *interfaces = nullptr;
  NSString *result = nil;
  if (!getifaddrs(&interfaces)) {
    for (auto *it = interfaces; it; it = it->ifa_next) {
      if (!it->ifa_addr || it->ifa_addr->sa_family != AF_INET)
        continue;
      auto *address = (sockaddr_in *)it->ifa_addr;
      uint32_t ip = ntohl(address->sin_addr.s_addr);
      if ((ip >> 24) != 10 && (ip >> 20) != 0xac1 && (ip >> 16) != 0xc0a8 && (ip >> 16) != 0xa9fe)
        continue;
      char text[INET_ADDRSTRLEN] = {};
      inet_ntop(AF_INET, &address->sin_addr, text, sizeof(text));
      result = [NSString stringWithUTF8String:text];
      if (!strcmp(it->ifa_name, "en0"))
        break;
    }
    freeifaddrs(interfaces);
  }
  return result;
}
static NSDictionary *decode(const char *json) {
  NSData *data = [[NSString stringWithUTF8String:json] dataUsingEncoding:NSUTF8StringEncoding];
  id result = data ? [NSJSONSerialization JSONObjectWithData:data options:0 error:nil] : nil;
  return [result isKindOfClass:NSDictionary.class] ? result : nil;
}
} // namespace
@interface CloverProfileLauncher () <UITableViewDataSource, UITableViewDelegate
#if TARGET_OS_IOS && !TARGET_OS_TV
                                     ,
                                     UIDocumentPickerDelegate
#endif
                                     > {
  void *_transfer;
}
@property(nonatomic, strong) UIView *container;
@property(nonatomic, strong) UIStackView *contents;
@property(nonatomic, strong) UITableView *table;
@property(nonatomic, strong) UILabel *address;
@property(nonatomic, strong) UILabel *status;
@property(nonatomic, strong) UIImageView *qr;
@property(nonatomic, strong) UIProgressView *progress;
@property(nonatomic, strong) UIButton *resume;
@property(nonatomic, strong) UIButton *stop;
@property(nonatomic, strong) UIButton *cancel;
@property(nonatomic, strong) UIButton *importImage;
@property(nonatomic, strong) UIButton *collectionBack;
@property(nonatomic, strong) UIButton *codeRefresh;
@property(nonatomic, strong) UILabel *codeLabel;
@property(nonatomic, strong) UILabel *pairStatus;
@property(nonatomic, strong) UILabel *libraryCount;
@property(nonatomic, strong) NSString *pin;
@property(nonatomic, strong) UIViewController *picker;
@property(nonatomic, weak) UIViewController *presenter;
@property(nonatomic, strong) NSArray<NSDictionary *> *profiles;
@property(nonatomic, strong) NSDictionary *snapshot;
@property(nonatomic, strong) NSDictionary *command;
@property(nonatomic, strong) NSDictionary *connection;
@property(nonatomic, strong) NSString *hostError;
@property(nonatomic) BOOL foreground;
@property(nonatomic) BOOL active;
@property(nonatomic) BOOL classicActive;
@property(nonatomic, strong) NSString *classicProfile;
@property(nonatomic, strong) NSString *focusedProfile;
@property(nonatomic, strong) NSString *openingProfile;
@property(nonatomic) NSUInteger tableReloads;
@property(nonatomic) uint64_t focusGeneration;
@property(nonatomic) BOOL restoringFocus;
@property(nonatomic) BOOL enabled;
@property(nonatomic) BOOL closed;
@property(nonatomic) uint64_t revision;
@property(nonatomic, strong) NSLayoutConstraint *bottom;
@property(nonatomic, strong) NSLayoutConstraint *gameHeight;
- (void)configureTransfer:(BOOL)renew;
#if TARGET_OS_TV
- (void)buildTV;
- (void)updateTVLibrary;
#endif
#if TARGET_OS_IOS && !TARGET_OS_TV
- (void)selectLocalURL:(NSURL *)source;
#endif
@end
@implementation CloverProfileLauncher
#if TARGET_OS_TV
- (void)buildTV {
  self.contents = CloverColumn(26);
  [self.container addSubview:self.contents];
  self.contents.translatesAutoresizingMaskIntoConstraints = NO;
  self.bottom = [self.contents.bottomAnchor
      constraintEqualToAnchor:self.container.safeAreaLayoutGuide.bottomAnchor
                     constant:-20];
  self.gameHeight = [self.contents.heightAnchor constraintEqualToConstant:72];
  [NSLayoutConstraint activateConstraints:@[
    [self.contents.leadingAnchor
        constraintEqualToAnchor:self.container.safeAreaLayoutGuide.leadingAnchor
                       constant:32],
    [self.contents.trailingAnchor
        constraintEqualToAnchor:self.container.safeAreaLayoutGuide.trailingAnchor
                       constant:-32],
    [self.contents.topAnchor constraintEqualToAnchor:self.container.safeAreaLayoutGuide.topAnchor
                                            constant:24],
    self.bottom
  ]];
  UIStackView *bar = [UIStackView new];
  bar.spacing = 24;
  bar.alignment = UIStackViewAlignmentCenter;
  UIImageView *mark =
      [[UIImageView alloc] initWithImage:[UIImage systemImageNamed:@"gamecontroller.fill"]];
  mark.tintColor = CloverMint();
  mark.contentMode = UIViewContentModeScaleAspectFit;
  [mark.widthAnchor constraintEqualToConstant:54].active = YES;
  [mark.heightAnchor constraintEqualToConstant:54].active = YES;
  [bar addArrangedSubview:mark];
  UIStackView *brand = CloverColumn(4);
  [brand addArrangedSubview:CloverLabel(@"clover", 42, UIFontWeightBold, CloverInk())];
  [brand addArrangedSubview:CloverLabel(@"YOUR CLASSIC COLLECTION", 15, UIFontWeightSemibold,
                                        CloverMuted())];
  [bar addArrangedSubview:brand];
  UIView *space = [UIView new];
  [bar addArrangedSubview:space];
  self.resume = [UIButton buttonWithType:UIButtonTypeSystem];
  [self.resume setTitle:@"Play / Resume" forState:UIControlStateNormal];
  [self.resume addTarget:self
                  action:@selector(resumeAction)
        forControlEvents:UIControlEventPrimaryActionTriggered];
  self.stop = [UIButton buttonWithType:UIButtonTypeSystem];
  [self.stop setTitle:@"Back to library" forState:UIControlStateNormal];
  [self.stop addTarget:self
                action:@selector(stopAction)
      forControlEvents:UIControlEventPrimaryActionTriggered];
  self.cancel = [UIButton buttonWithType:UIButtonTypeSystem];
  [self.cancel setTitle:@"Cancel transfer" forState:UIControlStateNormal];
  [self.cancel addTarget:self
                  action:@selector(cancelAction)
        forControlEvents:UIControlEventPrimaryActionTriggered];
  for (UIView *view in @[ self.resume, self.stop, self.cancel ])
    [bar addArrangedSubview:view];
  [self.contents addArrangedSubview:bar];
  UIStackView *body = [UIStackView new];
  body.spacing = 32;
  body.alignment = UIStackViewAlignmentFill;
  UIStackView *library = CloverColumn(14);
  [body addArrangedSubview:library];
  [library addArrangedSubview:CloverLabel(@"Classic collections", 32, UIFontWeightSemibold,
                                          CloverInk())];
  self.libraryCount = CloverLabel(@"No games imported yet", 20, UIFontWeightRegular, CloverMuted());
  [library addArrangedSubview:self.libraryCount];
  self.table = [[UITableView alloc] initWithFrame:CGRectZero style:UITableViewStylePlain];
  self.table.backgroundColor = UIColor.clearColor;
  self.table.dataSource = self;
  self.table.delegate = self;
  self.table.rowHeight = 124;
  self.table.clipsToBounds = NO;
  self.table.remembersLastFocusedIndexPath = YES;
  UITapGestureRecognizer *select =
      [[UITapGestureRecognizer alloc] initWithTarget:self
                                              action:@selector(selectFocusedCollection:)];
  select.allowedPressTypes = @[ @(UIPressTypeSelect) ];
  select.allowedTouchTypes = @[];
  select.cancelsTouchesInView = NO;
  [self.table addGestureRecognizer:select];
  UITapGestureRecognizer *pointer =
      [[UITapGestureRecognizer alloc] initWithTarget:self action:@selector(selectCollection:)];
  pointer.allowedPressTypes = @[];
  pointer.cancelsTouchesInView = NO;
  [self.table addGestureRecognizer:pointer];
  self.table.accessibilityIdentifier = @"clover-profile-games";
  [library addArrangedSubview:self.table];
  self.status = CloverLabel(@"Checking your collection…", 20, UIFontWeightRegular, CloverMuted());
  self.status.numberOfLines = 2;
  self.status.accessibilityIdentifier = @"clover-profile-status";
  [library addArrangedSubview:self.status];
  self.progress = [UIProgressView new];
  self.progress.progressTintColor = CloverMint();
  self.progress.trackTintColor = CloverPanel();
  [library addArrangedSubview:self.progress];
  UIView *panel = [UIView new];
  panel.backgroundColor = CloverPanel();
  panel.layer.cornerRadius = 28;
  [panel.widthAnchor constraintEqualToConstant:460].active = YES;
  [body addArrangedSubview:panel];
  UIStackView *transfer = CloverColumn(15);
  [panel addSubview:transfer];
  CloverPin(transfer, panel, 28);
  [transfer addArrangedSubview:CloverLabel(@"LOCAL TRANSFER", 16, UIFontWeightBold, CloverMint())];
  [transfer
      addArrangedSubview:CloverLabel(@"Add a collection", 30, UIFontWeightSemibold, CloverInk())];
  [transfer addArrangedSubview:CloverLabel(@"On your phone or computer, open", 20,
                                           UIFontWeightRegular, CloverMuted())];
  self.address = CloverLabel(@"Connect to your local network", 20, UIFontWeightMedium, CloverInk());
  self.address.font = [UIFont monospacedSystemFontOfSize:20 weight:UIFontWeightMedium];
  self.address.accessibilityIdentifier = @"clover-transfer-address";
  [transfer addArrangedSubview:self.address];
  [transfer addArrangedSubview:CloverLabel(@"THEN ENTER THIS CODE", 14, UIFontWeightSemibold,
                                           CloverMuted())];
  self.codeLabel = CloverLabel(@"— — —", 60, UIFontWeightSemibold, CloverInk());
  self.codeLabel.font = [UIFont monospacedDigitSystemFontOfSize:60 weight:UIFontWeightSemibold];
  self.codeLabel.adjustsFontSizeToFitWidth = YES;
  self.codeLabel.minimumScaleFactor = 0.5;
  self.codeLabel.numberOfLines = 1;
  self.codeLabel.accessibilityIdentifier = @"clover-pairing-code";
  [transfer addArrangedSubview:self.codeLabel];
  self.pairStatus =
      CloverLabel(@"Six digits. No account needed.", 18, UIFontWeightRegular, CloverMuted());
  self.pairStatus.numberOfLines = 2;
  [transfer addArrangedSubview:self.pairStatus];
  UIStackView *qrRow = [UIStackView new];
  qrRow.spacing = 18;
  qrRow.alignment = UIStackViewAlignmentCenter;
  self.qr = [UIImageView new];
  self.qr.contentMode = UIViewContentModeScaleAspectFit;
  self.qr.layer.magnificationFilter = kCAFilterNearest;
  [self.qr.widthAnchor constraintEqualToConstant:156].active = YES;
  [self.qr.heightAnchor constraintEqualToConstant:156].active = YES;
  [qrRow addArrangedSubview:self.qr];
  [qrRow addArrangedSubview:CloverLabel(@"Or scan to\nconnect instantly", 20, UIFontWeightRegular,
                                        CloverMuted())];
  [transfer addArrangedSubview:qrRow];
  self.codeRefresh = [UIButton buttonWithType:UIButtonTypeSystem];
  [self.codeRefresh setTitle:@"New code" forState:UIControlStateNormal];
  self.codeRefresh.accessibilityIdentifier = @"clover-new-pairing-code";
  [self.codeRefresh addTarget:self
                       action:@selector(newCodeAction)
             forControlEvents:UIControlEventPrimaryActionTriggered];
  [transfer addArrangedSubview:self.codeRefresh];
  [transfer
      addArrangedSubview:CloverLabel(
                             @"Choose a .hsqs Classic image. Keep Clover open while it imports.",
                             18, UIFontWeightRegular, CloverMuted())];
  UIView *spacer = [UIView new];
  [transfer addArrangedSubview:spacer];
  [spacer setContentHuggingPriority:1 forAxis:UILayoutConstraintAxisVertical];
  [self.contents addArrangedSubview:body];
  UILabel *warning =
      CloverLabel(@"Trusted Wi-Fi only · Local HTTP is not encrypted. Keep your original images — "
                  @"tvOS may remove cached content. Save backup is not available yet.",
                  16, UIFontWeightRegular, CloverMuted());
  warning.numberOfLines = 2;
  [self.contents addArrangedSubview:warning];
  self.resume.hidden = YES;
  self.stop.hidden = YES;
  self.cancel.hidden = YES;
}
- (void)updateTVLibrary {
  self.libraryCount.text =
      self.profiles.count
          ? [NSString stringWithFormat:@"%lu %@ · Select to launch the Classic menu",
                                       (unsigned long)self.profiles.count,
                                       self.profiles.count == 1 ? @"collection" : @"collections"]
          : @"NES + SNES · Ready when you are";
  if (self.profiles.count) {
    self.table.backgroundView = nil;
    return;
  }
  UIView *empty = [UIView new];
  UIStackView *message = CloverColumn(18);
  message.alignment = UIStackViewAlignmentCenter;
  UIImageView *icon =
      [[UIImageView alloc] initWithImage:[UIImage systemImageNamed:@"gamecontroller"]];
  icon.tintColor = CloverMint();
  icon.contentMode = UIViewContentModeScaleAspectFit;
  [icon.widthAnchor constraintEqualToConstant:94].active = YES;
  [icon.heightAnchor constraintEqualToConstant:74].active = YES;
  [message addArrangedSubview:icon];
  UILabel *title =
      CloverLabel(@"Your collection starts here", 30, UIFontWeightSemibold, CloverInk());
  title.textAlignment = NSTextAlignmentCenter;
  [message addArrangedSubview:title];
  UILabel *detail = CloverLabel(
      @"Bring your classics to the big screen.\nUse the transfer panel to add your first image.",
      22, UIFontWeightRegular, CloverMuted());
  detail.textAlignment = NSTextAlignmentCenter;
  [message addArrangedSubview:detail];
  [empty addSubview:message];
  message.translatesAutoresizingMaskIntoConstraints = NO;
  [NSLayoutConstraint activateConstraints:@[
    [message.centerXAnchor constraintEqualToAnchor:empty.centerXAnchor],
    [message.centerYAnchor constraintEqualToAnchor:empty.centerYAnchor],
    [message.widthAnchor constraintEqualToAnchor:empty.widthAnchor constant:-96],
    [title.widthAnchor constraintEqualToAnchor:message.widthAnchor],
    [detail.widthAnchor constraintEqualToAnchor:message.widthAnchor]
  ]];
  self.table.backgroundView = empty;
}
#endif
- (BOOL)install:(SDL_Window *)window error:(NSString **)error {
  NSURL *base = [NSFileManager.defaultManager URLsForDirectory:
#if TARGET_OS_TV
                                                  NSCachesDirectory
#else
                                                  NSApplicationSupportDirectory
#endif
                                                     inDomains:NSUserDomainMask]
                    .firstObject;
  char diagnostic[2048] = {};
  _transfer =
      clover_transfer_create([[base URLByAppendingPathComponent:@"CloverProfiles"
                                                    isDirectory:YES] fileSystemRepresentation],
                             diagnostic, sizeof(diagnostic));
  if (!_transfer) {
    if (error)
      *error = [NSString stringWithUTF8String:diagnostic];
    return NO;
  }
  UIWindow *native =
      window ? (__bridge UIWindow *)SDL_GetPointerProperty(
                   SDL_GetWindowProperties(window), SDL_PROP_WINDOW_UIKIT_WINDOW_POINTER, nullptr)
             : nil;
  UIView *root = native.rootViewController.view;
  if (!root) {
    if (error)
      *error = @"No native launcher view";
    [self shutdown];
    return NO;
  }
  self.profiles = @[];
  self.presenter = native.rootViewController;
#if TARGET_OS_TV
  self.container = [CloverCollectionRootView new];
#else
  self.container = [UIView new];
#endif
  self.container.translatesAutoresizingMaskIntoConstraints = NO;
  self.container.backgroundColor = [UIColor colorWithRed:0.063 green:0.09 blue:0.15 alpha:1];
  [root addSubview:self.container];
  [NSLayoutConstraint activateConstraints:@[
    [self.container.leadingAnchor constraintEqualToAnchor:root.leadingAnchor],
    [self.container.trailingAnchor constraintEqualToAnchor:root.trailingAnchor],
    [self.container.topAnchor constraintEqualToAnchor:root.topAnchor],
    [self.container.bottomAnchor constraintEqualToAnchor:root.bottomAnchor]
  ]];
#if TARGET_OS_TV
  [self buildTV];
  [self setForeground:YES];
  [self poll];
  return YES;
#else
  self.container.backgroundColor = UIColor.systemGroupedBackgroundColor;
  self.contents = [UIStackView new];
  self.contents.axis = UILayoutConstraintAxisVertical;
  self.contents.spacing = 8;
  self.contents.translatesAutoresizingMaskIntoConstraints = NO;
  [self.container addSubview:self.contents];
  self.bottom = [self.contents.bottomAnchor
      constraintEqualToAnchor:self.container.safeAreaLayoutGuide.bottomAnchor
                     constant:-12];
  self.gameHeight = [self.contents.heightAnchor constraintEqualToConstant:TARGET_OS_TV ? 72 : 44];
  [NSLayoutConstraint activateConstraints:@[
    [self.contents.leadingAnchor
        constraintEqualToAnchor:self.container.safeAreaLayoutGuide.leadingAnchor
                       constant:24],
    [self.contents.trailingAnchor
        constraintEqualToAnchor:self.container.safeAreaLayoutGuide.trailingAnchor
                       constant:-24],
    [self.contents.topAnchor constraintEqualToAnchor:self.container.safeAreaLayoutGuide.topAnchor
                                            constant:12],
    self.bottom
  ]];
  UIStackView *bar = [UIStackView new];
  bar.spacing = 16;
  bar.alignment = UIStackViewAlignmentCenter;
  UILabel *title = [UILabel new];
  title.text = @"Clover";
  title.textColor = UIColor.labelColor;
  title.font = [UIFont preferredFontForTextStyle:UIFontTextStyleLargeTitle];
  title.adjustsFontForContentSizeCategory = YES;
  [bar addArrangedSubview:title];
#if TARGET_OS_IOS && !TARGET_OS_TV
  self.importImage = [UIButton buttonWithType:UIButtonTypeSystem];
  [self.importImage setTitle:@"Import .hsqs" forState:UIControlStateNormal];
  UIButtonConfiguration *importStyle = UIButtonConfiguration.filledButtonConfiguration;
  importStyle.image = [UIImage systemImageNamed:@"plus"];
  importStyle.imagePadding = 6;
  self.importImage.configuration = importStyle;
  self.importImage.accessibilityIdentifier = @"clover-import-image";
  [self.importImage addTarget:self
                       action:@selector(importAction)
             forControlEvents:UIControlEventPrimaryActionTriggered];
  [bar addArrangedSubview:self.importImage];
#endif
  [self.contents addArrangedSubview:bar];
  UILabel *subtitle = [UILabel new];
  subtitle.text = @"Your Classic collections";
  subtitle.textColor = UIColor.secondaryLabelColor;
  subtitle.font = [UIFont preferredFontForTextStyle:UIFontTextStyleHeadline];
  subtitle.adjustsFontForContentSizeCategory = YES;
  [self.contents addArrangedSubview:subtitle];
  self.cancel = [UIButton buttonWithType:UIButtonTypeSystem];
  [self.cancel setTitle:@"Cancel import" forState:UIControlStateNormal];
  [self.cancel addTarget:self
                  action:@selector(cancelAction)
        forControlEvents:UIControlEventPrimaryActionTriggered];
  self.status = [UILabel new];
  self.status.textColor = UIColor.secondaryLabelColor;
  self.status.numberOfLines = 2;
  self.status.font = [UIFont preferredFontForTextStyle:UIFontTextStyleSubheadline];
  self.status.accessibilityIdentifier = @"clover-profile-status";
  [self.contents addArrangedSubview:self.status];
  self.progress = [UIProgressView new];
  [self.contents addArrangedSubview:self.progress];
  [self.contents addArrangedSubview:self.cancel];
  self.table = [[UITableView alloc] initWithFrame:CGRectZero style:UITableViewStyleInsetGrouped];
  self.table.backgroundColor = UIColor.clearColor;
  self.table.dataSource = self;
  self.table.delegate = self;
  self.table.rowHeight = UITableViewAutomaticDimension;
  self.table.estimatedRowHeight = 80;
  self.table.accessibilityIdentifier = @"clover-profile-games";
  [self.contents addArrangedSubview:self.table];
  UILabel *warning = [UILabel new];
  warning.textColor = UIColor.lightGrayColor;
  warning.numberOfLines = 2;
  warning.font = [UIFont systemFontOfSize:12];
  warning.textColor = UIColor.secondaryLabelColor;
  warning.text = @"Import your own .hsqs Classic image from Files. Keep the original image; saves "
                 @"are stored on this device and are not backed up yet.";
  [self.contents addArrangedSubview:warning];
  self.collectionBack = [UIButton buttonWithType:UIButtonTypeSystem];
  [self.collectionBack setTitle:@"Collections" forState:UIControlStateNormal];
  self.collectionBack.tintColor = UIColor.whiteColor;
  self.collectionBack.backgroundColor = [UIColor colorWithWhite:0.05 alpha:0.85];
  self.collectionBack.layer.cornerRadius = 10;
  self.collectionBack.translatesAutoresizingMaskIntoConstraints = NO;
  self.collectionBack.hidden = YES;
  self.collectionBack.accessibilityIdentifier = @"clover-back-to-collections";
  [self.collectionBack addTarget:self
                          action:@selector(returnToLauncher)
                forControlEvents:UIControlEventPrimaryActionTriggered];
  [root addSubview:self.collectionBack];
  [NSLayoutConstraint activateConstraints:@[
    [self.collectionBack.trailingAnchor
        constraintEqualToAnchor:root.safeAreaLayoutGuide.trailingAnchor
                       constant:-8],
    [self.collectionBack.topAnchor constraintEqualToAnchor:root.safeAreaLayoutGuide.topAnchor
                                                  constant:8],
    [self.collectionBack.widthAnchor constraintEqualToConstant:116],
    [self.collectionBack.heightAnchor constraintEqualToConstant:44]
  ]];
  self.resume.hidden = YES;
  self.stop.hidden = YES;
  [self setForeground:YES];
  [self poll];
  return YES;
#endif
}
- (void)enableIfNeeded {
  [self configureTransfer:NO];
}
- (void)configureTransfer:(BOOL)renew {
  if (self.closed || !self.foreground || self.active || self.classicActive ||
      (self.enabled && !renew) || !_transfer)
    return;
  // Browsing/playing already imported profiles and local Files import must also
  // work offline; a foreground lease is distinct from an open LAN listener.
  clover_transfer_activate(_transfer);
#if !TARGET_OS_TV
  // Files-first on iOS: no LAN listener, pairing code or network permission.
  return;
#else
  NSString *host = localAddress();
  if (!host) {
    self.address.text =
        @"Connect to a local IPv4 network, then return to Clover to enable transfers.";
    self.codeLabel.text = @"— — —";
    self.qr.image = nil;
    return;
  }
  uint8_t random[16];
  if (SecRandomCopyBytes(kSecRandomDefault, sizeof(random), random) != errSecSuccess) {
    self.status.text = @"Cannot create a secure pairing code";
    return;
  }
  NSMutableString *token = [NSMutableString new];
  for (auto byte : random)
    [token appendFormat:@"%02x", byte];
  NSMutableString *code;
  do {
    code = [NSMutableString new];
    while (code.length < 6) {
      uint8_t digit;
      if (SecRandomCopyBytes(kSecRandomDefault, 1, &digit) != errSecSuccess) {
        self.status.text = @"Cannot create a pairing code";
        return;
      }
      if (digit < 250)
        [code appendFormat:@"%u", digit % 10];
    }
  } while ([code isEqual:self.pin]);
  uint16_t port = 0;
  char error[2048] = {};
  int result = renew ? clover_transfer_renew(_transfer, token.UTF8String, code.UTF8String, &port,
                                             error, sizeof(error))
                     : clover_transfer_enable(_transfer, token.UTF8String, code.UTF8String, &port,
                                              error, sizeof(error));
  if (result) {
    self.status.text = [NSString stringWithUTF8String:error];
    return;
  }
  self.enabled = YES;
  self.pin = code;
  self.connection = @{@"port" : @(port), @"code" : code};
  NSString *url = [NSString stringWithFormat:@"http://%@:%u/", host, port];
  NSString *grouped =
      [NSString stringWithFormat:@"%@ %@", [code substringToIndex:3], [code substringFromIndex:3]];
#if TARGET_OS_TV
  self.address.text = url;
  self.codeLabel.text = grouped;
  self.codeLabel.accessibilityValue = code;
#else
  self.address.text = [NSString stringWithFormat:@"Open %@\n6-digit code: %@", url, grouped];
#endif
  NSString *paired = [url stringByAppendingFormat:@"#%@", code];
  CIFilter *filter = [CIFilter filterWithName:@"CIQRCodeGenerator"];
  [filter setValue:[paired dataUsingEncoding:NSUTF8StringEncoding] forKey:@"inputMessage"];
  [filter setValue:@"M" forKey:@"inputCorrectionLevel"];
  CIImage *image = filter.outputImage;
  CIContext *context = [CIContext contextWithOptions:nil];
  CGImageRef pixels = image ? [context createCGImage:image fromRect:image.extent] : nullptr;
  if (pixels) {
    UIImage *code = [UIImage imageWithCGImage:pixels];
    CGImageRelease(pixels);
    UIGraphicsImageRenderer *renderer = [[UIGraphicsImageRenderer alloc]
        initWithSize:CGSizeMake(code.size.width + 8, code.size.height + 8)];
    self.qr.image = [renderer imageWithActions:^(UIGraphicsImageRendererContext *context) {
      [UIColor.whiteColor setFill];
      [context fillRect:CGRectMake(0, 0, code.size.width + 8, code.size.height + 8)];
      [code drawAtPoint:CGPointMake(4, 4)];
    }];
  }
  // Readable quiet zone around the QR, independent of the dark launcher theme.
  self.qr.backgroundColor = UIColor.whiteColor;
#endif
}
- (void)setForeground:(BOOL)foreground {
  _foreground = foreground;
  if (!foreground) {
    self.command = nil;
    self.openingProfile = nil;
    self.focusGeneration++;
    self.restoringFocus = NO;
    if (_transfer)
      clover_transfer_disable(_transfer);
    self.enabled = NO;
    self.connection = nil;
    self.qr.image = nil;
    self.codeLabel.text = @"— — —";
    UIViewController *picker = self.picker;
    self.picker = nil;
    [picker dismissViewControllerAnimated:NO completion:nil];
    self.address.text = @"Transfers paused — return to Clover for a new pairing code";
  } else if (self.classicActive) {
    if (!self.active)
      clover_transfer_activate(_transfer);
  } else
    [self enableIfNeeded];
}
- (void)setClassicActive:(BOOL)active profile:(NSString *)identity {
  uint64_t focusGeneration = ++self.focusGeneration;
  self.restoringFocus = !active;
#if TARGET_OS_TV
  // Unhiding UIKit can synchronously focus its default first row. Capture the
  // opened collection before that callback can overwrite the return target.
  NSString *returnProfile = self.classicProfile ?: self.focusedProfile;
#endif
  self.classicActive = active;
  self.classicProfile = active ? identity : nil;
  self.active = NO;
  self.openingProfile = nil;
  self.container.hidden = active;
  self.collectionBack.hidden = !active;
  [self.collectionBack setTitle:@"Collections" forState:UIControlStateNormal];
  clover_transfer_disable(_transfer);
  self.enabled = NO;
  self.connection = nil;
  if (active && self.foreground)
    clover_transfer_activate(_transfer);
  if (!active)
    [self enableIfNeeded];
#if TARGET_OS_TV
  SDL_SetHint(SDL_HINT_APPLE_TV_CONTROLLER_UI_EVENTS, active ? "0" : "1");
  self.focusedProfile = active ? identity : returnProfile;
  if (!active && self.profiles.count) {
    // Focus eligibility settles after UIKit unhides the table. Request the
    // captured row on the next main-queue turn, not inside the hide callback.
    __weak CloverProfileLauncher *weakSelf = self;
    dispatch_async(dispatch_get_main_queue(), ^{
      CloverProfileLauncher *self = weakSelf;
      if (!self || self.closed || !self.foreground || self.classicActive ||
          self.focusGeneration != focusGeneration)
        return;
      self.focusedProfile = returnProfile;
      [self.container layoutIfNeeded];
      [self moveCollectionFocus:0];
      self.restoringFocus = NO;
    });
  } else
    self.restoringFocus = NO;
  [self.presenter setNeedsFocusUpdate];
#else
  (void)focusGeneration;
  self.restoringFocus = NO;
#endif
}
- (void)activateFocusedCollection {
  if (self.closed || !self.foreground || self.active || self.classicActive || self.command ||
      self.openingProfile || [self.snapshot[@"busy"] boolValue])
    return;
  for (NSIndexPath *path in self.table.indexPathsForVisibleRows) {
    if ([self.table cellForRowAtIndexPath:path].focused) {
      [self tableView:self.table didSelectRowAtIndexPath:path];
      return;
    }
  }
}
- (void)moveCollectionFocus:(NSInteger)direction {
#if TARGET_OS_TV
  if (self.closed || !self.foreground || self.active || self.classicActive || self.openingProfile ||
      !self.profiles.count)
    return;
  if (direction) {
    self.focusGeneration++;
    self.restoringFocus = NO;
  }
  NSIndexPath *current = nil;
  for (NSIndexPath *path in self.table.indexPathsForVisibleRows)
    if ([self.table cellForRowAtIndexPath:path].focused) {
      current = path;
      break;
    }
  NSInteger index = direction && current
                        ? current.section
                        : [self indexPathForPreferredFocusedViewInTableView:self.table].section;
  if (direction && current)
    index = MAX(0, MIN((NSInteger)self.profiles.count - 1, index + direction));
  NSIndexPath *target = [NSIndexPath indexPathForRow:0 inSection:index];
  self.focusedProfile = self.profiles[index][@"id"];
  [self.table scrollToRowAtIndexPath:target
                    atScrollPosition:UITableViewScrollPositionNone
                            animated:NO];
  [self.table layoutIfNeeded];
  ((CloverCollectionRootView *)self.container).requestedFocus =
      [self.table cellForRowAtIndexPath:target];
  UIFocusSystem *system = [UIFocusSystem focusSystemForEnvironment:self.table];
  [system requestFocusUpdateToEnvironment:self.container];
  [system updateFocusIfNeeded];
#else
  (void)direction;
#endif
}
#if TARGET_OS_TV
- (void)selectFocusedCollection:(UITapGestureRecognizer *)recognizer {
  if (recognizer.state == UIGestureRecognizerStateEnded)
    [self activateFocusedCollection];
}
- (void)selectCollection:(UITapGestureRecognizer *)recognizer {
  if (recognizer.state != UIGestureRecognizerStateEnded)
    return;
  NSIndexPath *path = [self.table indexPathForRowAtPoint:[recognizer locationInView:self.table]];
  if (path)
    [self tableView:self.table didSelectRowAtIndexPath:path];
}
#endif
- (void)openClassicGame:(NSString *)identity {
  if (!self.classicActive || self.active || !self.foreground || self.closed)
    return;
  char error[2048] = {};
  if (clover_transfer_prepare(_transfer, self.classicProfile.UTF8String, identity.UTF8String, error,
                              sizeof(error)))
    self.command = @{
      @"action" : @"classic-error",
      @"error" : [NSString stringWithUTF8String:error] ?: @"Cannot prepare the selected game"
    };
}
- (void)setGameActive:(BOOL)active {
  if (_active == active)
    return;
  _active = active;
  if (self.classicActive) {
    self.container.hidden = YES;
    [self.collectionBack setTitle:active ? @"Classic menu" : @"Collections"
                         forState:UIControlStateNormal];
    if (active)
      clover_transfer_disable(_transfer);
    else if (self.foreground)
      clover_transfer_activate(_transfer);
    return;
  }
#if TARGET_OS_TV
  // Use SDL's public policy switch: UIKit focus in the launcher, gamepad input
  // in gameplay. SDL itself stays unmodified.
  SDL_SetHint(SDL_HINT_APPLE_TV_CONTROLLER_UI_EVENTS, active ? "0" : "1");
  [self.presenter setNeedsFocusUpdate];
#endif
  if (active) {
    if (_transfer)
      clover_transfer_disable(_transfer);
    self.enabled = NO;
    self.connection = nil;
  }
  self.container.backgroundColor =
      active ? UIColor.clearColor : [UIColor colorWithRed:0.063 green:0.09 blue:0.15 alpha:1];
  self.bottom.active = !active;
  self.gameHeight.active = active;
  // Keep the command bar available without a fullscreen UIKit hit surface.
  for (NSUInteger n = 1; n < self.contents.arrangedSubviews.count; ++n)
    self.contents.arrangedSubviews[n].hidden = active;
  self.resume.hidden = self.stop.hidden = !active;
  self.cancel.hidden = active;
  self.importImage.hidden = active;
  self.codeRefresh.hidden = active;
  self.container.userInteractionEnabled = YES;
  if (!active)
    [self enableIfNeeded];
}
- (void)poll {
  if (!_transfer || self.closed)
    return;
  std::vector<char> json(262144);
  char error[2048] = {};
  if (clover_transfer_snapshot(_transfer, json.data(), json.size(), error, sizeof(error))) {
    self.status.text = [NSString stringWithUTF8String:error];
    return;
  }
  NSDictionary *snapshot = decode(json.data());
  if (!snapshot)
    return;
  self.snapshot = snapshot;
#if TARGET_OS_TV
  NSString *focused = @"";
  for (NSIndexPath *path in self.table.indexPathsForVisibleRows) {
    if ([self.table cellForRowAtIndexPath:path].focused) {
      focused = self.profiles[path.section][@"id"];
      break;
    }
  }
  NSMutableDictionary *details = [snapshot mutableCopy];
  details[@"focused_collection"] = focused;
  self.snapshot = details;
  details[@"table_reloads"] = @(self.tableReloads);
  details[@"remembered_collection"] = self.focusedProfile ?: @"";
#endif
  if ([snapshot[@"busy"] boolValue])
    self.hostError = nil;
  self.status.text = self.hostError ?: snapshot[@"message"];
  if ([snapshot[@"limited"] boolValue])
    self.status.text = [self.status.text stringByAppendingString:@" — Profile list limit reached"];
  double total = [snapshot[@"total"] doubleValue];
  self.progress.progress = total > 0 ? [snapshot[@"received"] doubleValue] / total : 0;
  self.cancel.enabled = [snapshot[@"busy"] boolValue];
  self.codeRefresh.enabled = ![snapshot[@"busy"] boolValue] && !self.picker && self.foreground;
  self.importImage.enabled = ![snapshot[@"busy"] boolValue] && !self.picker;
  if (![self.profiles isEqual:snapshot[@"profiles"]]) {
    self.profiles = snapshot[@"profiles"];
    [self.table reloadData];
    self.tableReloads++;
#if TARGET_OS_TV
    [self updateTVLibrary];
    [self.presenter setNeedsFocusUpdate];
#endif
  }
  self.revision = [snapshot[@"revision"] unsignedLongLongValue];
  uint64_t remaining = [snapshot[@"pairing_seconds_remaining"] unsignedLongLongValue],
           locked = [snapshot[@"pairing_locked_seconds"] unsignedLongLongValue];
  self.pairStatus.text = locked ? [NSString stringWithFormat:@"Too many attempts · wait %llus",
                                                             (unsigned long long)locked]
                         : [snapshot[@"paired"] boolValue] ? @"Browser connected · ready to upload"
                         : remaining
                             ? [NSString stringWithFormat:@"Code expires in %llu:%02llu",
                                                          (unsigned long long)(remaining / 60),
                                                          (unsigned long long)(remaining % 60)]
                             : @"Code expired · select New code";
  if (!remaining && self.enabled)
    self.codeLabel.text = @"EXPIRED";
#if TARGET_OS_TV
  self.cancel.hidden = self.active || ![snapshot[@"busy"] boolValue];
  self.progress.hidden = ![snapshot[@"busy"] boolValue];
#else
  self.cancel.hidden = self.progress.hidden = ![snapshot[@"busy"] boolValue];
  BOOL idle = [snapshot[@"message"] hasPrefix:@"Ready to upload"];
  if (idle)
    self.status.text = @"Choose a collection to open its Classic menu.";
  self.status.hidden = idle && !self.hostError.length && !self.profiles.count;
  if (!self.profiles.count) {
    UILabel *empty = [UILabel new];
    empty.text = @"Your classics, ready to play\n\nTap Import .hsqs to choose a Classic image from "
                 @"Files.\nThen open its original Classic menu here.";
    empty.numberOfLines = 0;
    empty.textAlignment = NSTextAlignmentCenter;
    empty.textColor = UIColor.secondaryLabelColor;
    empty.font = [UIFont preferredFontForTextStyle:UIFontTextStyleBody];
    self.table.backgroundView = empty;
  } else
    self.table.backgroundView = nil;
#endif
}
- (void)newCodeAction {
  if (self.closed || self.active || !self.foreground || self.picker ||
      [self.snapshot[@"busy"] boolValue])
    return;
  [self configureTransfer:YES];
  [self poll];
}
- (NSInteger)numberOfSectionsInTableView:(UITableView *)tableView {
  (void)tableView;
  return self.profiles.count;
}
- (NSInteger)tableView:(UITableView *)tableView numberOfRowsInSection:(NSInteger)section {
  (void)tableView;
  (void)section;
  return 1;
}
- (NSString *)tableView:(UITableView *)tableView titleForHeaderInSection:(NSInteger)section {
  (void)tableView;
  (void)section;
  return nil;
}
- (UITableViewCell *)tableView:(UITableView *)tableView
         cellForRowAtIndexPath:(NSIndexPath *)indexPath {
#if TARGET_OS_TV
  CloverGameCell *cell = [tableView dequeueReusableCellWithIdentifier:@"card"]
                             ?: [[CloverGameCell alloc] initWithStyle:UITableViewCellStyleDefault
                                                      reuseIdentifier:@"card"];
  NSDictionary *profile = self.profiles[indexPath.section];
  cell.playable = YES;
  cell.badge.text = [profile[@"system"] uppercaseString];
  NSArray *parts = [profile[@"label"] componentsSeparatedByString:@"/"];
  NSString *region =
      parts.count > 1
          ? [parts[1] stringByTrimmingCharactersInSet:NSCharacterSet.whitespaceCharacterSet]
          : @"Classic";
  NSString *version =
      parts.count > 2
          ? [parts[2] stringByTrimmingCharactersInSet:NSCharacterSet.whitespaceCharacterSet]
          : @"Open Classic menu";
  cell.gameTitle.text = [NSString stringWithFormat:@"%@ Classic · %@", cell.badge.text, region];
  cell.detail.text = version;
  cell.accessibilityLabel = cell.gameTitle.text;
  cell.accessibilityValue = cell.detail.text;
  cell.accessibilityIdentifier = [@"clover-collection-" stringByAppendingString:profile[@"id"]];
  [cell applyFocus:cell.focused];
  return cell;
#else
  UITableViewCell *cell = [tableView dequeueReusableCellWithIdentifier:@"game"]
                              ?: [[UITableViewCell alloc] initWithStyle:UITableViewCellStyleSubtitle
                                                        reuseIdentifier:@"game"];
  NSDictionary *profile = self.profiles[indexPath.section];
  UIListContentConfiguration *content = [UIListContentConfiguration subtitleCellConfiguration];
  NSArray *parts = [profile[@"label"] componentsSeparatedByString:@"/"];
  NSString *region =
      parts.count > 1
          ? [parts[1] stringByTrimmingCharactersInSet:NSCharacterSet.whitespaceCharacterSet]
          : @"";
  content.text =
      [NSString stringWithFormat:@"%@ Classic · %@", [profile[@"system"] uppercaseString], region];
  content.secondaryText =
      parts.count > 2
          ? [parts[2] stringByTrimmingCharactersInSet:NSCharacterSet.whitespaceCharacterSet]
          : @"Open Classic menu";
  content.image = [UIImage systemImageNamed:@"gamecontroller"];
  content.imageProperties.tintColor = UIColor.systemBlueColor;
  cell.contentConfiguration = content;
  cell.accessoryType = UITableViewCellAccessoryDisclosureIndicator;
  cell.accessibilityIdentifier = [@"clover-collection-" stringByAppendingString:profile[@"id"]];
  return cell;
#endif
}
#if TARGET_OS_TV
- (UIView *)tableView:(UITableView *)tableView viewForHeaderInSection:(NSInteger)section {
  (void)tableView;
  NSDictionary *profile = self.profiles[section];
  NSArray *parts = [profile[@"label"] componentsSeparatedByString:@"/"];
  NSString *region =
      parts.count > 1
          ? [parts[1] stringByTrimmingCharactersInSet:NSCharacterSet.whitespaceCharacterSet]
          : @"Classic";
  UIView *view = [UIView new];
  UILabel *label = CloverLabel(
      [NSString stringWithFormat:@"%@  ·  %@", [profile[@"system"] uppercaseString], region], 18,
      UIFontWeightSemibold, CloverMuted());
  [view addSubview:label];
  CloverPin(label, view, 12);
  return view;
}
- (CGFloat)tableView:(UITableView *)tableView heightForHeaderInSection:(NSInteger)section {
  (void)tableView;
  (void)section;
  return 52;
}
- (NSIndexPath *)indexPathForPreferredFocusedViewInTableView:(UITableView *)tableView {
  (void)tableView;
  for (NSUInteger index = 0; index < self.profiles.count; index++)
    if ([self.profiles[index][@"id"] isEqual:self.focusedProfile])
      return [NSIndexPath indexPathForRow:0 inSection:index];
  return self.profiles.count ? [NSIndexPath indexPathForRow:0 inSection:0] : nil;
}
- (void)tableView:(UITableView *)tableView
     didUpdateFocusInContext:(UITableViewFocusUpdateContext *)context
    withAnimationCoordinator:(UIFocusAnimationCoordinator *)coordinator {
  (void)tableView;
  (void)coordinator;
  UIView *requested = ((CloverCollectionRootView *)self.container).requestedFocus;
  BOOL reached = !requested || context.nextFocusedView == requested ||
                 (requested == self.table && context.nextFocusedIndexPath);
  if (reached && !self.restoringFocus && !self.classicActive && !self.container.hidden &&
      !self.openingProfile && context.nextFocusedIndexPath &&
      context.nextFocusedIndexPath.section < self.profiles.count)
    self.focusedProfile = self.profiles[context.nextFocusedIndexPath.section][@"id"];
  // A one-shot request must not fight subsequent user movement until the next poll.
  if (reached && context.nextFocusedIndexPath)
    ((CloverCollectionRootView *)self.container).requestedFocus = nil;
}
#endif
- (void)tableView:(UITableView *)tableView didSelectRowAtIndexPath:(NSIndexPath *)indexPath {
  [tableView deselectRowAtIndexPath:indexPath animated:NO];
  if (self.closed || !self.foreground || self.active || self.classicActive || self.openingProfile ||
      [self.snapshot[@"busy"] boolValue] || self.command ||
      indexPath.section >= self.profiles.count)
    return;
  NSDictionary *profile = self.profiles[indexPath.section];
  self.hostError = nil;
  char error[2048] = {};
  if (clover_transfer_prepare_collection(_transfer, [profile[@"id"] UTF8String], error,
                                         sizeof(error)))
    self.hostError = [NSString stringWithUTF8String:error];
  else {
    self.openingProfile = profile[@"id"];
    self.focusedProfile = profile[@"id"];
  }
  [self poll];
}
- (void)resumeAction {
  if (self.active && !self.command)
    self.command = @{@"action" : @"resume"};
}
- (void)stopAction {
  if (self.active && !self.command)
    self.command = @{@"action" : @"stop"};
}
- (void)returnToLauncher {
  if (self.active)
    [self stopAction];
  else if (self.classicActive && !self.command)
    self.command = @{@"action" : @"close-classic"};
}
- (void)cancelAction {
  self.openingProfile = nil;
  if (_transfer)
    clover_transfer_cancel(_transfer);
  UIViewController *picker = self.picker;
  self.picker = nil;
  [picker dismissViewControllerAnimated:YES completion:nil];
}
#if TARGET_OS_IOS && !TARGET_OS_TV
- (void)importAction {
  if (self.closed || self.active || !self.foreground || self.picker ||
      [self.snapshot[@"busy"] boolValue] || self.presenter.presentedViewController)
    return;
  UIDocumentPickerViewController *picker =
      [[UIDocumentPickerViewController alloc] initForOpeningContentTypes:@[ UTTypeData ] asCopy:NO];
  picker.delegate = self;
  picker.allowsMultipleSelection = NO;
  self.picker = picker;
  [self.presenter presentViewController:picker animated:YES completion:nil];
}
- (void)documentPickerWasCancelled:(UIDocumentPickerViewController *)controller {
  if (controller != self.picker)
    return;
  self.picker = nil;
  [self poll];
}
- (void)documentPicker:(UIDocumentPickerViewController *)controller
    didPickDocumentsAtURLs:(NSArray<NSURL *> *)urls {
  if (controller != self.picker || self.closed || !self.foreground)
    return;
  self.picker = nil;
  if (urls.count != 1) {
    self.hostError = @"Choose one complete .hsqs filesystem image";
    [self poll];
    return;
  }
  [self selectLocalURL:urls.firstObject];
}
- (void)selectLocalURL:(NSURL *)source {
  if (self.closed || self.active || !self.foreground)
    return;
  if (![source.pathExtension.lowercaseString isEqual:@"hsqs"]) {
    self.hostError = @"Choose a complete .hsqs filesystem image";
    [self poll];
    return;
  }
  char error[2048] = {};
  void *lease = clover_transfer_local_begin(_transfer, error, sizeof(error));
  if (!lease) {
    self.hostError = [NSString stringWithUTF8String:error];
    [self poll];
    return;
  }
  __weak CloverProfileLauncher *weakSelf = self;
  [NSThread detachNewThreadWithBlock:^{
    @autoreleasepool {
      BOOL scoped = [source startAccessingSecurityScopedResource];
      NSFileCoordinator *coordinator = [[NSFileCoordinator alloc] initWithFilePresenter:nil];
      __block BOOL ran = NO;
      [coordinator
          coordinateReadingItemAtURL:source
                             options:NSFileCoordinatorReadingWithoutChanges
                               error:nil
                          byAccessor:^(NSURL *url) {
                            ran = YES;
                            int fd = open(url.fileSystemRepresentation,
                                          O_RDONLY | O_NOFOLLOW | O_NONBLOCK | O_CLOEXEC);
                            char diagnostic[2048] = {};
                            clover_transfer_local_run(lease, fd, diagnostic, sizeof(diagnostic));
                          }];
      if (!ran) {
        char diagnostic[2048] = {};
        clover_transfer_local_run(lease, -1, diagnostic, sizeof(diagnostic));
      }
      if (scoped)
        [source stopAccessingSecurityScopedResource];
      dispatch_async(dispatch_get_main_queue(), ^{
        CloverProfileLauncher *self = weakSelf;
        if (self && !self.closed)
          [self poll];
      });
    }
  }];
  [self poll];
}
#endif
- (void)probeAction:(NSDictionary *)command {
  NSString *action = command[@"action"];
  if ([action isEqual:@"play"])
    [self resumeAction];
  else if ([action isEqual:@"stop"])
    [self stopAction];
  else if ([action isEqual:@"new-code"])
    [self newCodeAction];
#if TARGET_OS_TV
  else if ([action isEqual:@"collection-burst"]) {
    for (SDL_Keycode key : {SDLK_DOWN, SDLK_UP, SDLK_DOWN, SDLK_UP}) {
      SDL_Event press = {};
      press.type = SDL_EVENT_KEY_DOWN;
      press.key.key = key;
      press.key.down = true;
      SDL_PushEvent(&press);
      press.type = SDL_EVENT_KEY_UP;
      press.key.down = false;
      SDL_PushEvent(&press);
    }
  } else if ([action isEqual:@"collection-up"] || [action isEqual:@"collection-down"]) {
    SDL_Event press = {};
    press.type = SDL_EVENT_KEY_DOWN;
    press.key.key = [action isEqual:@"collection-up"] ? SDLK_UP : SDLK_DOWN;
    press.key.down = true;
    SDL_PushEvent(&press);
    press.type = SDL_EVENT_KEY_UP;
    press.key.down = false;
    SDL_PushEvent(&press);
  } else if ([action isEqual:@"focus-library"] && self.profiles.count &&
             [self.profiles.firstObject[@"games"] count]) {
    [self.table layoutIfNeeded];
    ((CloverCollectionRootView *)self.container).requestedFocus = self.table;
    UIFocusSystem *system = [UIFocusSystem focusSystemForEnvironment:self.table];
    [system requestFocusUpdateToEnvironment:self.container];
    [system updateFocusIfNeeded];
    [self.presenter setNeedsFocusUpdate];
    [self.presenter updateFocusIfNeeded];
  }
#endif
#if TARGET_OS_IOS && !TARGET_OS_TV
  else if ([action isEqual:@"select-file"] && [command[@"path"] isKindOfClass:NSString.class])
    [self selectLocalURL:[NSURL fileURLWithPath:command[@"path"]]];
#endif
  else if ([action hasPrefix:@"menu-"] && self.classicActive && !self.active) {
    NSDictionary *keys =
        @{@"menu-confirm" : @3, @"menu-right" : @4, @"menu-left" : @5, @"menu-back" : @1};
    if (keys[action])
      self.command = @{@"action" : @"classic-key", @"key" : keys[action]};
  } else if ([action isEqual:@"open"] && !self.active && !self.classicActive &&
             ![self.snapshot[@"busy"] boolValue]) {
#if TARGET_OS_TV
    // Exercise production event routing, not a direct worker launch by ID.
    SDL_Event press = {};
    press.type = SDL_EVENT_KEY_DOWN;
    press.key.key = SDLK_RETURN;
    press.key.scancode = SDL_SCANCODE_RETURN;
    press.key.down = true;
    SDL_PushEvent(&press);
    press.type = SDL_EVENT_KEY_UP;
    press.key.down = false;
    SDL_PushEvent(&press);
#else
    for (NSUInteger index = 0; index < self.profiles.count; index++)
      if ([self.profiles[index][@"id"] isEqual:command[@"profile"]]) {
        [self tableView:self.table
            didSelectRowAtIndexPath:[NSIndexPath indexPathForRow:0 inSection:index]];
        break;
      }
#endif
  }
}
- (NSDictionary *)takeCommand {
  if (self.command) {
    NSDictionary *command = self.command;
    self.command = nil;
    return command;
  }
  if (!_transfer || self.active || !self.foreground)
    return nil;
  char json[8192] = {}, error[2048] = {};
  if (clover_transfer_take_launch(_transfer, json, sizeof(json), error, sizeof(error))) {
    self.status.text = [NSString stringWithUTF8String:error];
    return nil;
  }
  NSDictionary *command = decode(json);
  if ([command[@"action"] isEqual:@"open-classic"])
    self.classicProfile = command[@"profile"];
  return command;
}
- (void)didApply:(NSString *)action error:(NSString *)error {
  if ([action isEqual:@"open-classic"] || [action isEqual:@"classic-error"])
    self.openingProfile = nil;
  if (error.length) {
    self.hostError = error;
    self.status.text = error;
    return;
  }
  self.hostError = nil;
  if ([action isEqual:@"open-classic"])
    [self setClassicActive:YES profile:self.classicProfile];
  if ([action isEqual:@"close-classic"])
    [self setClassicActive:NO profile:nil];
  if ([action isEqual:@"load-profile"])
    [self setGameActive:YES];
  if ([action isEqual:@"stop"])
    [self setGameActive:NO];
}
- (void)shutdown {
  if (self.closed)
    return;
  self.closed = YES;
  if (_transfer) {
    clover_transfer_destroy(_transfer);
    _transfer = nullptr;
  }
  UIViewController *picker = self.picker;
  self.picker = nil;
  [picker dismissViewControllerAnimated:NO completion:nil];
  [self.container removeFromSuperview];
  self.container = nil;
  self.command = nil;
  [self.collectionBack removeFromSuperview];
  self.collectionBack = nil;
}
@end
