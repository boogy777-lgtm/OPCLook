#include "stdafx.h"
#include "adapter.hpp"
#include <cstring>

void OpclDaGroup::onReadAdvise(BYTE mode, DWORD transaction, HRESULT master_quality, HRESULT master_error, DWORD n, SODaCItem** items,
                                SOCmnVariant* values, WORD* qualities, SOCmnDateTime* times, HRESULT* errors) {
    count = 0;
    envelope = {};
    result = OPCL_DA_INVALID;
    if (n > OPCL_DA_MAX_FACTS) { result = OPCL_DA_OVERFLOW; return; }
    if (n && (!items || !values || !qualities || !times || !errors)) return;
    for (DWORD i = 0; i < n; ++i) {
        if (!items[i] || !items[i]->getClientHandle()) return;
    }
    envelope.mode = mode;
    envelope.transaction = transaction;
    envelope.master_quality = static_cast<int32_t>(master_quality);
    envelope.master_error = static_cast<int32_t>(master_error);
    for (DWORD i = 0; i < n; ++i) {
        FILETIME ft{};
        times[i].get(&ft);
        OpclDaFact fact{};
        fact.client_handle = items[i]->getClientHandle();
        fact.hresult = static_cast<int32_t>(errors[i]);
        fact.filetime_low = ft.dwLowDateTime;
        fact.filetime_high = ft.dwHighDateTime;
        fact.quality = qualities[i];
        fact.vartype = values[i].vt;
        if (values[i].vt == VT_I4) fact.i4 = values[i].lVal;
        facts[i] = fact;
    }
    count = n;
    result = OPCL_DA_OK;
}

extern "C" uint32_t opcl_da_copy_facts(const void* opaque, OpclDaEnvelope* envelope, OpclDaFact* out, uint32_t capacity, uint32_t* count) {
    if (!count) return OPCL_DA_INVALID;
    *count = 0;
    if (!opaque || !envelope) return OPCL_DA_INVALID;
    *envelope = {};
    const OpclDaGroup* group = static_cast<const OpclDaGroup*>(opaque);
    if (group->result != OPCL_DA_OK) return group->result;
    if (capacity < group->count) return OPCL_DA_OVERFLOW;
    if (group->count && !out) return OPCL_DA_INVALID;
    if (group->count) std::memcpy(out, group->facts, group->count * sizeof(OpclDaFact));
    *envelope = group->envelope;
    *count = group->count;
    return OPCL_DA_OK;
}
