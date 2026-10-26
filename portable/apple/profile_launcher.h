#pragma once
#import <Foundation/Foundation.h>
#include <SDL3/SDL.h>

// Main-thread UIKit leaf. The opaque transfer service owns only worker state.
@interface CloverProfileLauncher : NSObject
- (BOOL)install:(SDL_Window *)window error:(NSString **)error;
- (void)setForeground:(BOOL)foreground;
- (void)setGameActive:(BOOL)active;
- (void)setClassicActive:(BOOL)active profile:(NSString *)identity;
- (void)openClassicGame:(NSString *)identity;
- (void)activateFocusedCollection;
- (void)moveCollectionFocus:(NSInteger)direction;
- (void)poll;
- (NSDictionary *)takeCommand;
- (void)didApply:(NSString *)action error:(NSString *)error;
- (void)shutdown;
- (void)returnToLauncher;
@property(nonatomic, readonly) NSDictionary *snapshot;
// Used only by the explicit, run-tagged simulator probe, never a web endpoint.
@property(nonatomic, readonly) NSDictionary *connection;
- (void)probeAction:(NSDictionary *)command;
@end
