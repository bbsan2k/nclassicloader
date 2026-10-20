#import <AppKit/AppKit.h>
#include "native_menu.hpp"

@interface CloverMenuActions : NSObject
@property(nonatomic) Uint32 event;
- (void)command:(NSMenuItem*)sender;
@end
@implementation CloverMenuActions
- (void)command:(NSMenuItem*)sender {
  SDL_Event message{};message.type=self.event;message.user.code=int(sender.tag);
  SDL_PushEvent(&message);
}
@end

namespace clover::ui {
void installNativeMenu(bool inLauncher) {
  [NSApplication sharedApplication];
  static CloverMenuActions* actions=[[CloverMenuActions alloc] init];
  actions.event=nativeMenuEvent();
  NSMenu* bar=[[NSMenu alloc] initWithTitle:@""];
  auto menu=[&](NSString* title) {
    NSMenuItem* item=[[NSMenuItem alloc] initWithTitle:title action:nil keyEquivalent:@""];
    NSMenu* submenu=[[NSMenu alloc] initWithTitle:title];item.submenu=submenu;[bar addItem:item];return submenu;
  };
  auto command=[&](NSMenu* parent,NSString* title,NSString* key,MenuCommand code,NSEventModifierFlags modifiers=NSEventModifierFlagCommand,bool enabled=true) {
    NSMenuItem* item=[[NSMenuItem alloc] initWithTitle:title action:@selector(command:) keyEquivalent:key];
    item.target=actions;item.tag=int(code);item.keyEquivalentModifierMask=modifiers;item.enabled=enabled;[parent addItem:item];
  };
  auto app=menu(@"Classic Menu");app.autoenablesItems=NO;
  NSMenuItem* about=[[NSMenuItem alloc] initWithTitle:@"About Classic Menu" action:@selector(orderFrontStandardAboutPanel:) keyEquivalent:@""];[app addItem:about];
  [app addItem:[NSMenuItem separatorItem]];
  NSMenuItem* hide=[[NSMenuItem alloc] initWithTitle:@"Hide Classic Menu" action:@selector(hide:) keyEquivalent:@"h"];[app addItem:hide];
  NSMenuItem* others=[[NSMenuItem alloc] initWithTitle:@"Hide Others" action:@selector(hideOtherApplications:) keyEquivalent:@"h"];others.keyEquivalentModifierMask=NSEventModifierFlagCommand|NSEventModifierFlagOption;[app addItem:others];
  NSMenuItem* show=[[NSMenuItem alloc] initWithTitle:@"Show All" action:@selector(unhideAllApplications:) keyEquivalent:@""];[app addItem:show];
  [app addItem:[NSMenuItem separatorItem]];
  command(app,@"Quit Classic Menu",@"q",MenuCommand::quit);
  auto library=menu(@"Library");library.autoenablesItems=NO;
  command(library,@"Image Library / Switch Version…",@"i",MenuCommand::library,NSEventModifierFlagCommand|NSEventModifierFlagShift);
  command(library,@"Import Image…",@"o",MenuCommand::importImage);
  [library addItem:[NSMenuItem separatorItem]];
  command(library,@"Choose ROM for Selected Game…",@"o",MenuCommand::chooseROM,NSEventModifierFlagCommand|NSEventModifierFlagShift,!inLauncher);
  command(library,@"Choose Firmware Folder…",@"",MenuCommand::firmware,NSEventModifierFlagCommand,!inLauncher);
  auto view=menu(@"View");view.autoenablesItems=NO;
  command(view,@"Settings…",@",",MenuCommand::settings,NSEventModifierFlagCommand,!inLauncher);
  command(view,@"Next Sort Order",@"",MenuCommand::sort,NSEventModifierFlagCommand,!inLauncher);
  command(view,@"Toggle Menu Sound",@"",MenuCommand::sound,NSEventModifierFlagCommand,!inLauncher);
  [view addItem:[NSMenuItem separatorItem]];
  command(view,@"Toggle Full Screen",@"f",MenuCommand::fullscreen,NSEventModifierFlagCommand|NSEventModifierFlagControl,!inLauncher);
  auto window=menu(@"Window");
  NSMenuItem* minimize=[[NSMenuItem alloc] initWithTitle:@"Minimize" action:@selector(performMiniaturize:) keyEquivalent:@"m"];[window addItem:minimize];
  NSMenuItem* zoom=[[NSMenuItem alloc] initWithTitle:@"Zoom" action:@selector(performZoom:) keyEquivalent:@""];[window addItem:zoom];
  auto help=menu(@"Help");help.autoenablesItems=NO;
  command(help,@"Controls and Help",@"?",MenuCommand::help,NSEventModifierFlagCommand,!inLauncher);
  NSApp.mainMenu=bar;NSApp.windowsMenu=window;NSApp.helpMenu=help;
}

extern "C" Uint32 clover_native_menu_event() { return clover::ui::nativeMenuEvent(); }
extern "C" void clover_install_native_menu(int inLauncher) { clover::ui::installNativeMenu(inLauncher != 0); }
}
