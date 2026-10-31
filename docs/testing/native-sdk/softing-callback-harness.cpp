// Test-only shell around exact upstream source inserted below. No COM server,
// protocol implementation, or product code is created by this harness.
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <cstddef>
#include <vector>
using BYTE = std::uint8_t;
using DWORD = std::uint32_t;
using WORD = std::uint16_t;
using HRESULT = std::int32_t;
using OPCHANDLE = std::uint32_t;
struct VARIANT { int value; };
struct FILETIME { std::uint32_t low, high; };
struct SOCmnString {};
struct SOCmnVariant { int value = 0; SOCmnVariant& operator=(VARIANT* v) { value = v->value; return *this; } };
struct SOCmnDateTime { FILETIME value{}; SOCmnDateTime& operator=(FILETIME* v) { value = *v; return *this; } };
struct SODaCItem {
    HRESULT read_status = 0;
    HRESULT write_status = 0;
    void addRef() {}
    void release() {}
    void setRead(HRESULT status, SOCmnVariant*, WORD, SOCmnDateTime*, DWORD) { read_status = status; }
    void setWrite(HRESULT status, DWORD) { write_status = status; }
};
template<class T> struct SOCmnPointer {
    T* p = nullptr;
    SOCmnPointer& operator=(T* value) { p = value; return *this; }
    bool isNotNull() const { return p != nullptr; }
    void addRef() { p->addRef(); }
    T* operator->() { return p; }
    operator T*() { return p; }
};
struct Group {
    SODaCItem items[16];
    std::vector<HRESULT> read_results, write_results;
    std::vector<SODaCItem*> read_items, write_items;
    std::vector<int> values;
    std::vector<WORD> qualities;
    std::vector<std::uint32_t> times;
    SODaCItem* getItemByClientHandle(OPCHANDLE handle) {
        return handle > 0 && handle < 16 ? &items[handle] : nullptr;
    }
    void setLastError(HRESULT, int) {}
    void onReadAdvise(BYTE, DWORD, HRESULT, HRESULT, DWORD count, SODaCItem** item,
                      SOCmnVariant* value, WORD* quality, SOCmnDateTime* time, HRESULT* errors) {
        for (DWORD i = 0; i < count; ++i) {
            read_items.push_back(item[i]); read_results.push_back(errors[i]); values.push_back(value[i].value);
            qualities.push_back(quality[i]); times.push_back(time[i].value.low);
        }
    }
    void onWriteAdvise(DWORD, HRESULT, DWORD count, SODaCItem** item, HRESULT* errors) {
        for (DWORD i = 0; i < count; ++i) { write_items.push_back(item[i]); write_results.push_back(errors[i]); }
    }
};
struct GroupImpl { void onDataChangeBegin() {} void onDataChangeEnd() {} };
struct SODaCDataCallback {
    Group* m_group;
    GroupImpl* m_pComGroupImpl;
    HRESULT handleReadAdvise(BYTE, DWORD, OPCHANDLE, HRESULT, HRESULT, DWORD,
                            OPCHANDLE*, VARIANT*, WORD*, FILETIME*, HRESULT*);
};
struct SOCltComGuard { explicit SOCltComGuard(SODaCDataCallback*) {} };
#define IN
#define S_OK 0
#define OPC_E_INVALIDHANDLE (-1)
#define SODAC_ERR_READANDDATACHANGE_CALLBACK 0
#define SODAC_ERR_ADVISE_DATACALLBACK 0
#define _TRACE(...)
#define _TRACE_IF(...) if (false)
#define SUCCEEDED(status) ((status) >= 0)
#define _T(text) text
#define LPCTSTR const char*

@SDK_METHOD@

// Exact loop and onWriteAdvise call from the upstream OnWriteComplete body.
// The rest of the COM entry point is deliberately not emulated.
static void write_fragment(Group* m_group, DWORD dwCount, OPCHANDLE* pClienthandles, HRESULT* pErrors) {
    DWORD i; DWORD rellevantItemCount = 0; DWORD dwTransid = 7; HRESULT hrMastererr = 0;
    SOCmnPointer<SODaCItem> item;
    std::vector<ptrdiff_t> item_storage(dwCount);
    std::vector<HRESULT> error_storage(dwCount);
    ptrdiff_t* pItems = item_storage.data();
    HRESULT* pRellevantErrors = error_storage.data();
@SDK_WRITE_FRAGMENT@
}

struct CaseResult { bool read; bool write; };
static CaseResult run_case(const std::vector<OPCHANDLE>& handles, BYTE mode) {
    Group group; GroupImpl impl; SODaCDataCallback callback{&group, &impl};
    const DWORD count = static_cast<DWORD>(handles.size());
    std::vector<OPCHANDLE> input = handles;
    std::vector<VARIANT> values(count);
    std::vector<WORD> qualities(count);
    std::vector<FILETIME> times(count);
    std::vector<HRESULT> errors(count);
    std::vector<HRESULT> expected;
    std::vector<SODaCItem*> expected_items;
    std::vector<int> expected_values;
    std::vector<WORD> expected_qualities;
    std::vector<std::uint32_t> expected_times;
    for (DWORD i = 0; i < count; ++i) {
        values[i].value = 100 + static_cast<int>(i);
        qualities[i] = static_cast<WORD>(0xc0 + i);
        times[i] = {10 + i, 0};
        errors[i] = i % 3 == 0 ? -123 : (i % 3 == 1 ? 7 : 0);
        if (handles[i] > 0 && handles[i] < 16) {
            expected_items.push_back(&group.items[handles[i]]);
            expected.push_back(errors[i]); expected_values.push_back(values[i].value);
            expected_qualities.push_back(qualities[i]); expected_times.push_back(times[i].low);
        }
    }
    const auto original_errors = errors;
    callback.handleReadAdvise(mode, 42, 0, 0, 0, count, input.data(), values.data(), qualities.data(), times.data(), errors.data());
    bool read_okay = group.read_items == expected_items && group.read_results == expected && group.values == expected_values &&
        group.qualities == expected_qualities && group.times == expected_times && errors == original_errors;
    for (DWORD i = 0; i < count; ++i)
        if (handles[i] > 0 && handles[i] < 16) read_okay = read_okay && group.items[handles[i]].read_status == errors[i];
    write_fragment(&group, count, input.data(), errors.data());
    bool write_okay = group.write_items == expected_items && group.write_results == expected && errors == original_errors;
    for (DWORD i = 0; i < count; ++i)
        if (handles[i] > 0 && handles[i] < 16) write_okay = write_okay && group.items[handles[i]].write_status == errors[i];
    return {read_okay, write_okay};
}
int main(int argc, char** argv) {
    const bool baseline = argc == 2 && std::strcmp(argv[1], "--expect-existing-defect") == 0;
    const std::vector<std::vector<OPCHANDLE>> cases = {
        {}, {0}, {0, 1}, {1, 0, 2}, {1, 2, 0}, {0, 1, 2, 0, 3}, {1, 2, 3}
    };
    int failed_read = 0, failed_write = 0;
    for (BYTE mode : {BYTE(0), BYTE(1)}) {
        for (const auto& handles : cases) {
            CaseResult result = run_case(handles, mode);
            if (!result.read || !result.write) {
                failed_read += !result.read;
                failed_write += !result.write;
                std::printf("mismatch mode=%u read=%d write=%d handles=", unsigned(mode), int(!result.read), int(!result.write));
                for (auto h : handles) std::printf("%u,", unsigned(h));
                std::printf("\n");
            }
        }
    }
    std::printf("cases=%zu read_mismatches=%d write_mismatches=%d\n", cases.size() * 2, failed_read, failed_write);
    return (baseline ? failed_read > 0 && failed_write > 0 : failed_read == 0 && failed_write == 0) ? 0 : 1;
}
