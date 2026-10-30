#pragma once
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Opaque SDK objects. No COM interface or SDK-owned value crosses this ABI.
   Host create/destroy are called on the same thread. A session and its items
   have one serialized caller; distinct sessions may run on distinct threads.
   Host destroy must not race any other call. Handles are invalid immediately
   after removal/close; the eventual Rust owner must reject stale registrations. */
typedef struct OpclDaHost OpclDaHost;
typedef struct OpclDaSession OpclDaSession;
typedef struct OpclDaItem OpclDaItem;
typedef struct OpclDaGuid {
    uint32_t data1; uint16_t data2; uint16_t data3; uint8_t data4[8];
} OpclDaGuid;
typedef struct OpclDaReadI4 {
    int32_t service_result, item_result, value;
    uint32_t filetime_low, filetime_high;
    uint16_t quality, vartype;
} OpclDaReadI4;

int32_t opcl_da_host_create(OpclDaHost** out);
/* Returns E_PENDING while sessions remain; leaves host alive in that case. */
int32_t opcl_da_host_destroy(OpclDaHost* host);
int32_t opcl_da_session_open(OpclDaHost* host, const char* name, const char* node,
                             const OpclDaGuid* clsid, OpclDaSession** out);
int32_t opcl_da_session_close(OpclDaSession* session);
int32_t opcl_da_item_add(OpclDaSession* session, const char* item_id, OpclDaItem** out,
                         uint16_t* canonical_type, uint32_t* access_rights);
int32_t opcl_da_item_remove(OpclDaSession* session, OpclDaItem* item);
int32_t opcl_da_read_i4(OpclDaSession* session, OpclDaItem* item, uint32_t max_age,
                        OpclDaReadI4* out);
int32_t opcl_da_write_i4(OpclDaSession* session, OpclDaItem* item, int32_t value,
                         int32_t* item_result);
#ifdef __cplusplus
}
#endif
