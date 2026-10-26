#pragma once
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// Transfer handles are main-thread-owned; worker state contains no core/renderer.
void *clover_transfer_create(const char *root, char *error, size_t size);
int32_t clover_transfer_enable(void *service, const char *token, const char *code, uint16_t *port,
                               char *error, size_t size);
int32_t clover_transfer_renew(void *service, const char *token, const char *code, uint16_t *port,
                              char *error, size_t size);
int32_t clover_transfer_snapshot(void *service, char *json, size_t length, char *error,
                                 size_t size);
int32_t clover_transfer_prepare(void *service, const char *profile, const char *game, char *error,
                                size_t size);
int32_t clover_transfer_prepare_collection(void *service, const char *profile, char *error,
                                           size_t size);
int32_t clover_transfer_take_launch(void *service, char *json, size_t length, char *error,
                                    size_t size);
void clover_transfer_disable(void *service);
void clover_transfer_cancel(void *service);
void clover_transfer_destroy(void *service);
void clover_transfer_activate(void *service);
void *clover_transfer_local_begin(void *service, char *error, size_t size);
// Consumes lease and owned fd (or -1 for a failed/cancelled coordinated read).
int32_t clover_transfer_local_run(void *lease, int fd, char *error, size_t size);
int32_t clover_apple_profile_game_start(void *state, const char *rom, const char *saves,
                                        const char *firmware, char *error, size_t size);
int32_t clover_apple_classic_open(void *state, const char *assets, const char *data,
                                  const char *cache, const char *home, char *error, size_t size);
int32_t clover_apple_classic_close(void *state, char *error, size_t size);
int32_t clover_apple_classic_input(void *state, uint16_t mask, uint32_t key);
int32_t clover_apple_classic_point(void *state, uint32_t window, float x, float y);
int32_t clover_apple_classic_action(void *state, char *json, size_t size, char *error,
                                    size_t capacity);
int32_t clover_apple_classic_report(void *state, char *json, size_t size, char *error,
                                    size_t capacity);
int32_t clover_apple_classic_error(void *state, const char *message);

// A handle has one owner. All calls must be serialized on its creating OS thread.
int32_t clover_apple_preview_init(void **state, char *error, size_t error_size);
int32_t clover_apple_preview_iterate(void *state, char *error, size_t error_size);
// 1 background/pause, 2 foreground, 3 explicit resume,
// 4 audio interruption began (blocks resume), 5 ended (does not resume).
int32_t clover_apple_preview_event(void *state, uint32_t kind);
int32_t clover_apple_preview_probe(void *state, uint32_t *frames, uint64_t *checksum, char *error,
                                   size_t error_size);
void clover_apple_preview_quit(void *state);
int32_t clover_apple_game_start(void *state, const char *rom, const char *root, char *error,
                                size_t size);
int32_t clover_apple_game_buttons(void *state, uint16_t buttons);
int32_t clover_apple_game_report(void *state, char *json, size_t size, char *error,
                                 size_t error_size);
int32_t clover_apple_game_stop(void *state, char *error, size_t size);
// Thread-independent bounded import; no live core handle. JSON capacity >= 4096.
int32_t clover_apple_import_nes(const char *source, const char *library, char *json,
                                size_t json_size, char *error, size_t error_size);
int32_t clover_apple_library_scan(const char *library, char *json, size_t size, char *error,
                                  size_t error_size);
int32_t clover_apple_library_open(const char *library, const char *digest, char *json, size_t size,
                                  char *error, size_t error_size);
int32_t clover_apple_library_scene(void *state, const char *json, char *error, size_t size);
int32_t clover_apple_library_pick(void *state, uint64_t revision, uint32_t window, float x, float y,
                                  char *digest, size_t size, char *error, size_t error_size);
int32_t clover_apple_library_report(void *state, char *json, size_t size, char *error,
                                    size_t error_size);

#ifdef __cplusplus
}
#endif
