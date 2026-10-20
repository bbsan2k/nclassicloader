#import <AppKit/AppKit.h>
#include "macos_menu.hpp"
#include <iostream>
int main() {
  @autoreleasepool {
    if(!SDL_Init(SDL_INIT_EVENTS)) return 1;
    clover::ui::installNativeMenu();
    auto library=[NSApp.mainMenu itemWithTitle:@"Library"].submenu;
    auto item=[library itemWithTitle:@"Image Library / Switch Version…"];
    if(!item || !item.enabled || ![item.keyEquivalent isEqualToString:@"i"]) return 2;
    [NSApp sendAction:item.action to:item.target from:item];
    SDL_Event event{};bool received=false;
    while(SDL_PollEvent(&event)) if(event.type==clover::ui::nativeMenuEvent() && event.user.code==int(clover::ui::MenuCommand::library)) received=true;
    clover::ui::installNativeMenu(true, true);
    auto rom=[[NSApp.mainMenu itemWithTitle:@"Library"].submenu itemWithTitle:@"Choose ROM for Selected Game…"];
    bool disabled=rom && !rom.enabled;
    auto fullscreen=[[NSApp.mainMenu itemWithTitle:@"View"].submenu itemWithTitle:@"Toggle Full Screen"];
    bool fullscreenEnabled=fullscreen && fullscreen.enabled;
    SDL_Quit();
    if(!received || !disabled || !fullscreenEnabled) return 3;
    std::cout<<"Native menu labels, command dispatch and launcher availability passed\n";
  }
}
