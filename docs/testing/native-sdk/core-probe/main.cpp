#include "stdafx.h"
#include <cstdio>
#include <cstring>
#include <type_traits>
#include <thread>
#include "adapter.hpp"
#include "runtime.h"
#include <cstddef>
static_assert(sizeof(OpclDaFact) == 24 && offsetof(OpclDaFact, client_handle) == 0 &&
              offsetof(OpclDaFact, hresult) == 4 && offsetof(OpclDaFact, filetime_low) == 8 &&
              offsetof(OpclDaFact, filetime_high) == 12 && offsetof(OpclDaFact, i4) == 16 &&
              offsetof(OpclDaFact, quality) == 20 && offsetof(OpclDaFact, vartype) == 22 &&
              sizeof(OpclDaEnvelope) == 16 && offsetof(OpclDaEnvelope, master_error) == 12, "ABI layout");

// SDK's sample host supplies this C++ hook for Win32 resource lookup.
// The probe executable does not include the sample's message resource table.
HINSTANCE getInstanceHandle() { return ::GetModuleHandle(nullptr); }

static_assert(std::is_same<XML_Char, char>::value, "SDK expects narrow Expat strings");
using ExpectedExpatStart = void (XMLCALL *)(void*, const char*, const char**);
static_assert(std::is_same<XML_StartElementHandler, ExpectedExpatStart>::value,
              "SDK Expat callback ABI changed");

class CaptureGroup : public SODaCComGroupDefault {
public:
    DWORD read_count = 0, write_count = 0;
    SODaCItem* read_item = nullptr;
    SODaCItem* write_item = nullptr;
    HRESULT read_result = E_FAIL, write_result = E_FAIL;
    LONG read_value = 0;
    WORD read_quality = 0;
    FILETIME read_time{};
    void onReadAdvise(BYTE, DWORD, HRESULT, HRESULT, DWORD count, SODaCItem** items,
                      SOCmnVariant* values, WORD* qualities, SOCmnDateTime* times, HRESULT* errors) override {
        read_count = count;
        if (count) { read_item = items[0]; read_result = errors[0]; read_value = values[0].lVal; read_quality = qualities[0]; times[0].get(&read_time); }
    }
    void onWriteAdvise(DWORD, HRESULT, DWORD count, SODaCItem** items, HRESULT* errors) override {
        write_count = count;
        if (count) { write_item = items[0]; write_result = errors[0]; }
    }
};
class ProbeCallback : public SODaCDataCallback {
public:
    explicit ProbeCallback(SODaCComGroupImpl* group) : SODaCDataCallback(group) {
        // Offline test only: no real COM advise was established. The SDK
        // OnWriteComplete entry point ignores callbacks unless this is set.
        m_established = TRUE;
    }
    ~ProbeCallback() override { m_established = FALSE; }
    using SODaCDataCallback::handleReadAdvise;
};
static int adapter_test() {
    OpclDaGroup* group = new OpclDaGroup;
    SODaCItem* item = group->addItem(_T("N2.offline"));
    if (!item) { group->release(); return 20; }
    const OPCHANDLE valid = item->getClientHandle();
    OPCHANDLE handles[] = {0, valid};
    VARIANT values[2]; VariantInit(&values[0]); VariantInit(&values[1]);
    values[1].vt = VT_I4; values[1].lVal = -42;
    WORD qualities[] = {0, 0xc0};
    FILETIME times[] = {{0, 0}, {1, 0}};
    HRESULT errors[] = {E_INVALIDARG, static_cast<HRESULT>(7)};
    bool ok = valid != 0;
    {
        ProbeCallback callback(group);
        ok = ok && SUCCEEDED(callback.handleReadAdvise(0, 42, group->getOpcHandle(), S_OK, S_OK,
                                                       2, handles, values, qualities, times, errors));
        OpclDaFact fact{}; OpclDaEnvelope envelope{}; uint32_t count = 99;
        ok = ok && opcl_da_copy_facts(group, &envelope, &fact, 1, &count) == OPCL_DA_OK && count == 1 &&
             envelope.mode == 0 && envelope.transaction == 42 && envelope.master_quality == S_OK && envelope.master_error == S_OK &&
             fact.client_handle == valid && fact.hresult == 7 && fact.quality == 0xc0 &&
             fact.filetime_low == 1 && fact.filetime_high == 0 && fact.vartype == VT_I4 && fact.i4 == -42;
        std::printf("N2_FACT %u %u %d %d %u %d %u %u %d %u %u\n", envelope.mode, envelope.transaction, envelope.master_quality, envelope.master_error, fact.client_handle, fact.hresult, fact.filetime_low, fact.filetime_high, fact.i4, fact.quality, fact.vartype);
        count = 99;
        ok = ok && opcl_da_copy_facts(group, &envelope, &fact, 0, &count) == OPCL_DA_OVERFLOW && count == 0;
        count = 99;
        ok = ok && opcl_da_copy_facts(group, &envelope, nullptr, 1, &count) == OPCL_DA_INVALID && count == 0;
        ok = ok && opcl_da_copy_facts(nullptr, &envelope, &fact, 1, &count) == OPCL_DA_INVALID;
        ok = ok && opcl_da_copy_facts(group, &envelope, &fact, 1, nullptr) == OPCL_DA_INVALID;
        values[1].vt = VT_EMPTY;
        errors[1] = E_FAIL;
        ok = ok && SUCCEEDED(callback.handleReadAdvise(1, 42, group->getOpcHandle(), S_OK, S_OK,
                                                       2, handles, values, qualities, times, errors));
        ok = ok && opcl_da_copy_facts(group, &envelope, &fact, 1, &count) == OPCL_DA_OK && count == 1 &&
             fact.hresult == E_FAIL && fact.vartype == VT_EMPTY && fact.quality == 0xc0;
        SODaCItem* batch_items[OPCL_DA_MAX_FACTS]{};
        SOCmnVariant batch_values[OPCL_DA_MAX_FACTS];
        WORD batch_qualities[OPCL_DA_MAX_FACTS]{};
        SOCmnDateTime batch_times[OPCL_DA_MAX_FACTS];
        HRESULT batch_errors[OPCL_DA_MAX_FACTS]{};
        for (uint32_t i = 0; i < OPCL_DA_MAX_FACTS; ++i) {
            batch_items[i] = item;
            batch_values[i].vt = VT_I4;
            batch_values[i].lVal = static_cast<LONG>(i);
        }
        group->onReadAdvise(0, 77, S_OK, S_OK, OPCL_DA_MAX_FACTS, batch_items, batch_values,
                            batch_qualities, batch_times, batch_errors);
        OpclDaFact batch[OPCL_DA_MAX_FACTS]{};
        ok = ok && opcl_da_copy_facts(group, &envelope, batch, OPCL_DA_MAX_FACTS, &count) == OPCL_DA_OK &&
             count == OPCL_DA_MAX_FACTS && envelope.transaction == 77 && batch[63].i4 == 63;
        group->onReadAdvise(1, 99, E_FAIL, E_INVALIDARG, 0, nullptr, nullptr, nullptr, nullptr, nullptr);
        ok = ok && opcl_da_copy_facts(group, &envelope, nullptr, 0, &count) == OPCL_DA_OK && count == 0 &&
             envelope.mode == 1 && envelope.transaction == 99 && envelope.master_quality == E_FAIL && envelope.master_error == E_INVALIDARG;
        group->onReadAdvise(0, 0, S_OK, S_OK, OPCL_DA_MAX_FACTS + 1, nullptr, nullptr, nullptr, nullptr, nullptr);
        ok = ok && opcl_da_copy_facts(group, &envelope, &fact, 1, &count) == OPCL_DA_OVERFLOW && count == 0;
        group->onReadAdvise(0, 77, S_OK, S_OK, OPCL_DA_MAX_FACTS, batch_items, batch_values,
                            batch_qualities, batch_times, batch_errors);
        group->onReadAdvise(0, 0, S_OK, S_OK, 1, nullptr, nullptr, nullptr, nullptr, nullptr);
        ok = ok && opcl_da_copy_facts(group, &envelope, &fact, 1, &count) == OPCL_DA_INVALID && count == 0;
    }
    VariantClear(&values[0]); VariantClear(&values[1]);
    item->release(); group->release();
    std::printf("N2 adapter callback/ABI: %s\n", ok ? "PASS" : "FAIL");
    return ok ? 0 : 21;
}
static void phase(const char* name) {
    std::printf("phase: %s\n", name);
    std::fflush(stdout);
}
static int live_fixture() {
    const OpclDaGuid clsid = {0x7d4bd1b1, 0x7422, 0x48c0,
                              {0xad, 0x58, 0x60, 0x1b, 0x70, 0x14, 0x49, 0xc6}};
    OpclDaHost* host = nullptr;
    OpclDaSession* session = nullptr;
    OpclDaItem* item = nullptr;
    uint16_t type = 0; uint32_t rights = 0; int32_t write_result = E_FAIL;
    OpclDaReadI4 value{};
    HRESULT hr = opcl_da_host_create(&host);
    if (FAILED(hr)) { std::printf("host %08lx\n", hr); return 31; }
    hr = opcl_da_session_open(host, "OPCLook.NativeDaFixture.1", "localhost", &clsid, &session);
    bool ok = SUCCEEDED(hr);
    // This caller has no COM apartment. SDK task dispatch must perform all
    // synchronous OPC I/O on the SDK's per-server worker.
    if (ok) { std::thread caller([&] {
    ULONG_PTR token = 0;
    ok = CoGetContextToken(&token) == CO_E_NOTINITIALIZED;
    if (ok) hr = opcl_da_item_add(session, "Fixture.Value", &item, &type, &rights);
    if (SUCCEEDED(hr)) hr = opcl_da_read_i4(session, item, 0, &value);
    ok = ok && SUCCEEDED(hr) && SUCCEEDED(value.service_result) && SUCCEEDED(value.item_result) &&
              type == VT_I4 && (rights & OPC_WRITEABLE) && value.value == 42 &&
              (value.quality & 0xc0) == 0xc0 && (value.filetime_low || value.filetime_high);
    if (ok) {
        hr = opcl_da_write_i4(session, item, 77, &write_result);
        ok = SUCCEEDED(hr) && SUCCEEDED(write_result);
    }
    if (ok) {
        hr = opcl_da_read_i4(session, item, 0, &value);
        ok = SUCCEEDED(hr) && SUCCEEDED(value.service_result) && SUCCEEDED(value.item_result) && value.value == 77;
    }
    ok = ok && CoGetContextToken(&token) == CO_E_NOTINITIALIZED;
    }); caller.join(); }
    if (ok) {
        hr = opcl_da_item_remove(session, item); item = nullptr;
        ok = SUCCEEDED(hr);
    }
    if (ok) {
        hr = opcl_da_item_add(session, "Fixture.Value", &item, &type, &rights);
        if (SUCCEEDED(hr)) hr = opcl_da_read_i4(session, item, 0, &value);
        ok = SUCCEEDED(hr) && SUCCEEDED(value.service_result) && SUCCEEDED(value.item_result) && value.value == 77;
    }
    if (session) { hr = opcl_da_session_close(session); ok = ok && SUCCEEDED(hr); }
    hr = opcl_da_host_destroy(host); ok = ok && SUCCEEDED(hr);
    std::printf("live SDK C ABI fixture %s (hr=%08lx item=%08lx)\n", ok ? "PASS" : "FAIL", hr, write_result);
    return ok ? 0 : 32;
}
static int run_callbacks() {
    phase("create-group");
    CaptureGroup* group = new CaptureGroup;
    phase("add-item");
    SODaCItem* item = group->addItem(_T("N1.probe"));
    if (!item) { group->release(); return 11; }
    OPCHANDLE valid = item->getClientHandle();
    if (valid == 0) { item->release(); group->release(); return 12; }
    OPCHANDLE handles[] = {0, valid};
    VARIANT values[2]; VariantInit(&values[0]); VariantInit(&values[1]);
    values[0].vt = VT_I4; values[0].lVal = 11;
    values[1].vt = VT_I4; values[1].lVal = 22;
    WORD qualities[] = {0, 0xc0};
    FILETIME times[] = {{0, 0}, {1, 0}};
    // Distinguish ordinary S_OK and a positive non-S_OK HRESULT. The latter
    // is a test fixture for preserving raw bits, not a claim about a server.
    const HRESULT valid_statuses[] = {S_OK, static_cast<HRESULT>(7)};
    bool read_setup = true, read_correct = true, write_setup = true, write_correct = true;
    bool unchanged = true;
    {
        phase("create-offline-callback");
        ProbeCallback callback(group);
        for (HRESULT valid_status : valid_statuses) {
            HRESULT errors[] = {E_INVALIDARG, valid_status};
            for (BYTE mode = 0; mode < 2; ++mode) {
                phase(mode == 0 ? "read-complete" : "read-data-change");
                HRESULT call = callback.handleReadAdvise(mode, 42, group->getOpcHandle(), S_OK, S_OK,
                                                         2, handles, values, qualities, times, errors);
                read_setup = read_setup && SUCCEEDED(call) && group->read_count == 1 && group->read_item == item &&
                             group->read_value == 22 && group->read_quality == 0xc0 &&
                             group->read_time.dwLowDateTime == 1 && item->getReadResult() == valid_status;
                read_correct = read_correct && group->read_result == valid_status;
            }
            phase("write-complete");
            HRESULT call = callback.OnWriteComplete(42, group->getOpcHandle(), S_OK, 2, handles, errors);
            write_setup = write_setup && SUCCEEDED(call) && group->write_count == 1 &&
                          group->write_item == item && group->write_result == valid_status;
            write_correct = write_correct && item->getWriteResult() == valid_status;
            unchanged = unchanged && errors[0] == E_INVALIDARG && errors[1] == valid_status;
        }
    }
    std::printf("read_setup=%d read_correct=%d write_setup=%d write_correct=%d unchanged=%d\n",
                read_setup, read_correct, write_setup, write_correct, unchanged);
    std::fflush(stdout);
    phase("release-fixture");
    VariantClear(&values[0]); VariantClear(&values[1]);
    item->release();
    group->release();
    if (!read_setup || !write_setup || !unchanged) return 13;
    // Bit flags let main check baseline and fixed expectations after cleanup.
    return (read_correct ? 1 : 0) | (write_correct ? 2 : 0);
}
int main(int argc, char** argv) {
    phase("start");
    if (!getInstanceHandle()) return 9;
    if (argc == 2 && std::strcmp(argv[1], "--live") == 0) return live_fixture();
    const bool baseline = argc == 2 && std::strcmp(argv[1], "--expect-existing-defect") == 0;
    phase("common-init");
    initSOCmn();
    phase("common-init-complete");
    int result = 10;
    {
        phase("get-entry");
        SOCmnPointer<SODaCEntry> entry = getSODaCEntry();
        if (entry.isNotNull()) {
            phase("entry-init");
            if (entry->initialize(COINIT_MULTITHREADED)) {
                phase("callbacks");
                result = run_callbacks();
                if (!baseline && result == 3 && adapter_test() != 0) result = 21;
            }
            phase("disconnect");
            entry->disconnect();
            phase("terminate");
            if (!entry->terminate()) result = 14;
            // initialize/terminate run on this same thread. With no prior COM
            // initialization, the baseline leaks the MTA because its mode is 0.
            ULONG_PTR token = 0;
            const HRESULT apartment = CoGetContextToken(&token);
            const bool balanced = apartment == CO_E_NOTINITIALIZED;
            std::printf("MTA lifecycle balanced=%d\n", balanced);
            if (baseline ? balanced : !balanced) result = 22;
            if (!balanced) CoUninitialize(); // only balance the baseline leak
        }
        phase("entry-release");
        entry.release();
    }
    phase("common-exit");
    exitSOCmn();
    phase("done");
    if (result > 3) return result;
    return (baseline ? result == 0 : result == 3) ? 0 : 1;
}
