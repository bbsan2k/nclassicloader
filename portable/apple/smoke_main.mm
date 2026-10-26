// App-hosted generated-ROM tests. No imported games or system firmware are bundled.
#import <UIKit/UIKit.h>
#include "tests/smoke_entry.h"

@interface CloverSmokeSceneDelegate : UIResponder <UIWindowSceneDelegate>
@property(nonatomic, strong) UIWindow *window;
@property(nonatomic, strong) UILabel *status;
@end

@implementation CloverSmokeSceneDelegate
- (void)scene:(UIScene *)scene
    willConnectToSession:(UISceneSession *)session
                 options:(UISceneConnectionOptions *)options {
  (void)session;
  (void)options;
  if (![scene isKindOfClass:UIWindowScene.class])
    return;
  self.window = [[UIWindow alloc] initWithWindowScene:(UIWindowScene *)scene];
  UIViewController *controller = [UIViewController new];
  controller.view.backgroundColor = UIColor.blackColor;
  self.status = [[UILabel alloc] initWithFrame:controller.view.bounds];
  self.status.autoresizingMask = UIViewAutoresizingFlexibleWidth | UIViewAutoresizingFlexibleHeight;
  self.status.numberOfLines = 0;
  self.status.textAlignment = NSTextAlignmentCenter;
  self.status.textColor = UIColor.whiteColor;
  self.status.text = @"Clover: running generated NES/SNES tests…";
  self.status.accessibilityIdentifier = @"clover-core-smoke-status";
  [controller.view addSubview:self.status];
  self.window.rootViewController = controller;
  [self.window makeKeyAndVisible];
  // A real OS thread owns each entire core lifetime; no dispatch-queue affinity assumption.
  [NSThread detachNewThreadSelector:@selector(runCoreChecks) toTarget:self withObject:nil];
}

- (void)runCoreChecks {
  @autoreleasepool {
    NSFileManager *files = NSFileManager.defaultManager;
    NSURL *cache =
        [files URLsForDirectory:NSCachesDirectory inDomains:NSUserDomainMask].firstObject;
    NSURL *root =
        [cache URLByAppendingPathComponent:[@"clover-core-smoke-"
                                               stringByAppendingString:NSUUID.UUID.UUIDString]
                               isDirectory:YES];
    NSError *error = nil;
    BOOL ready = cache && [files createDirectoryAtURL:root
                              withIntermediateDirectories:YES
                                               attributes:nil
                                                    error:&error];
    NSMutableArray *checks = [NSMutableArray new];
    if (ready) {
      char message[2048] = {};
      int nes = clover_nes_smoke(root.fileSystemRepresentation, message, sizeof(message));
      [checks addObject:@{
        @"system" : @"nes",
        @"passed" : @(nes == 0),
        @"error" : [NSString stringWithUTF8String:message] ?: @"Non-UTF8 diagnostic"
      }];
      message[0] = 0;
      int snes = clover_snes_smoke(root.fileSystemRepresentation, message, sizeof(message));
      [checks addObject:@{
        @"system" : @"snes",
        @"passed" : @(snes == 0),
        @"error" : [NSString stringWithUTF8String:message] ?: @"Non-UTF8 diagnostic"
      }];
    }
    BOOL passed = ready && checks.count == 2;
    for (NSDictionary *check in checks)
      passed = passed && [check[@"passed"] boolValue];
    NSString *runID = NSProcessInfo.processInfo.environment[@"CLOVER_SMOKE_RUN_ID"] ?: @"manual";
    NSDictionary *report = @{
      @"schema" : @1,
      @"platform" : @CLOVER_APPLE_PLATFORM,
      @"run_id" : runID,
      @"passed" : @(passed),
      @"checks" : checks,
      @"artifacts" : root.path ?: @"",
      @"error" : error.localizedDescription ?: @""
    };
    NSData *json = [NSJSONSerialization dataWithJSONObject:report
                                                   options:NSJSONWritingPrettyPrinted
                                                     error:&error];
    NSURL *receipt = [cache URLByAppendingPathComponent:@"clover-smoke-result.json"];
    BOOL recorded =
        json && receipt && [json writeToURL:receipt options:NSDataWritingAtomic error:&error];
    NSLog(@"Clover core smoke: %@; receipt=%@; error=%@", passed ? @"PASS" : @"FAIL", receipt,
          error);
    dispatch_async(dispatch_get_main_queue(), ^{
      self.status.text =
          recorded && passed
              ? @"PASS: generated NES + SNES core checks"
              : [NSString stringWithFormat:@"FAIL: %@", error.localizedDescription
                                                            ?: @"See smoke receipt / console"];
      self.status.textColor = recorded && passed ? UIColor.greenColor : UIColor.redColor;
    });
  }
}
@end

@interface CloverSmokeDelegate : UIResponder <UIApplicationDelegate>
@end
@implementation CloverSmokeDelegate
- (UISceneConfiguration *)application:(UIApplication *)application
    configurationForConnectingSceneSession:(UISceneSession *)session
                                   options:(UISceneConnectionOptions *)options {
  (void)application;
  (void)options;
  UISceneConfiguration *configuration =
      [[UISceneConfiguration alloc] initWithName:@"Clover Core Smoke" sessionRole:session.role];
  configuration.delegateClass = CloverSmokeSceneDelegate.class;
  return configuration;
}
@end

int main(int argc, char *argv[]) {
  @autoreleasepool {
    return UIApplicationMain(argc, argv, nil, NSStringFromClass(CloverSmokeDelegate.class));
  }
}
