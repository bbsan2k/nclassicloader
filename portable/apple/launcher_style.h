#pragma once
#import <UIKit/UIKit.h>

// Native, content-free launcher styling. No extracted console assets required.
static UIColor *CloverInk() {
  return [UIColor colorWithRed:0.91 green:0.94 blue:0.98 alpha:1];
}
static UIColor *CloverMuted() {
  return [UIColor colorWithRed:0.61 green:0.68 blue:0.77 alpha:1];
}
static UIColor *CloverMint() {
  return [UIColor colorWithRed:0.48 green:0.94 blue:0.75 alpha:1];
}
static UIColor *CloverPanel() {
  return [UIColor colorWithRed:0.075 green:0.105 blue:0.16 alpha:1];
}
static UILabel *CloverLabel(NSString *text, CGFloat size, UIFontWeight weight, UIColor *color) {
  UILabel *label = [UILabel new];
  label.text = text;
  label.font = [UIFont systemFontOfSize:size weight:weight];
  label.textColor = color;
  label.numberOfLines = 0;
  return label;
}
static UIStackView *CloverColumn(CGFloat spacing) {
  UIStackView *stack = [UIStackView new];
  stack.axis = UILayoutConstraintAxisVertical;
  stack.spacing = spacing;
  return stack;
}
static void CloverPin(UIView *child, UIView *parent, CGFloat inset) {
  child.translatesAutoresizingMaskIntoConstraints = NO;
  [NSLayoutConstraint activateConstraints:@[
    [child.leadingAnchor constraintEqualToAnchor:parent.leadingAnchor constant:inset],
    [child.trailingAnchor constraintEqualToAnchor:parent.trailingAnchor constant:-inset],
    [child.topAnchor constraintEqualToAnchor:parent.topAnchor constant:inset],
    [child.bottomAnchor constraintEqualToAnchor:parent.bottomAnchor constant:-inset]
  ]];
}
@interface CloverCollectionRootView : UIView
@property(nonatomic, weak) UIView *requestedFocus;
@end
@implementation CloverCollectionRootView
- (NSArray<id<UIFocusEnvironment>> *)preferredFocusEnvironments {
  return self.requestedFocus ? @[ self.requestedFocus ] : [super preferredFocusEnvironments];
}
@end

@interface CloverGameCell : UITableViewCell
@property(nonatomic, strong) UIView *card;
@property(nonatomic, strong) UILabel *gameTitle;
@property(nonatomic, strong) UILabel *detail;
@property(nonatomic, strong) UILabel *badge;
@property(nonatomic) BOOL playable;
- (void)applyFocus:(BOOL)focused;
@end
@implementation CloverGameCell
- (instancetype)initWithStyle:(UITableViewCellStyle)style reuseIdentifier:(NSString *)identifier {
  if ((self = [super initWithStyle:style reuseIdentifier:identifier])) {
    self.backgroundColor = UIColor.clearColor;
    self.selectionStyle = UITableViewCellSelectionStyleNone;
    self.focusStyle = UITableViewCellFocusStyleCustom;
    self.clipsToBounds = NO;
    self.contentView.clipsToBounds = NO;
    self.card = [UIView new];
    self.card.layer.cornerRadius = 18;
    self.card.layer.shadowRadius = 14;
    self.card.layer.shadowOffset = CGSizeMake(0, 6);
    self.card.layer.shadowColor = UIColor.blackColor.CGColor;
    [self.contentView addSubview:self.card];
    self.card.translatesAutoresizingMaskIntoConstraints = NO;
    [NSLayoutConstraint activateConstraints:@[
      [self.card.leadingAnchor constraintEqualToAnchor:self.contentView.leadingAnchor constant:12],
      [self.card.trailingAnchor constraintEqualToAnchor:self.contentView.trailingAnchor
                                               constant:-16],
      [self.card.topAnchor constraintEqualToAnchor:self.contentView.topAnchor constant:8],
      [self.card.bottomAnchor constraintEqualToAnchor:self.contentView.bottomAnchor constant:-8]
    ]];
    UIStackView *row = [UIStackView new];
    row.spacing = 22;
    row.alignment = UIStackViewAlignmentCenter;
    self.badge = CloverLabel(@"", 18, UIFontWeightBold, CloverMint());
    self.badge.textAlignment = NSTextAlignmentCenter;
    [self.badge.widthAnchor constraintEqualToConstant:70].active = YES;
    UIStackView *titles = CloverColumn(6);
    self.gameTitle = CloverLabel(@"", 30, UIFontWeightSemibold, CloverInk());
    self.gameTitle.numberOfLines = 1;
    self.detail = CloverLabel(@"", 18, UIFontWeightRegular, CloverMuted());
    self.detail.numberOfLines = 1;
    [titles addArrangedSubview:self.gameTitle];
    [titles addArrangedSubview:self.detail];
    [row addArrangedSubview:self.badge];
    [row addArrangedSubview:titles];
    [self.card addSubview:row];
    CloverPin(row, self.card, 20);
    [self applyFocus:NO];
  }
  return self;
}
- (void)applyFocus:(BOOL)focused {
  self.card.backgroundColor = focused ? CloverMint() : CloverPanel();
  self.card.transform =
      focused ? CGAffineTransformMakeScale(1.025, 1.025) : CGAffineTransformIdentity;
  self.card.layer.shadowOpacity = focused ? 0.35 : 0;
  UIColor *dark = [UIColor colorWithRed:0.035 green:0.10 blue:0.09 alpha:1];
  self.gameTitle.textColor = focused ? dark : CloverInk();
  self.detail.textColor = focused ? dark : CloverMuted();
  self.badge.textColor = focused ? dark : CloverMint();
  self.gameTitle.alpha = self.playable ? 1 : 0.6;
  self.badge.alpha = self.playable ? 1 : 0.6;
}
- (void)prepareForReuse {
  [super prepareForReuse];
  [self applyFocus:NO];
}
- (void)didUpdateFocusInContext:(UIFocusUpdateContext *)context
       withAnimationCoordinator:(UIFocusAnimationCoordinator *)coordinator {
  [super didUpdateFocusInContext:context withAnimationCoordinator:coordinator];
  [coordinator
      addCoordinatedAnimations:^{
        [self applyFocus:self.focused];
      }
                    completion:nil];
}
@end
