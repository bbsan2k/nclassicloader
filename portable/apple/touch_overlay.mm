#import "touch_overlay.h"
#import <UIKit/UIKit.h>
#include <TargetConditionals.h>
#include "touch_controls.h"
#include <array>

namespace {
static uint64_t nextSource = 1; // Allocated only on the main thread; never recycled.
struct Contact {
  uint64_t id = 0;
  uint16_t mask = 0;
};
} // namespace
@interface CloverControlBand : UIView
@property(nonatomic, weak) CloverTouchOverlay *owner;
@end
@interface CloverTouchOverlay () {
  std::array<Contact, 10> _contacts;
}
@property(nonatomic, strong) CloverControlBand *band;
@property(nonatomic, copy) void (^emit)(uint64_t, uint64_t, uint16_t, BOOL, BOOL);
@property(nonatomic) uint64_t source;
@property(nonatomic) uint64_t nextFinger;
@property(nonatomic, strong) NSMapTable<UITouch *, NSNumber *> *fingers;
@property(nonatomic) BOOL active;
@property(nonatomic) BOOL accepting;
@property(nonatomic) BOOL closed;
@property(nonatomic) CGRect previousFrame;
- (void)contact:(uint64_t)finger begin:(BOOL)begin ended:(BOOL)ended point:(CGPoint)point;
- (void)touches:(NSSet<UITouch *> *)touches begin:(BOOL)begin ended:(BOOL)ended;
- (uint16_t)held;
@end

@implementation CloverControlBand
- (BOOL)pointInside:(CGPoint)point withEvent:(UIEvent *)event {
  (void)event;
  return self.owner.active && self.owner.accepting && self.owner.supported &&
         clover::apple::TouchControls::captures(self.bounds.size.width, self.bounds.size.height,
                                                point.x, point.y);
}
- (void)touchesBegan:(NSSet<UITouch *> *)touches withEvent:(UIEvent *)event {
  (void)event;
  [self.owner touches:touches begin:YES ended:NO];
}
- (void)touchesMoved:(NSSet<UITouch *> *)touches withEvent:(UIEvent *)event {
  (void)event;
  [self.owner touches:touches begin:NO ended:NO];
}
- (void)touchesEnded:(NSSet<UITouch *> *)touches withEvent:(UIEvent *)event {
  (void)event;
  [self.owner touches:touches begin:NO ended:YES];
}
- (void)touchesCancelled:(NSSet<UITouch *> *)touches withEvent:(UIEvent *)event {
  (void)event;
  [self.owner touches:touches begin:NO ended:YES];
}
- (void)layoutSubviews {
  [super layoutSubviews];
  if (!CGRectEqualToRect(self.frame, self.owner.previousFrame)) {
    [self.owner cancelTouches];
    self.owner.previousFrame = self.frame;
  }
  [self setNeedsDisplay];
}
- (void)drawRect:(CGRect)rect {
  (void)rect;
  CGFloat width = self.bounds.size.width;
  if (!self.owner.supported) {
    [@"Use a controller at this window size" drawAtPoint:CGPointMake(12, 40)
                                          withAttributes:@{
                                            NSFontAttributeName : [UIFont systemFontOfSize:14],
                                            NSForegroundColorAttributeName : UIColor.whiteColor
                                          }];
    return;
  }
  using S = clover::apple::InputState;
  [[UIColor colorWithWhite:0.08 alpha:0.25] setFill];
  [[UIBezierPath bezierPathWithRoundedRect:CGRectMake(0, 4, 132, 132) cornerRadius:12] fill];
  uint16_t held = [self.owner held];
  NSArray<NSString *> *labels = @[ @"←", @"↑", @"→", @"↓", @"B", @"A", @"SELECT", @"START" ];
  const uint16_t masks[] = {S::Left, S::Up, S::Right, S::Down, S::B, S::A, S::Select, S::Start};
  CGRect boxes[] = {CGRectMake(0, 48, 44, 44),
                    CGRectMake(44, 4, 44, 44),
                    CGRectMake(88, 48, 44, 44),
                    CGRectMake(44, 92, 44, 44),
                    CGRectMake(width - 154, 64, 60, 60),
                    CGRectMake(width - 76, 20, 60, 60),
                    CGRectMake(width / 2 - 64, 100, 56, 44),
                    CGRectMake(width / 2 + 8, 100, 56, 44)};
  for (NSUInteger n = 0; n < 8; ++n) {
    UIColor *fill = (held & masks[n]) ? [UIColor colorWithRed:0.2 green:0.7 blue:1 alpha:0.8]
                                      : [UIColor colorWithWhite:0.12 alpha:0.65];
    [fill setFill];
    [[UIColor colorWithWhite:1 alpha:0.75] setStroke];
    UIBezierPath *path = (n == 4 || n == 5)
                             ? [UIBezierPath bezierPathWithOvalInRect:boxes[n]]
                             : [UIBezierPath bezierPathWithRoundedRect:boxes[n] cornerRadius:9];
    path.lineWidth = 1.5;
    [path fill];
    [path stroke];
    NSDictionary *attributes = @{
      NSFontAttributeName : [UIFont boldSystemFontOfSize:n >= 6 ? 10 : 24],
      NSForegroundColorAttributeName : UIColor.whiteColor
    };
    CGSize size = [labels[n] sizeWithAttributes:attributes];
    [labels[n] drawAtPoint:CGPointMake(CGRectGetMidX(boxes[n]) - size.width / 2,
                                       CGRectGetMidY(boxes[n]) - size.height / 2)
            withAttributes:attributes];
  }
}
@end

@implementation CloverTouchOverlay
- (void)install:(SDL_Window *)window emit:(void (^)(uint64_t, uint64_t, uint16_t, BOOL, BOOL))emit {
  if (!NSThread.isMainThread || nextSource == UINT64_MAX)
    return;
  self.source = nextSource++;
  self.nextFinger = 1;
  self.emit = emit;
  self.fingers = [NSMapTable strongToStrongObjectsMapTable];
#if TARGET_OS_IOS && !TARGET_OS_TV
  if (!window)
    return;
  UIWindow *native = (__bridge UIWindow *)SDL_GetPointerProperty(
      SDL_GetWindowProperties(window), SDL_PROP_WINDOW_UIKIT_WINDOW_POINTER, nullptr);
  UIView *root = native.rootViewController.view;
  if (!root)
    return;
  self.band = [CloverControlBand new];
  self.band.owner = self;
  self.band.multipleTouchEnabled = YES;
  self.band.backgroundColor = UIColor.clearColor;
  self.band.opaque = NO;
  self.band.translatesAutoresizingMaskIntoConstraints = NO;
  self.band.hidden = YES;
  self.band.accessibilityIdentifier = @"clover-touch-controller";
  [root addSubview:self.band];
  [NSLayoutConstraint activateConstraints:@[
    [self.band.leadingAnchor constraintEqualToAnchor:root.safeAreaLayoutGuide.leadingAnchor
                                            constant:8],
    [self.band.trailingAnchor constraintEqualToAnchor:root.safeAreaLayoutGuide.trailingAnchor
                                             constant:-8],
    [self.band.bottomAnchor constraintEqualToAnchor:root.safeAreaLayoutGuide.bottomAnchor
                                           constant:-8],
    [self.band.heightAnchor constraintEqualToConstant:150]
  ]];
#else
  (void)window;
#endif
}
- (BOOL)supported {
  return self.band && self.band.superview.safeAreaLayoutGuide.layoutFrame.size.height >= 240 &&
         clover::apple::TouchControls::supported(self.band.bounds.size.width,
                                                 self.band.bounds.size.height);
}
- (float)probeWidth {
  return self.band ? self.band.bounds.size.width : 640;
}
- (void)setAccepting:(BOOL)accepting {
  if (!accepting && _accepting)
    [self cancelTouches];
  _accepting = accepting;
}
- (void)setGameActive:(BOOL)active {
  if (self.closed)
    return;
  if (!active && self.active)
    [self cancelTouches];
  self.active = active;
  self.band.hidden = !active;
}
- (uint16_t)held {
  uint16_t mask = 0;
  for (const auto &contact : _contacts)
    mask |= contact.mask;
  return mask;
}
- (void)contact:(uint64_t)finger begin:(BOOL)begin ended:(BOOL)ended point:(CGPoint)point {
  if (self.closed || !self.active || !self.accepting || !self.emit || !finger)
    return;
  Contact *slot = nullptr;
  for (auto &contact : _contacts)
    if (contact.id == finger)
      slot = &contact;
  if (!slot && begin)
    for (auto &contact : _contacts)
      if (!contact.id) {
        slot = &contact;
        break;
      }
  if (!slot)
    return; // Moves after cancellation never recreate a contact.
  float width = self.band ? self.band.bounds.size.width : 640;
  float height = self.band ? self.band.bounds.size.height : 150;
  uint16_t mask = ended ? 0 : clover::apple::TouchControls::hit(width, height, point.x, point.y);
  BOOL press = !ended && mask && slot->mask == 0;
  // Emitting may service pending lifecycle events synchronously and cancel all
  // contacts. Publish first so that cancellation cannot be undone on return.
  if (ended)
    *slot = {};
  else
    *slot = {finger, mask};
  self.emit(self.source, finger, mask, !ended, press);
  [self.band setNeedsDisplay];
}
- (void)touches:(NSSet<UITouch *> *)touches begin:(BOOL)begin ended:(BOOL)ended {
  if (!self.accepting || !self.active || self.closed)
    return;
  unsigned count = 0;
  for (UITouch *touch in touches) {
    if (++count > 10)
      break;
    NSNumber *token = [self.fingers objectForKey:touch];
    if (!token && begin && self.fingers.count < 10 && self.nextFinger < UINT64_MAX) {
      token = @(self.nextFinger++);
      [self.fingers setObject:token forKey:touch];
    }
    if (!token)
      continue;
    [self contact:token.unsignedLongLongValue
            begin:begin
            ended:ended
            point:[touch locationInView:self.band]];
    if (ended)
      [self.fingers removeObjectForKey:touch];
  }
}
- (void)probeContact:(uint64_t)finger phase:(NSString *)phase x:(float)x y:(float)y {
  [self contact:finger
          begin:[phase isEqual:@"begin"]
          ended:[phase isEqual:@"end"]
          point:CGPointMake(x, y)];
}
- (void)cancelTouches {
  for (auto &contact : _contacts)
    if (contact.id) {
      if (self.emit)
        self.emit(self.source, contact.id, 0, NO, NO);
      contact = {};
    }
  [self.fingers removeAllObjects];
  [self.band setNeedsDisplay];
}
- (void)shutdown {
  [self cancelTouches];
  self.closed = YES;
  self.emit = nil;
  [self.band removeFromSuperview];
  self.band = nil;
}
@end
