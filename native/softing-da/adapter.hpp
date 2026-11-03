#pragma once
#include "adapter.h"

// Include after the pinned SDK headers. This is an offline integration slice;
// callback and snapshot must run on a serialized SDK host execution context.
class OpclDaGroup : public SODaCComGroupDefault {
public:
    OpclDaEnvelope envelope{};
    OpclDaFact facts[OPCL_DA_MAX_FACTS]{};
    uint32_t count = 0;
    uint32_t result = OPCL_DA_INVALID;
    void onReadAdvise(BYTE, DWORD, HRESULT, HRESULT, DWORD n, SODaCItem** items,
                      SOCmnVariant* values, WORD* qualities, SOCmnDateTime* times, HRESULT* errors) override;
};
