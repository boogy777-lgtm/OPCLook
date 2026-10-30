#pragma once
#include <stdint.h>

// Offline N2.1 ABI. The caller owns the group and must serialize access with
// SDK callbacks; no pointers from the SDK or VARIANT ownership cross this ABI.
#define OPCL_DA_MAX_FACTS 64u
#define OPCL_DA_OK 0u
#define OPCL_DA_INVALID 1u
#define OPCL_DA_OVERFLOW 2u

// mode 1 = SDK data change, 0 = SDK read complete.
typedef struct OpclDaEnvelope {
    uint32_t mode;
    uint32_t transaction;
    int32_t master_quality;
    int32_t master_error;
} OpclDaEnvelope;

typedef struct OpclDaFact {
    uint32_t client_handle;
    int32_t hresult;
    uint32_t filetime_low;
    uint32_t filetime_high;
    int32_t i4;
    uint16_t quality;
    uint16_t vartype;
} OpclDaFact;

#ifdef __cplusplus
extern "C" {
#endif
// Returns the most recent whole callback, or an error with *count = 0.
// group must be a live OpclDaGroup, created and owned by the SDK host.
uint32_t opcl_da_copy_facts(const void* group, OpclDaEnvelope* envelope, OpclDaFact* out, uint32_t capacity, uint32_t* count);
#ifdef __cplusplus
}
#endif
