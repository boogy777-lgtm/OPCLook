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

/* Scalar numeric payload uses low/high words to keep its ABI layout stable on
   Win32. Unsupported values (including arrays and BYREF) fail without
   manufacturing a value. Float and DATE use IEEE f64 bits. */
typedef struct OpclDaScalar {
    int32_t service_result, item_result;
    uint32_t filetime_low, filetime_high;
    uint32_t bits_low, bits_high;
    uint16_t quality, vartype;
    uint32_t text_len;
} OpclDaScalar;

#define OPCL_DA_VALUE_BYTES 65536u
typedef struct OpclDaPush {
    uint32_t registration_low, registration_high;
    int32_t master_quality;
    int32_t adapter_result;
    OpclDaScalar value;
} OpclDaPush;

#define OPCL_DA_MAX_BROWSE 256u
#define OPCL_DA_BROWSE_NAME_BYTES 1024u
#define OPCL_DA_BROWSE_ID_BYTES 4096u
typedef struct OpclDaBrowseRecord {
    uint32_t is_branch, name_len, node_id_len;
    char name[OPCL_DA_BROWSE_NAME_BYTES];
    char node_id[OPCL_DA_BROWSE_ID_BYTES];
} OpclDaBrowseRecord;
int32_t opcl_da_browse(OpclDaSession* session, const char* node_id, OpclDaBrowseRecord* out,
                       uint32_t capacity, uint32_t* count);

#define OPCL_DA_MAX_SERVERS 256u
#define OPCL_DA_PROGID_BYTES 1024u
#define OPCL_DA_DESCRIPTION_BYTES 2048u
typedef struct OpclDaServerRecord {
    OpclDaGuid clsid;
    uint32_t progid_len, description_len, flags;
    char progid[OPCL_DA_PROGID_BYTES];
    char description[OPCL_DA_DESCRIPTION_BYTES];
} OpclDaServerRecord;

/* kind 0 = local Windows class lookup; kind 1 = remote SDK OPCEnum. */
int32_t opcl_da_resolve(OpclDaHost* host, const char* node, const char* progid,
                        uint32_t kind, OpclDaGuid* out);
int32_t opcl_da_discover(OpclDaHost* host, const char* node,
                         OpclDaServerRecord* out, uint32_t capacity, uint32_t* count, uint32_t* fallback);
int32_t opcl_da_host_create(OpclDaHost** out);
/* Returns E_PENDING while sessions remain; leaves host alive in that case. */
int32_t opcl_da_host_destroy(OpclDaHost* host);
int32_t opcl_da_session_open(OpclDaHost* host, const char* name, const char* node,
                             const OpclDaGuid* clsid, uint32_t subscription, uint32_t update_rate_ms,
                             uint32_t registration_budget, uint32_t* revised_rate_ms, OpclDaSession** out);
int32_t opcl_da_session_close(OpclDaSession* session);
/* Uses the SDK server worker. On success, state is the raw OPCSERVERSTATE;
   zero also represents an absent status output and must not mean Running. */
int32_t opcl_da_test_status(OpclDaSession* session, int32_t* state);
int32_t opcl_da_item_add(OpclDaSession* session, const char* item_id, uint32_t registration_low,
                         uint32_t registration_high, OpclDaItem** out,
                         uint16_t* canonical_type, uint32_t* access_rights);
int32_t opcl_da_item_remove(OpclDaSession* session, OpclDaItem* item);
int32_t opcl_da_take_push(OpclDaSession* session, OpclDaPush* out, char* payload,
                          uint32_t capacity, uint32_t* count, uint32_t* overflow);
#ifdef OPCL_DA_TESTING
/* Test-only one-shot injection: state 0 idle, 1 armed, 2 consumed. */
int32_t opcl_da_test_arm_write_exception(void);
int32_t opcl_da_test_write_exception_state(void);
int32_t opcl_da_test_queue_bounds(void);
int32_t opcl_da_test_codec_bounds(void);
int32_t opcl_da_test_ephemeral_worker(uint32_t guarded);
#endif
int32_t opcl_da_read_i4(OpclDaSession* session, OpclDaItem* item, uint32_t max_age,
                        OpclDaReadI4* out);
int32_t opcl_da_read_scalar(OpclDaSession* session, OpclDaItem* item, uint32_t max_age,
                            OpclDaScalar* out, char* text, uint32_t text_capacity);
int32_t opcl_da_write_scalar(OpclDaSession* session, OpclDaItem* item, uint16_t vartype,
                             uint32_t bits_low, uint32_t bits_high, int32_t* item_result);
int32_t opcl_da_write_array(OpclDaSession* session, OpclDaItem* item, uint16_t vartype,
                            const uint8_t* data, uint32_t bytes, int32_t* item_result);
int32_t opcl_da_write_text(OpclDaSession* session, OpclDaItem* item, const char* utf8,
                           int32_t* item_result);
int32_t opcl_da_write_i4(OpclDaSession* session, OpclDaItem* item, int32_t value,
                         int32_t* item_result);
#ifdef __cplusplus
}
#endif
