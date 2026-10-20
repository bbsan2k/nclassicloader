#pragma once

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct CloverAres CloverAres;

CloverAres* clover_ares_create(void);
void clover_ares_destroy(CloverAres* core);

// All strings are UTF-8. coprocessor_firmware may be empty; system_firmware
// must contain the 64-byte ares SMP IPL named ipl.rom.
int clover_ares_load(CloverAres* core, const char* rom, const char* saves,
                     const char* coprocessor_firmware, const char* system_firmware);
int clover_ares_run_frame(CloverAres* core, uint16_t buttons);
void clover_ares_unload(CloverAres* core);
int clover_ares_loaded(const CloverAres* core);

// Returned buffers are borrowed and remain valid until the next core operation.
// Pixel words are 0xAARRGGBB; audio is interleaved float stereo.
const uint32_t* clover_ares_frame(const CloverAres* core, size_t* pixel_count,
                                  uint32_t* width, uint32_t* height);
const float* clover_ares_audio(const CloverAres* core, size_t* float_count);
uint32_t clover_ares_audio_rate(const CloverAres* core);

int clover_ares_save_state(CloverAres* core, const char* path);
int clover_ares_load_state(CloverAres* core, const char* path);
const char* clover_ares_error(const CloverAres* core);

#ifdef __cplusplus
}
#endif
