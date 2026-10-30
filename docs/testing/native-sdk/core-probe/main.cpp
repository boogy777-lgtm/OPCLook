#include "stdafx.h"
#include <cstdio>
#include <cstring>
#include <type_traits>

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
static void phase(const char* name) {
    std::printf("phase: %s\n", name);
    std::fflush(stdout);
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
                phase(mode == 0 ? "read-data-change" : "read-complete");
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
            }
            phase("disconnect");
            entry->disconnect();
            phase("terminate");
            if (!entry->terminate()) result = 14;
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
