#pragma once
#import <Foundation/Foundation.h>
#include <SDL3/SDL.h>

// All UI/command methods run on the main thread. The worker owns only staged
// bytes and a cancellation token, never a Rust host or ares session.
@interface CloverDocumentImport : NSObject
- (void)install:(SDL_Window *)window pause:(void (^)(void))pause;
- (NSDictionary *)takeCommand;
- (void)didApply:(NSString *)action error:(NSString *)error;
- (void)cancelForBackground;
- (void)shutdown;
- (void)refreshLibrary;
- (void)moveSelection:(NSInteger)delta;
- (void)selectDigest:(NSString *)digest;
- (void)openSelection;
// Explicitly injected selection for sandbox tests, not provider/UI selection proof.
- (void)probeSelect:(NSURL *)url;
- (void)probeCancel;
- (void)probeAction:(NSString *)action url:(NSURL *)url;
@property(nonatomic, readonly) NSDictionary *probeStatus;
@end
