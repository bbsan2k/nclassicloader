#ifndef CLOVER_SMOKE_ENTRY_H
#define CLOVER_SMOKE_ENTRY_H

#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

int clover_snes_smoke(const char* artifact_root, char* error, size_t error_size);
int clover_nes_smoke(const char* artifact_root, char* error, size_t error_size);

#ifdef __cplusplus
}
#endif

#endif
