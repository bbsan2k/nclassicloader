#pragma once
#import <Foundation/Foundation.h>
#include <SDL3/SDL.h>

@interface CloverTouchOverlay : NSObject
- (void)install:(SDL_Window *)window
           emit:(void (^)(uint64_t source, uint64_t finger, uint16_t mask, BOOL down,
                          BOOL press))emit;
- (void)setGameActive:(BOOL)active;
- (void)setAccepting:(BOOL)accepting;
- (void)cancelTouches;
- (void)shutdown;
- (void)probeContact:(uint64_t)finger phase:(NSString *)phase x:(float)x y:(float)y;
@property(nonatomic, readonly) BOOL supported;
@property(nonatomic, readonly) float probeWidth;
@end
