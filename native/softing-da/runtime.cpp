#include "stdafx.h"
#include "runtime.h"
#ifdef OPCL_DA_TESTING
#include "SOCltThread.h"
#endif
#include <atomic>
#include <new>
#include <unordered_set>
#include <unordered_map>
#include <string>
#include <vector>
#include <cwchar>
#include <deque>
#include <memory>
#include <mutex>
#include <utility>
#include <cstring>
#include <cstddef>

static_assert(sizeof(OpclDaReadOutcome) == 36, "read outcome ABI size");
static_assert(offsetof(OpclDaReadOutcome, available) == 32, "read outcome ABI offset");
#ifdef OPCL_DA_TESTING
#include <cstdio>
#include <condition_variable>
#include <chrono>
// Test process only: 0=idle, 1=armed, 2=consumed. The SDK worker executes
// the throw after a real successful syncWrite; no item ID or retry is involved.
static std::atomic<int32_t> test_write_exception{0};
extern "C" int32_t opcl_da_test_arm_write_exception(void) {
    int32_t expected = 0;
    return test_write_exception.compare_exchange_strong(expected, 1) ? S_OK : E_UNEXPECTED;
}
extern "C" int32_t opcl_da_test_write_exception_state(void) {
    return test_write_exception.load();
}
#define OPCL_DISCOVERY_TRACE(phase) do { std::fprintf(stderr, "SDK discovery: %s\n", phase); std::fflush(stderr); } while (0)
static LONG CALLBACK trace_test_access_violation(EXCEPTION_POINTERS* exception) {
    if (!exception || !exception->ExceptionRecord ||
        exception->ExceptionRecord->ExceptionCode != EXCEPTION_ACCESS_VIOLATION) return EXCEPTION_CONTINUE_SEARCH;
    void* address = exception->ExceptionRecord->ExceptionAddress;
    HMODULE module = nullptr;
    char name[MAX_PATH] = {};
    if (GetModuleHandleExA(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                           reinterpret_cast<LPCSTR>(address), &module)) {
        GetModuleFileNameA(module, name, MAX_PATH);
    }
    std::fprintf(stderr, "SDK fixture AV: thread=%lu address=%p module=%s base=%p offset=%llu",
                 static_cast<unsigned long>(GetCurrentThreadId()), address, name, module,
                 module ? static_cast<unsigned long long>(reinterpret_cast<uintptr_t>(address) - reinterpret_cast<uintptr_t>(module)) : 0ULL);
    if (exception->ExceptionRecord->NumberParameters >= 2) std::fprintf(stderr, " access=%llu target=%p",
        static_cast<unsigned long long>(exception->ExceptionRecord->ExceptionInformation[0]),
        reinterpret_cast<void*>(exception->ExceptionRecord->ExceptionInformation[1]));
#ifdef _M_IX86
    if (exception->ContextRecord) std::fprintf(stderr, " EIP=%08lx ESP=%08lx",
        static_cast<unsigned long>(exception->ContextRecord->Eip), static_cast<unsigned long>(exception->ContextRecord->Esp));
#endif
    std::fprintf(stderr, "\n"); std::fflush(stderr);
    return EXCEPTION_CONTINUE_SEARCH;
}
#else
#define OPCL_DISCOVERY_TRACE(phase) do {} while (0)
#endif

// The SDK asks its host for this module handle to load resources.
HINSTANCE getInstanceHandle() { return ::GetModuleHandle(nullptr); }

// All calls to OPC and all VARIANT ownership stay inside the SDK. The ABI
// initially exposes I4 explicitly; other types must fail, never coerce to I4.
struct OpclDaHost {
    explicit OpclDaHost(SODaCEntry* e) : entry(e), owner_thread(GetCurrentThreadId()), sessions(0) {}
    SODaCEntry* entry;
    DWORD owner_thread;
    std::atomic<unsigned> sessions;
};
// The SDK remains the sole OPC callback owner. This object only receives
// SDK onReadAdvise facts and publishes them to a bounded application queue.
struct PendingPush { OpclDaPush fact; std::vector<char> payload; };
struct PushState {
    std::mutex mutex;
    std::unordered_map<OPCHANDLE, uint64_t> registrations;
    std::deque<PendingPush> pending;
    size_t pending_bytes = 0;
    bool overflow = false;
#ifdef OPCL_DA_TESTING
    // One test-only callback slot; no SDK object or caller buffer is retained.
    std::condition_variable async_ready;
    uint32_t async_next = 1;
    uint32_t async_pending = 0;
    OPCHANDLE async_handle = 0;
    bool async_done = false;
    bool async_timeout = false;
    bool async_submit_done = false;
    bool async_task_done = false;
    HRESULT async_submit_result = E_FAIL;
    HRESULT async_submit_item = E_FAIL;
    HRESULT async_result = E_FAIL;
    OpclDaReadI4 async_value{};
#endif
};
class RuntimeGroup : public SODaCComGroupDefault {
public:
    std::shared_ptr<PushState> push;
#ifdef OPCL_DA_TESTING
    bool test_has_correlated_callback() const { return m_dataCallback != nullptr; }
    std::atomic<bool> test_disable_initial_read{false};
protected:
    BOOL performStateTransitionAction(DWORD action) override {
        if (test_disable_initial_read.load() && action == SODAC_ITEM_ACTION_INITIAL_READ) return FALSE;
        return SODaCComGroupDefault::performStateTransitionAction(action);
    }
public:
#endif
    void onReadAdvise(BYTE mode, DWORD transaction, HRESULT master_quality,
                      HRESULT master_error, DWORD n, SODaCItem** items,
                      SOCmnVariant* values, WORD* qualities, SOCmnDateTime* times,
                      HRESULT* errors) override;
};
class RuntimeCreator : public SODaCCreator {
public:
    SODaCGroup* createGroup(BYTE protocol, SODaCServer* parent) override {
        if (protocol == SOCLT_PROTOCOL_DCOM) return new RuntimeGroup();
        return SODaCCreator::createGroup(protocol, parent);
    }
};
struct OpclDaItem { SODaCItem* sdk; };
// A reference to an SDK browse object, never a COM interface or VARIANT.
struct BrowseRef {
    explicit BrowseRef(SOCltBrowseObject* object = nullptr) : p(object) { if (p) p->addRef(); }
    BrowseRef(const BrowseRef& other) : p(other.p) { if (p) p->addRef(); }
    BrowseRef& operator=(const BrowseRef& other) {
        if (other.p) other.p->addRef();
        if (p) p->release();
        p = other.p; return *this;
    }
    ~BrowseRef() { if (p) p->release(); }
    SOCltBrowseObject* p;
};
struct OpclDaSession {
    OpclDaHost* host;
    SODaCServer* server;
    SODaCGroup* group;
    std::unordered_set<OpclDaItem*> items;
    std::unordered_map<std::string, BrowseRef> branches;
    bool subscription;
    uint32_t registration_budget;
    std::shared_ptr<PushState> push;
    std::vector<SODaCItem*> retired;
};
static std::atomic<bool> host_exists(false);
// ABI text is bounded UTF-8. SDK objects remain responsible for BSTR/VARIANT.
static bool to_wide(const char* input, std::vector<WCHAR>& output) {
    if (!input) return false;
    size_t size = 0;
    while (size <= 65536 && input[size]) ++size;
    if (size > 65536) return false;
    if (size == 0) { output.assign(1, 0); return true; }
    const int needed = MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, input,
                                            static_cast<int>(size), nullptr, 0);
    if (needed <= 0) return false;
    output.resize(static_cast<size_t>(needed) + 1);
    if (MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, input, static_cast<int>(size),
                            output.data(), needed) != needed) return false;
    output[needed] = 0;
    return true;
}
static bool owns(OpclDaSession* s, OpclDaItem* i) {
    return s && i && s->items.find(i) != s->items.end();
}
extern "C" int32_t opcl_da_host_create(OpclDaHost** out) try {
#ifdef OPCL_DA_TESTING
    static std::once_flag trace_handler;
    std::call_once(trace_handler, [] { AddVectoredExceptionHandler(1, trace_test_access_violation); });
#endif
    if (!out) return E_INVALIDARG;
    *out = nullptr;
    bool expected = false;
    if (!host_exists.compare_exchange_strong(expected, true)) return E_PENDING;
    initSOCmn();
    SODaCEntry* entry = getSODaCEntry();
    if (!entry || !entry->initialize(COINIT_MULTITHREADED)) {
        if (entry) entry->release();
        exitSOCmn(); host_exists.store(false); return E_FAIL;
    }
    RuntimeCreator* creator = new (std::nothrow) RuntimeCreator();
    if (!creator) {
        entry->terminate(); entry->release(); exitSOCmn(); host_exists.store(false);
        return E_OUTOFMEMORY;
    }
    entry->setCreator(creator); creator->release();
    OpclDaHost* host = new (std::nothrow) OpclDaHost(entry);
    if (!host) {
        entry->terminate(); entry->release(); exitSOCmn(); host_exists.store(false);
        return E_OUTOFMEMORY;
    }
    *out = host; return S_OK;
} catch (...) { return E_UNEXPECTED; }
extern "C" int32_t opcl_da_host_destroy(OpclDaHost* host) try {
    if (!host || host->owner_thread != GetCurrentThreadId()) return E_INVALIDARG;
    if (host->sessions.load() != 0) return E_PENDING;
    const BOOL ok = host->entry->terminate();
    host->entry->release(); delete host; exitSOCmn(); host_exists.store(false);
    return ok ? S_OK : E_FAIL;
} catch (...) { return E_UNEXPECTED; }
extern "C" int32_t opcl_da_session_open(OpclDaHost* host, const char* name, const char* node,
                                           const OpclDaGuid* id, uint32_t subscription, uint32_t update_rate_ms,
                                           uint32_t registration_budget, uint32_t* revised_rate_ms, OpclDaSession** out) try {
    if (!out || !revised_rate_ms) return E_INVALIDARG;
    *out = nullptr; *revised_rate_ms = 0;
    if (!host || !id || subscription > 1 || !registration_budget || !update_rate_ms) return E_INVALIDARG;
    std::vector<WCHAR> wide_name, wide_node;
    if (!to_wide(name, wide_name) || !to_wide(node, wide_node)) return E_INVALIDARG;
    GUID clsid{id->data1, id->data2, id->data3, {}};
    for (unsigned i = 0; i != 8; ++i) clsid.Data4[i] = id->data4[i];
    OPCL_DISCOVERY_TRACE("session addServer");
    SODaCServer* server = host->entry->addServer(wide_name.data(), wide_node.data(), clsid);
    if (!server) return E_OUTOFMEMORY;
    OPCL_DISCOVERY_TRACE("session connect");
    HRESULT hr = server->connect(FALSE);
    OPCL_DISCOVERY_TRACE("session connect returned");
    if (SUCCEEDED(hr) && !server->isConnected()) hr = E_FAIL;
    OPCL_DISCOVERY_TRACE("session addGroup");
    SODaCGroup* group = SUCCEEDED(hr) ? server->addGroup(nullptr, update_rate_ms) : nullptr;
    std::shared_ptr<PushState> push;
    if (SUCCEEDED(hr) && subscription) {
        try { push = std::make_shared<PushState>(); }
        catch (...) { hr = E_OUTOFMEMORY; }
    }
    if (SUCCEEDED(hr) && !group) hr = E_OUTOFMEMORY;
    if (SUCCEEDED(hr)) {
        RuntimeGroup* runtime_group = static_cast<RuntimeGroup*>(group);
        runtime_group->push = push;
        group->setDataRetrieval(subscription ? SODaCGroup::drAsyncAdvise : SODaCGroup::drPolledRefresh);
        OPCL_DISCOVERY_TRACE("session group start");
        hr = group->start(FALSE);
        OPCL_DISCOVERY_TRACE("session group start returned");
        if (SUCCEEDED(hr) && !group->isStarted()) hr = E_FAIL;
        // The SDK must actually retain advise mode; never silently turn the
        // requested stream into its cyclic sync-read fallback.
        if (SUCCEEDED(hr) && subscription && group->getDataRetrieval() != SODaCGroup::drAsyncAdvise) hr = E_NOINTERFACE;
        if (SUCCEEDED(hr)) *revised_rate_ms = group->getUpdateRate();
    }
    if (FAILED(hr)) {
        if (group) { group->disconnect(); server->removeBranch(group); group->release(); }
        server->disconnect(); host->entry->removeBranch(server); server->release();
        return hr;
    }
    OpclDaSession* session = new (std::nothrow) OpclDaSession{host, server, group, {}, {}, subscription != 0, registration_budget, push, {}};
    if (!session) {
        group->disconnect(); server->removeBranch(group); group->release();
        server->disconnect(); host->entry->removeBranch(server); server->release();
        return E_OUTOFMEMORY;
    }
    ++host->sessions; *out = session;
    OPCL_DISCOVERY_TRACE("session open returned");
    return S_OK;
} catch (...) { return E_UNEXPECTED; }
#ifdef OPCL_DA_TESTING
extern "C" int32_t opcl_da_test_da1_is_inactive(OpclDaSession* s) try {
    if (!s || s->subscription || s->server->getForcedOPCVersion() != SODAC_VERSION_OPC_DAV10A ||
        !s->group->isConnected() || s->group->isStarted()) return E_FAIL;
    return S_OK;
} catch (...) { return E_UNEXPECTED; }
extern "C" int32_t opcl_da_test_da1_inactive(OpclDaSession* s) try {
    if (!s || s->subscription || !s->items.empty()) return E_INVALIDARG;
    HRESULT hr = s->group->disconnect();
    if (FAILED(hr)) return hr;
    if (!s->server->forceOPCVersion(SODAC_VERSION_OPC_DAV10A)) return E_FAIL;
    hr = s->group->connect(FALSE);
    if (FAILED(hr)) return hr;
    return opcl_da_test_da1_is_inactive(s);
} catch (...) { return E_UNEXPECTED; }
#endif
extern "C" int32_t opcl_da_item_add(OpclDaSession* s, const char* id, uint32_t registration_low,
                                     uint32_t registration_high, OpclDaItem** out,
                                     uint16_t* type, uint32_t* rights) try {
    if (!out || !type || !rights) return E_INVALIDARG;
    *out = nullptr; *type = 0; *rights = 0;
    const uint64_t registration = (static_cast<uint64_t>(registration_high) << 32) | registration_low;
    if (!s || !registration) return E_INVALIDARG;
    if (s->subscription) {
        if (s->retired.size() + s->items.size() >= s->registration_budget)
            return HRESULT_FROM_WIN32(ERROR_NOT_ENOUGH_MEMORY);
        // Every active item can later be retired without allocation failure.
        const size_t needed = s->retired.size() + s->items.size() + 1;
        if (s->retired.capacity() < needed) {
            const size_t doubled = s->retired.capacity() ? s->retired.capacity() * 2 : 64;
            const size_t desired = doubled > needed ? doubled : needed;
            const size_t target = desired > s->registration_budget ? s->registration_budget : desired;
            try { s->retired.reserve(target); }
            catch (...) { return E_OUTOFMEMORY; }
        }
    }
    std::vector<WCHAR> wide_id;
    if (!to_wide(id, wide_id)) return E_INVALIDARG;
    SODaCItem* sdk = s->group->addItem(wide_id.data());
    if (!sdk) return E_OUTOFMEMORY;
    OpclDaItem* item = new (std::nothrow) OpclDaItem{sdk};
    if (!item) { s->group->removeLeaf(sdk); sdk->release(); return E_OUTOFMEMORY; }
    if (s->subscription) {
        bool inserted = false;
        try {
            std::lock_guard<std::mutex> lock(s->push->mutex);
            inserted = sdk->getClientHandle() &&
                s->push->registrations.emplace(sdk->getClientHandle(), registration).second;
        } catch (...) {
            s->group->removeLeaf(sdk); sdk->release(); delete item; return E_OUTOFMEMORY;
        }
        if (!inserted) { s->group->removeLeaf(sdk); sdk->release(); delete item; return E_FAIL; }
    }
    HRESULT hr = sdk->start(FALSE);
    if (SUCCEEDED(hr) && !sdk->isStarted()) hr = E_FAIL;
    if (SUCCEEDED(hr)) {
        try { s->items.insert(item); }
        catch (...) { hr = E_OUTOFMEMORY; }
    }
    if (FAILED(hr)) {
        if (s->subscription) { std::lock_guard<std::mutex> lock(s->push->mutex); s->push->registrations.erase(sdk->getClientHandle()); }
        sdk->disconnect(); s->group->removeLeaf(sdk);
        if (s->subscription) s->retired.push_back(sdk); else sdk->release();
        delete item; return hr;
    }
    *type = sdk->getNativeDatatype(); *rights = sdk->getAccessRights(); *out = item;
    return S_OK;
} catch (...) { return E_UNEXPECTED; }
extern "C" int32_t opcl_da_item_remove(OpclDaSession* s, OpclDaItem* item) try {
    if (!owns(s, item)) return E_INVALIDARG;
#ifdef OPCL_DA_TESTING
    if (s->push) { std::lock_guard<std::mutex> lock(s->push->mutex); if (s->push->async_pending) return E_PENDING; }
#endif
    if (s->subscription) {
        std::lock_guard<std::mutex> lock(s->push->mutex);
        s->push->registrations.erase(item->sdk->getClientHandle());
    }
    s->items.erase(item);
    const HRESULT hr = item->sdk->disconnect();
    const BOOL removed = s->group->removeLeaf(item->sdk);
    // Retain the SDK object/handle until group close. A late callback cannot
    // resolve an old server handle to a newly registered SDK object.
    if (s->subscription) s->retired.push_back(item->sdk); else item->sdk->release();
    delete item;
    return FAILED(hr) ? hr : (removed ? S_OK : E_FAIL);
} catch (...) { return E_UNEXPECTED; }
static HRESULT clear_browse(OpclDaSession* s);
extern "C" int32_t opcl_da_session_close(OpclDaSession* s) try {
    if (!s) return E_INVALIDARG;
#ifdef OPCL_DA_TESTING
    if (s->push) { std::lock_guard<std::mutex> lock(s->push->mutex); if (s->push->async_pending) return E_PENDING; }
#endif
    OPCL_DISCOVERY_TRACE("session clear browse");
    const HRESULT cleared = clear_browse(s);
    if (FAILED(cleared)) return cleared;
    HRESULT result = S_OK;
    while (!s->items.empty()) {
        const HRESULT hr = opcl_da_item_remove(s, *s->items.begin());
        if (FAILED(hr)) result = hr;
    }
    OPCL_DISCOVERY_TRACE("session disconnect group");
    HRESULT hr = s->group->disconnect(); if (FAILED(hr)) result = hr;
    if (!s->server->removeBranch(s->group)) result = E_FAIL;
    s->group->release();
    for (SODaCItem* retired : s->retired) retired->release();
    s->retired.clear();
    OPCL_DISCOVERY_TRACE("session disconnect server");
    hr = s->server->disconnect(); if (FAILED(hr)) result = hr;
    if (!s->host->entry->removeBranch(s->server)) result = E_FAIL;
    s->server->release(); --s->host->sessions; delete s;
    OPCL_DISCOVERY_TRACE("session close returned");
    return result;
} catch (...) { return E_UNEXPECTED; }
// Dispatch synchronous SDK operations through its own per-server worker. The
// worker initializes COM; the session caller need not own a COM apartment.
class TestStatusTask : public SOCltTask {
public:
    TestStatusTask(SODaCServer* server, int32_t* state)
        : SOCltTask(server), server_(server), state_(state), ran_(false) {}
    void execute() override try {
        ran_ = true;
        SODaCComServerImpl* sdk = dynamic_cast<SODaCComServerImpl*>(server_);
        if (!sdk) { setResult(E_NOINTERFACE); return; }
        // The public SDK overload owns COM status and vendor allocations. A
        // zero-initialized output prevents a missing SDK status from becoming
        // a false RUNNING verdict; zero is an unknown state in the contract.
        OPCSERVERSTATUS status{};
        const HRESULT hr = sdk->getStatus(&status, nullptr);
        if (SUCCEEDED(hr)) *state_ = static_cast<int32_t>(status.dwServerState);
        setResult(hr);
    } catch (...) { setResult(E_UNEXPECTED); }
    bool ran() const { return ran_; }
private:
    SODaCServer* server_;
    int32_t* state_;
    bool ran_;
};
extern "C" int32_t opcl_da_server_status(OpclDaSession* session, int32_t* state) try {
    if (!session || !state) return E_INVALIDARG;
    *state = 0;
    TestStatusTask* task = new (std::nothrow) TestStatusTask(session->server, state);
    if (!task) return E_OUTOFMEMORY;
    const HRESULT hr = session->server->executeTask(task);
    const bool ran = task->ran();
    task->release();
    return ran || FAILED(hr) ? hr : E_UNEXPECTED;
} catch (...) { return E_UNEXPECTED; }
class ReadI4Task : public SOCltTask {
public:
    ReadI4Task(SODaCGroup* group, SODaCItem* item, DWORD age, OpclDaReadI4* out)
        : SOCltTask(group), group_(group), item_(item), age_(age), out_(out) {}
    void execute() override try {
        SODaCItem* items[] = {item_};
        SOCmnVariant value; SOCmnDateTime time; WORD quality = 0; HRESULT error = E_FAIL;
        const HRESULT hr = group_->syncRead(1, items, age_, &value, &quality, &time, &error);
        FILETIME ft{}; time.get(&ft);
        out_->service_result = hr; out_->item_result = error; out_->quality = quality;
        out_->filetime_low = ft.dwLowDateTime; out_->filetime_high = ft.dwHighDateTime;
        out_->vartype = value.vt;
        if (SUCCEEDED(hr) && SUCCEEDED(error)) {
            if (value.vt != VT_I4) { setResult(DISP_E_TYPEMISMATCH); return; }
            out_->value = value.lVal;
        }
        setResult(S_OK);
    } catch (...) { setResult(E_UNEXPECTED); }
private:
    SODaCGroup* group_;
    SODaCItem* item_;
    DWORD age_;
    OpclDaReadI4* out_;
};
class WriteI4Task : public SOCltTask {
public:
    WriteI4Task(SODaCGroup* group, SODaCItem* item, int32_t v, int32_t* result)
        : SOCltTask(group), group_(group), item_(item), value_(v), result_(result), ran_(false) {}
    void execute() override try {
        ran_ = true;
        SOCmnVariant value; value.setI4(static_cast<LONG>(value_));
        SODaCItem* items[] = {item_}; HRESULT error = E_FAIL;
        const HRESULT hr = group_->syncWrite(1, items, &value, nullptr, nullptr, &error);
        *result_ = error; setResult(hr);
    } catch (...) { setResult(E_UNEXPECTED); }
    bool ran() const { return ran_; }
private:
    SODaCGroup* group_;
    SODaCItem* item_;
    int32_t value_;
    int32_t* result_;
    bool ran_;
};
extern "C" int32_t opcl_da_read_i4(OpclDaSession* s, OpclDaItem* item, uint32_t max_age,
                                    OpclDaReadI4* out) try {
    if (!out) return E_INVALIDARG;
    *out = {};
    // The SDK can return S_FALSE if a task was not queued. Never let the
    // untouched output look like a successful zero-valued read.
    out->service_result = E_FAIL;
    out->item_result = E_FAIL;
    if (!owns(s, item)) return E_INVALIDARG;
    ReadI4Task* task = new (std::nothrow) ReadI4Task(s->group, item->sdk, max_age, out);
    if (!task) return E_OUTOFMEMORY;
    const HRESULT hr = s->group->executeTask(task);
    task->release();
    return hr;
} catch (...) { return E_UNEXPECTED; }
extern "C" int32_t opcl_da_write_i4(OpclDaSession* s, OpclDaItem* item, int32_t v,
                                     int32_t* item_result) try {
    if (!item_result || !owns(s, item)) return E_INVALIDARG;
    *item_result = E_FAIL;
    WriteI4Task* task = new (std::nothrow) WriteI4Task(s->group, item->sdk, v, item_result);
    if (!task) return E_OUTOFMEMORY;
    const HRESULT hr = s->group->executeTask(task);
    const bool ran = task->ran();
    task->release();
    return ran || FAILED(hr) ? hr : E_UNEXPECTED;
} catch (...) { return E_UNEXPECTED; }

// Scalar data is copied out of the SDK-owned SOCmnVariant before it dies.
// Every write constructs the VARIANT through the SDK's typed setters.
#include <cstring>
#include <cmath>
static uint64_t scalar_bits(const OpclDaScalar& value) {
    return (static_cast<uint64_t>(value.bits_high) << 32) | value.bits_low;
}
static void put_bits(OpclDaScalar* out, uint64_t bits) {
    out->bits_low = static_cast<uint32_t>(bits);
    out->bits_high = static_cast<uint32_t>(bits >> 32);
}
static HRESULT scalar_copy(const VARIANT& value, OpclDaScalar* out) {
    int64_t signed_value = 0; uint64_t unsigned_value = 0; double float_value = 0;
    switch (value.vt) {
    case VT_EMPTY: case VT_NULL: put_bits(out, 0); return S_OK;
    case VT_I1: signed_value = value.cVal; break;
    case VT_I2: signed_value = value.iVal; break;
    case VT_I4: signed_value = value.lVal; break;
    case VT_ERROR: unsigned_value = static_cast<uint32_t>(value.scode); goto unsign;
    case VT_FILETIME: unsigned_value = value.ullVal; goto unsign;
    case VT_INT: signed_value = value.intVal; break;
    case VT_I8: signed_value = value.llVal; break;
    case VT_CY: signed_value = value.cyVal.int64; break;
    case VT_UI1: unsigned_value = value.bVal; goto unsign;
    case VT_UI2: unsigned_value = value.uiVal; goto unsign;
    case VT_UI4: unsigned_value = value.ulVal; goto unsign;
    case VT_UINT: unsigned_value = value.uintVal; goto unsign;
    case VT_UI8: unsigned_value = value.ullVal; goto unsign;
    case VT_BOOL: unsigned_value = value.boolVal != VARIANT_FALSE; goto unsign;
    case VT_R4: float_value = value.fltVal; goto floating;
    case VT_R8: float_value = value.dblVal; goto floating;
    case VT_DATE: float_value = value.date; goto floating;
    default: return DISP_E_TYPEMISMATCH;
    }
    std::memcpy(&unsigned_value, &signed_value, sizeof(unsigned_value));
unsign:
    put_bits(out, unsigned_value); return S_OK;
floating:
    static_assert(sizeof(float_value) == sizeof(unsigned_value), "f64 ABI");
    std::memcpy(&unsigned_value, &float_value, sizeof(unsigned_value));
    put_bits(out, unsigned_value); return S_OK;
}
static HRESULT scalar_set(SOCmnVariant& value, uint16_t vt, uint32_t low, uint32_t high) {
    const uint64_t bits = (static_cast<uint64_t>(high) << 32) | low;
    int64_t signed_value; double floating;
    std::memcpy(&signed_value, &bits, sizeof(bits));
    std::memcpy(&floating, &bits, sizeof(bits));
    switch (vt) {
    case VT_EMPTY: return S_OK;
    case VT_I1: if (signed_value < -128 || signed_value > 127) return DISP_E_OVERFLOW; value.setI1(static_cast<char>(signed_value)); break;
    case VT_I2: if (signed_value < -32768 || signed_value > 32767) return DISP_E_OVERFLOW; value.setI2(static_cast<short>(signed_value)); break;
    case VT_I4: case VT_INT:
        if (signed_value < INT32_MIN || signed_value > INT32_MAX) return DISP_E_OVERFLOW;
        if (vt == VT_I4) value.setI4(static_cast<LONG>(signed_value)); else value.setINT(static_cast<int>(signed_value)); break;
    case VT_I8: value.setI8(signed_value); break;
    case VT_CY: value.setCY(signed_value); break;
    case VT_UI1: if (bits > UINT8_MAX) return DISP_E_OVERFLOW; value.setUI1(static_cast<BYTE>(bits)); break;
    case VT_UI2: if (bits > UINT16_MAX) return DISP_E_OVERFLOW; value.setUI2(static_cast<unsigned short>(bits)); break;
    case VT_UI4: case VT_UINT:
        if (bits > UINT32_MAX) return DISP_E_OVERFLOW;
        if (vt == VT_UI4) value.setUI4(static_cast<ULONG>(bits)); else value.setUINT(static_cast<unsigned int>(bits)); break;
    case VT_UI8: value.setUI8(bits); break;
    case VT_BOOL: if (bits > 1) return E_INVALIDARG; value.setBOOL(bits != 0); break;
    case VT_R4: if (std::isfinite(floating) && std::fabs(floating) > 3.402823466e+38) return DISP_E_OVERFLOW; value.setR4(static_cast<float>(floating)); break;
    case VT_R8: value.setR8(floating); break;
    case VT_DATE: if (!std::isfinite(floating)) return E_INVALIDARG; value.setDATE(floating); break;
    default: return DISP_E_TYPEMISMATCH;
    }
    return S_OK;
}
// Primitive byte width agrees with SDK SAFEARRAY element layout on Win32.
static uint32_t primitive_array_width(uint16_t vt) {
    switch (vt) {
    case VT_I1: case VT_UI1: return 1;
    case VT_I2: case VT_UI2: case VT_BOOL: return 2;
    case VT_I4: case VT_UI4: case VT_INT: case VT_UINT: case VT_R4: case VT_ERROR: return 4;
    case VT_I8: case VT_UI8: case VT_R8: case VT_CY: case VT_DATE: case VT_FILETIME: return 8;
    default: return 0;
    }
}
// VARIANT_BOOL uses only 0 and -1; reject noncanonical ABI bytes before
// constructing an SDK task, rather than sending an invalid SAFEARRAY.
static bool valid_bool_array(const uint8_t* data, uint32_t bytes) {
    if (bytes % 2 || (bytes && !data)) return false;
    for (uint32_t i = 0; i < bytes; i += 2) {
        if (!((data[i] == 0 && data[i + 1] == 0) ||
              (data[i] == 0xff && data[i + 1] == 0xff))) return false;
    }
    return true;
}
static HRESULT copy_sdk_value(VARIANT& value, OpclDaScalar* out, char* text, uint32_t capacity, unsigned depth = 0) {
    out->vartype = value.vt;
    if (depth >= 32 || (value.vt & VT_BYREF)) return DISP_E_TYPEMISMATCH;
    if (value.vt == (VT_ARRAY | VT_VARIANT)) {
        if (!value.parray || SafeArrayGetDim(value.parray) != 1 || SafeArrayGetElemsize(value.parray) != sizeof(VARIANT)) return DISP_E_TYPEMISMATCH;
        LONG lower = 0, upper = -1;
        HRESULT hr = SafeArrayGetLBound(value.parray, 1, &lower);
        if (FAILED(hr)) return hr;
        hr = SafeArrayGetUBound(value.parray, 1, &upper);
        if (FAILED(hr)) return hr;
        const int64_t count = static_cast<int64_t>(upper) - lower + 1;
        if (count < 0 || count > 256) return HRESULT_FROM_WIN32(ERROR_INSUFFICIENT_BUFFER);
        void* borrowed = nullptr; hr = SafeArrayAccessData(value.parray, &borrowed);
        if (FAILED(hr)) return hr;
        VARIANT* values = static_cast<VARIANT*>(borrowed);
        const uint32_t limit = capacity < 65536 ? capacity : 65536;
        uint32_t used = 0;
        for (int64_t i = 0; SUCCEEDED(hr) && i < count; ++i) {
            if (!values || !text || used > limit || limit - used < 6) { hr = HRESULT_FROM_WIN32(ERROR_INSUFFICIENT_BUFFER); break; }
            OpclDaScalar element{};
            const uint32_t header = used; used += 6;
            try { hr = copy_sdk_value(values[i], &element, text + used, limit - used, depth + 1); }
            catch (...) { hr = E_UNEXPECTED; }
            if (FAILED(hr)) break;
            uint32_t length = element.text_len;
            if (!(element.vartype & VT_ARRAY) && element.vartype != VT_BSTR) {
                if (FAILED(scalar_copy(values[i], &element)) || limit - used < 8) { hr = DISP_E_TYPEMISMATCH; break; }
                const uint64_t bits = scalar_bits(element);
                std::memcpy(text + used, &bits, 8); length = 8;
            }
            if (length > limit - used) { hr = HRESULT_FROM_WIN32(ERROR_INSUFFICIENT_BUFFER); break; }
            std::memcpy(text + header, &element.vartype, 2);
            std::memcpy(text + header + 2, &length, 4); used += length;
        }
        const HRESULT unlocked = SafeArrayUnaccessData(value.parray);
        if (FAILED(hr)) return hr;
        if (FAILED(unlocked)) return unlocked;
        out->text_len = used; return S_OK;
    }
    if (value.vt == (VT_ARRAY | VT_BSTR)) {
        if (!value.parray || SafeArrayGetDim(value.parray) != 1 ||
            SafeArrayGetElemsize(value.parray) != sizeof(BSTR)) return DISP_E_TYPEMISMATCH;
        LONG lower = 0, upper = -1;
        HRESULT hr = SafeArrayGetLBound(value.parray, 1, &lower);
        if (FAILED(hr)) return hr;
        hr = SafeArrayGetUBound(value.parray, 1, &upper);
        if (FAILED(hr)) return hr;
        const int64_t count = static_cast<int64_t>(upper) - lower + 1;
        if (count < 0 || count > 256) return HRESULT_FROM_WIN32(ERROR_INSUFFICIENT_BUFFER);
        void* borrowed = nullptr;
        hr = SafeArrayAccessData(value.parray, &borrowed);
        if (FAILED(hr)) return hr;
        const BSTR* strings = static_cast<const BSTR*>(borrowed);
        const uint32_t limit = capacity < 65536 ? capacity : 65536;
        uint32_t used = 0;
        for (int64_t i = 0; SUCCEEDED(hr) && i < count; ++i) {
            if (!strings) { hr = E_UNEXPECTED; break; }
            const BSTR v = strings[i];
            const UINT length = v ? SysStringLen(v) : 0;
            if (length > 65536 || (v && std::wcslen(v) != length)) { hr = DISP_E_TYPEMISMATCH; break; }
            const int size = length ? WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, v,
                static_cast<int>(length), nullptr, 0, nullptr, nullptr) : 0;
            if (length && size <= 0) { hr = E_INVALIDARG; break; }
            if (used > limit || limit - used < 4 || static_cast<uint32_t>(size) > limit - used - 4 || !text) {
                hr = HRESULT_FROM_WIN32(ERROR_INSUFFICIENT_BUFFER); break;
            }
            const uint32_t bytes = static_cast<uint32_t>(size);
            std::memcpy(text + used, &bytes, 4); used += 4;
            if (bytes && WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, v,
                static_cast<int>(length), text + used, size, nullptr, nullptr) != size) { hr = E_INVALIDARG; break; }
            used += bytes;
        }
        const HRESULT unlocked = SafeArrayUnaccessData(value.parray);
        if (FAILED(hr)) return hr;
        if (FAILED(unlocked)) return unlocked;
        out->text_len = used;
        return S_OK;
    }
    if ((value.vt & VT_ARRAY) && primitive_array_width(value.vt & ~VT_ARRAY)) {
        if (!value.parray || SafeArrayGetDim(value.parray) != 1) return DISP_E_TYPEMISMATCH;
        const ULONG element_size = primitive_array_width(value.vt & ~VT_ARRAY);
        if (SafeArrayGetElemsize(value.parray) != element_size) return DISP_E_TYPEMISMATCH;
        LONG lower = 0, upper = -1;
        HRESULT hr = SafeArrayGetLBound(value.parray, 1, &lower);
        if (FAILED(hr)) return hr;
        hr = SafeArrayGetUBound(value.parray, 1, &upper);
        if (FAILED(hr)) return hr;
        const int64_t elements = static_cast<int64_t>(upper) - lower + 1;
        if (elements < 0 || elements > 256 || elements > static_cast<int64_t>((capacity < 65536 ? capacity : 65536) / element_size))
            return HRESULT_FROM_WIN32(ERROR_INSUFFICIENT_BUFFER);
        const uint32_t bytes = static_cast<uint32_t>(elements * element_size);
        if (bytes && !text) return E_INVALIDARG;
        void* borrowed = nullptr;
        hr = SafeArrayAccessData(value.parray, &borrowed);
        if (FAILED(hr)) return hr;
        if (bytes && !borrowed) { SafeArrayUnaccessData(value.parray); return E_UNEXPECTED; }
        if (bytes) std::memcpy(text, borrowed, bytes);
        hr = SafeArrayUnaccessData(value.parray);
        if (FAILED(hr)) return hr;
        out->text_len = bytes;
        return S_OK;
    }
    if (value.vt != VT_BSTR) {
        // Unknown VARIANT tags are preserved for Rust's explicit Unsupported.
        (void)scalar_copy(value, out);
        return S_OK;
    }
    const UINT original = value.bstrVal ? SysStringLen(value.bstrVal) : 0;
    if (original > 65536 || original > capacity) return HRESULT_FROM_WIN32(ERROR_INSUFFICIENT_BUFFER);
    SOCmnVariant sdk_string;
    if (!sdk_string.initFrom(&value)) return E_OUTOFMEMORY;
    SOCmnString storage; LPCTSTR wide = sdk_string.getBSTR(storage);
    const size_t length = wide ? std::wcslen(wide) : 0;
    if (length != original) return DISP_E_TYPEMISMATCH;
    if (!length) return S_OK;
    const int needed = WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, wide,
                                           static_cast<int>(length), nullptr, 0, nullptr, nullptr);
    if (needed <= 0) return E_INVALIDARG;
    if (static_cast<uint32_t>(needed) > capacity || !text) return HRESULT_FROM_WIN32(ERROR_INSUFFICIENT_BUFFER);
    if (WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, wide, static_cast<int>(length),
                            text, needed, nullptr, nullptr) != needed) return E_INVALIDARG;
    out->text_len = static_cast<uint32_t>(needed);
    return S_OK;
}
#ifdef OPCL_DA_TESTING
extern "C" int32_t opcl_da_test_codec_bounds(void) try {
    std::wstring large(65536, L'x'); std::vector<char> bytes(65536);
    SOCmnVariant text; text.setBSTR(large.c_str()); if (!text.bstrVal) return E_OUTOFMEMORY;
    OpclDaScalar out{};
    if (FAILED(copy_sdk_value(text, &out, bytes.data(), static_cast<uint32_t>(bytes.size()))) || out.text_len != 65536) return E_FAIL;
    out = {};
    if (SUCCEEDED(copy_sdk_value(text, &out, bytes.data(), 65535))) return E_FAIL;
    std::wstring too_large(65537, L'x'); SOCmnVariant oversized; oversized.setBSTR(too_large.c_str());
    if (!oversized.bstrVal) return E_OUTOFMEMORY;
    if (SUCCEEDED(copy_sdk_value(oversized, &out, bytes.data(), 65536))) return E_FAIL;
    for (ULONG count = 256; count <= 257; ++count) {
        SOCmnVariant array;
        SAFEARRAY* storage = SOCmnVariant::safeArrayCreateVector(VT_UI1, 0, count);
        if (!storage) return E_OUTOFMEMORY;
        array.vt = VT_ARRAY | VT_UI1; array.parray = storage; out = {};
        const HRESULT hr = copy_sdk_value(array, &out, bytes.data(), static_cast<uint32_t>(bytes.size()));
        if (count == 256 ? (FAILED(hr) || out.text_len != 256) : SUCCEEDED(hr)) return E_FAIL;
    }
    return S_OK;
} catch (...) { return E_UNEXPECTED; }
#endif
static void queue_push(PushState& state, const OpclDaPush& fact, const char* data, uint32_t length) {
    if (state.pending.size() >= 256) { state.overflow = true; return; }
    try {
        PendingPush event{}; event.fact = fact;
        if (length) event.payload.assign(data, data + length);
        const size_t charge = sizeof(PendingPush) + event.payload.capacity();
        if (charge > 1048576u - state.pending_bytes) { state.overflow = true; return; }
        state.pending.push_back(std::move(event)); state.pending_bytes += charge;
    } catch (...) { state.overflow = true; }
}
#ifdef OPCL_DA_TESTING
extern "C" int32_t opcl_da_test_queue_bounds(void) try {
    PushState count_state; OpclDaPush fact{};
    for (unsigned i = 0; i < 257; ++i) queue_push(count_state, fact, nullptr, 0);
    if (count_state.pending.size() != 256 || !count_state.overflow || count_state.pending_bytes != 256 * sizeof(PendingPush)) return E_FAIL;
    PushState large; std::vector<char> payload(OPCL_DA_VALUE_BYTES, 'x');
    for (unsigned i = 0; i < 16; ++i) queue_push(large, fact, payload.data(), OPCL_DA_VALUE_BYTES);
    if (large.pending.empty() || !large.overflow || large.pending_bytes > 1048576) return E_FAIL;
    const size_t charge = sizeof(PendingPush) + large.pending.front().payload.capacity();
    if (large.pending.size() != 1048576 / charge) return E_FAIL;
    return S_OK;
} catch (...) { return E_UNEXPECTED; }
#endif
#ifdef OPCL_DA_TESTING
// One-shot callback barrier, before adapter registration lookup and mutex.
// SDK owns the callback stack and item references until onReadAdvise returns.
static HANDLE test_late_entered = nullptr;
static HANDLE test_late_release = nullptr;
static std::atomic<int> test_late_armed{0};
static std::atomic<LONG> test_late_value{0};
static std::mutex test_late_mutex;
static std::condition_variable test_late_cv;
static bool test_late_processed = false;
static bool test_late_wait_ok = false;
extern "C" int32_t opcl_da_test_late_arm(int32_t value) try {
    std::lock_guard<std::mutex> lock(test_late_mutex);
    if (test_late_entered || test_late_release) return E_PENDING;
    test_late_entered = CreateEventW(nullptr, TRUE, FALSE, nullptr);
    test_late_release = CreateEventW(nullptr, TRUE, FALSE, nullptr);
    if (!test_late_entered || !test_late_release) {
        if (test_late_entered) CloseHandle(test_late_entered);
        if (test_late_release) CloseHandle(test_late_release);
        test_late_entered = test_late_release = nullptr; return E_OUTOFMEMORY;
    }
    test_late_processed = false; test_late_wait_ok = false;
    test_late_value.store(value); test_late_armed.store(1); return S_OK;
} catch (...) { return E_UNEXPECTED; }
extern "C" int32_t opcl_da_test_late_entered(uint32_t timeout_ms) {
    if (!timeout_ms || timeout_ms > 30000) return E_INVALIDARG;
    return test_late_entered && WaitForSingleObject(test_late_entered, timeout_ms) == WAIT_OBJECT_0 ? S_OK : E_FAIL;
}
extern "C" int32_t opcl_da_test_late_held(void) {
    return test_late_release && WaitForSingleObject(test_late_release, 0) == WAIT_TIMEOUT ? S_OK : E_FAIL;
}
extern "C" int32_t opcl_da_test_late_release(void) {
    return test_late_release && SetEvent(test_late_release) ? S_OK : E_FAIL;
}
extern "C" int32_t opcl_da_test_late_processed(uint32_t timeout_ms) try {
    if (!timeout_ms || timeout_ms > 30000) return E_INVALIDARG;
    std::unique_lock<std::mutex> lock(test_late_mutex);
    if (!test_late_cv.wait_for(lock, std::chrono::milliseconds(timeout_ms), []{ return test_late_processed; })) return E_PENDING;
    return test_late_wait_ok ? S_OK : E_FAIL;
} catch (...) { return E_UNEXPECTED; }
extern "C" int32_t opcl_da_test_late_close(void) try {
    std::lock_guard<std::mutex> lock(test_late_mutex);
    if (!test_late_entered || !test_late_release || !test_late_processed) return E_PENDING;
    CloseHandle(test_late_entered); CloseHandle(test_late_release);
    test_late_entered = test_late_release = nullptr; test_late_armed.store(0); return S_OK;
} catch (...) { return E_UNEXPECTED; }
struct TestLateCompletion {
    bool claimed = false, wait_ok = false;
    ~TestLateCompletion() {
        if (!claimed) return;
        std::lock_guard<std::mutex> lock(test_late_mutex);
        test_late_wait_ok = wait_ok; test_late_processed = true; test_late_cv.notify_all();
    }
};
#endif
void RuntimeGroup::onReadAdvise(BYTE mode, DWORD transaction, HRESULT master_quality,
                                HRESULT master_error, DWORD n, SODaCItem** items,
                                SOCmnVariant* values, WORD* qualities, SOCmnDateTime* times,
                                HRESULT* errors) {
    std::shared_ptr<PushState> state = push;
    if (!state) return;
#ifdef OPCL_DA_TESTING
    TestLateCompletion late;
    if (mode == 1 && transaction == 0 && n == 1 && items && items[0] && values &&
        values[0].vt == VT_I4 && values[0].lVal == test_late_value.load()) {
        int expected = 1;
        if (test_late_armed.compare_exchange_strong(expected, 2)) {
            late.claimed = true;
            if (SetEvent(test_late_entered))
                late.wait_ok = WaitForSingleObject(test_late_release, 30000) == WAIT_OBJECT_0;
        }
    }
    if (mode == 0) {
        try {
            std::lock_guard<std::mutex> lock(state->mutex);
            if (!state->async_pending || state->async_pending != transaction || state->async_done) return;
            // The SDK can forward an empty completion envelope, including
            // after handle compaction. A failed master result is terminal for
            // this nonreused transaction; it carries no item/sample facts.
            const bool empty_master_failure = n == 0 && FAILED(master_error);
            if (!empty_master_failure &&
                (n != 1 || !items || !items[0] || items[0]->getClientHandle() != state->async_handle)) return;
            state->async_value = {};
            state->async_value.service_result = master_error;
            state->async_value.item_result = E_FAIL;
            state->async_result = FAILED(master_error) ? master_error : E_FAIL;
            if (n == 1 && items && items[0] && values && qualities && times && errors &&
                items[0]->getClientHandle() == state->async_handle) {
                state->async_value.item_result = errors[0];
                state->async_value.vartype = values[0].vt;
                state->async_value.quality = qualities[0];
                FILETIME ft{}; times[0].get(&ft);
                state->async_value.filetime_low = ft.dwLowDateTime;
                state->async_value.filetime_high = ft.dwHighDateTime;
                if (FAILED(master_error)) state->async_result = master_error;
                else if (FAILED(errors[0])) state->async_result = errors[0];
                else if (values[0].vt != VT_I4) state->async_result = DISP_E_TYPEMISMATCH;
                else {
                    state->async_value.value = values[0].lVal;
                    state->async_result = S_OK;
                }
            }
            state->async_done = true;
            state->async_ready.notify_one();
        } catch (...) { /* caller observes a bounded timeout, never a false value */ }
        return;
    }
#else
    if (mode == 0) return; // ReadComplete is not a stream update.
#endif
    try {
        std::lock_guard<std::mutex> lock(state->mutex);
        if (mode != 1 || transaction != 0 || n > 256 ||
            (n && (!items || !values || !qualities || !times || !errors))) {
            state->overflow = true; return;
        }
        std::vector<char> scratch(OPCL_DA_VALUE_BYTES);
        if (FAILED(master_error) || (!n && master_quality != S_OK)) {
            OpclDaPush envelope{};
            envelope.master_quality = master_quality;
            envelope.value.service_result = master_error;
            queue_push(*state, envelope, nullptr, 0);
        }
        for (DWORD i = 0; i < n; ++i) {
            if (!items[i]) { state->overflow = true; continue; }
            const auto found = state->registrations.find(items[i]->getClientHandle());
            if (found == state->registrations.end()) continue; // retired registration
            OpclDaPush event{};
            event.registration_low = static_cast<uint32_t>(found->second);
            event.registration_high = static_cast<uint32_t>(found->second >> 32);
            event.master_quality = master_quality;
            event.value.service_result = master_error;
            event.value.item_result = errors[i];
            event.value.quality = qualities[i];
            FILETIME ft{}; times[i].get(&ft);
            event.value.filetime_low = ft.dwLowDateTime;
            event.value.filetime_high = ft.dwHighDateTime;
            event.value.vartype = values[i].vt;
            if (SUCCEEDED(master_error) && SUCCEEDED(errors[i])) {
                const HRESULT hr = copy_sdk_value(values[i], &event.value, scratch.data(), OPCL_DA_VALUE_BYTES);
                if (FAILED(hr)) event.adapter_result = hr;
            }
            queue_push(*state, event, scratch.data(), event.adapter_result < 0 ? 0 : event.value.text_len);
        }
    } catch (...) {
        try { std::lock_guard<std::mutex> lock(state->mutex); state->overflow = true; } catch (...) {}
    }
}
extern "C" int32_t opcl_da_take_push(OpclDaSession* session, OpclDaPush* out, char* payload,
                                       uint32_t capacity, uint32_t* count, uint32_t* overflow) try {
    if (!session || !out || !count || !overflow || (capacity && !payload)) return E_INVALIDARG;
    *count = 0; *overflow = 0;
    if (!session->subscription || !session->push) return E_NOINTERFACE;
    std::lock_guard<std::mutex> lock(session->push->mutex);
    *overflow = session->push->overflow ? 1 : 0;
    session->push->overflow = false;
    if (!session->push->pending.empty()) {
        const PendingPush& front = session->push->pending.front();
        if (front.payload.size() > capacity) return HRESULT_FROM_WIN32(ERROR_INSUFFICIENT_BUFFER);
        *out = front.fact;
        if (!front.payload.empty()) std::memcpy(payload, front.payload.data(), front.payload.size());
        session->push->pending_bytes -= sizeof(PendingPush) + front.payload.capacity();
        session->push->pending.pop_front(); *count = 1;
    }
    return S_OK;
} catch (...) { return E_UNEXPECTED; }
#ifdef OPCL_DA_TESTING
// Probe only: stop SDK-generated initial async reads before registering an item,
// so their auto-generated transaction IDs cannot collide with the test ID.
extern "C" int32_t opcl_da_test_async_prepare(OpclDaSession* session) try {
    if (!session || !session->push || !session->items.empty()) return E_INVALIDARG;
    auto group = static_cast<RuntimeGroup*>(session->group);
    if (!group->test_has_correlated_callback()) return E_NOINTERFACE;
    group->test_disable_initial_read.store(true);
    return S_OK;
} catch (...) { return E_UNEXPECTED; }
// Test-only deterministic worker barrier. It never owns SDK/COM lifecycle.
static HANDLE test_async_entered = nullptr;
static HANDLE test_async_release = nullptr;
extern "C" int32_t opcl_da_test_async_gate_arm(void) try {
    if (test_async_entered || test_async_release) return E_PENDING;
    test_async_entered = CreateEventW(nullptr, TRUE, FALSE, nullptr);
    test_async_release = CreateEventW(nullptr, TRUE, FALSE, nullptr);
    if (!test_async_entered || !test_async_release) {
        if (test_async_entered) CloseHandle(test_async_entered);
        if (test_async_release) CloseHandle(test_async_release);
        test_async_entered = test_async_release = nullptr; return E_OUTOFMEMORY;
    }
    return S_OK;
} catch (...) { return E_UNEXPECTED; }
extern "C" int32_t opcl_da_test_async_gate_entered(uint32_t timeout_ms) {
    return test_async_entered && WaitForSingleObject(test_async_entered, timeout_ms) == WAIT_OBJECT_0 ? S_OK : E_FAIL;
}
extern "C" int32_t opcl_da_test_async_gate_held(void) {
    return test_async_release && WaitForSingleObject(test_async_release, 0) == WAIT_TIMEOUT ? S_OK : E_FAIL;
}
extern "C" int32_t opcl_da_test_async_gate_release(void) {
    return test_async_release && SetEvent(test_async_release) ? S_OK : E_FAIL;
}
extern "C" int32_t opcl_da_test_async_gate_close(void) {
    if (!test_async_entered || !test_async_release) return E_INVALIDARG;
    CloseHandle(test_async_entered); CloseHandle(test_async_release);
    test_async_entered = test_async_release = nullptr; return S_OK;
}
// The SDK submits AsyncIO on its own worker; the session caller waits for the
// SDK callback without retaining pointers to caller-owned memory.
// Bases are destroyed in reverse declaration order. This sentinel runs only
// after the SDK SOCltTask base has released its retained group reference.
class TestAsyncRetirement {
public:
    TestAsyncRetirement(std::shared_ptr<PushState> state, DWORD transaction)
        : state_(std::move(state)), transaction_(transaction) {}
    ~TestAsyncRetirement() {
        std::lock_guard<std::mutex> lock(state_->mutex);
        if (state_->async_pending == transaction_) {
            if (!state_->async_submit_done) {
                state_->async_submit_done = true;
                state_->async_submit_result = E_ABORT;
                state_->async_submit_item = E_ABORT;
            }
            state_->async_task_done = true;
            state_->async_ready.notify_all();
        }
    }
private:
    std::shared_ptr<PushState> state_;
    DWORD transaction_;
};
class TestAsyncReadTask : private TestAsyncRetirement, public SOCltTask {
public:
    TestAsyncReadTask(SODaCGroup* group, SODaCItem* item, DWORD age, DWORD transaction,
                      std::shared_ptr<PushState> state)
        : TestAsyncRetirement(state, transaction), SOCltTask(group), group_(group), item_(item),
          age_(age), transaction_(transaction), state_(std::move(state)) {
        item_->addRef();
    }
    ~TestAsyncReadTask() override { item_->release(); }
    void execute() override {
        HRESULT hr = E_UNEXPECTED, item_result = E_FAIL;
        try {
            if (test_async_entered && test_async_release) {
                SetEvent(test_async_entered);
                if (WaitForSingleObject(test_async_release, 60000) != WAIT_OBJECT_0) {
                    setResult(E_ABORT);
                    std::lock_guard<std::mutex> lock(state_->mutex);
                    state_->async_submit_done = true; state_->async_submit_result = E_ABORT;
                    state_->async_submit_item = E_ABORT; state_->async_ready.notify_all(); return;
                }
            }
            SODaCItem* items[] = {item_}; DWORD cancel = 0;
            hr = group_->asyncRead(1, items, age_, transaction_, &cancel, &item_result);
        } catch (...) { hr = E_UNEXPECTED; }
        setResult(hr);
        std::lock_guard<std::mutex> lock(state_->mutex);
        if (state_->async_pending == transaction_) {
            state_->async_submit_done = true;
            state_->async_submit_result = hr;
            state_->async_submit_item = item_result;
            state_->async_ready.notify_all();
        }
    }
private:
    SODaCGroup* group_; SODaCItem* item_; DWORD age_; DWORD transaction_;
    std::shared_ptr<PushState> state_;
};
static bool test_async_terminal(const PushState& state) {
    return state.async_task_done && state.async_submit_done &&
        (FAILED(state.async_submit_result) || FAILED(state.async_submit_item) || state.async_done);
}
// Nonblocking poll: a timed-out request remains pinned until the SDK task and
// completion are terminal. Only the serialized session owner may call this.
extern "C" int32_t opcl_da_test_async_retire(OpclDaSession* session, OpclDaReadI4* out) try {
    if (!session || !session->push || !out) return E_INVALIDARG;
    auto state = session->push;
    std::lock_guard<std::mutex> lock(state->mutex);
    if (!state->async_pending) return S_FALSE;
    if (!test_async_terminal(*state)) return E_PENDING;
    HRESULT result = FAILED(state->async_submit_result) ? state->async_submit_result :
                     FAILED(state->async_submit_item) ? state->async_submit_item : state->async_result;
    if (state->async_timeout) { *out = {}; result = S_OK; } // cleanup acknowledgement, not a second read result
    else if (SUCCEEDED(state->async_submit_result) && SUCCEEDED(state->async_submit_item)) *out = state->async_value;
    else { *out = {}; out->service_result = state->async_submit_result; out->item_result = state->async_submit_item; }
    state->async_pending = 0;
    return result;
} catch (...) { return E_UNEXPECTED; }
extern "C" int32_t opcl_da_test_async_is_pending(OpclDaSession* session, uint32_t* pending) try {
    if (!session || !session->push || !pending) return E_INVALIDARG;
    std::lock_guard<std::mutex> lock(session->push->mutex);
    *pending = session->push->async_pending ? 1 : 0; return S_OK;
} catch (...) { return E_UNEXPECTED; }
extern "C" int32_t opcl_da_test_async_wait_retire(OpclDaSession* session, uint32_t timeout_ms, OpclDaReadI4* out, uint32_t* retired) try {
    if (!session || !session->push || !out || !retired || !timeout_ms || timeout_ms > 30000) return E_INVALIDARG;
    *retired = 0;
    auto state = session->push;
    {
        std::unique_lock<std::mutex> lock(state->mutex);
        if (!state->async_pending) return S_FALSE;
        if (!state->async_ready.wait_for(lock, std::chrono::milliseconds(timeout_ms), [&] { return test_async_terminal(*state); })) return E_PENDING;
    }
    HRESULT result = opcl_da_test_async_retire(session, out);
    uint32_t pending = 1;
    if (SUCCEEDED(opcl_da_test_async_is_pending(session, &pending)) && !pending) *retired = 1;
    return result;
} catch (...) { return E_UNEXPECTED; }
extern "C" int32_t opcl_da_test_async_read_i4(OpclDaSession* session, OpclDaItem* item,
                                               uint32_t max_age, uint32_t timeout_ms,
                                               OpclDaReadI4* out) try {
    if (!out || !owns(session, item) || !timeout_ms || timeout_ms > 30000) return E_INVALIDARG;
    *out = {}; out->service_result = E_FAIL; out->item_result = E_FAIL;
    auto state = session->push;
    auto group = static_cast<RuntimeGroup*>(session->group);
    if (!state || !group->test_has_correlated_callback() || !group->test_disable_initial_read.load()) return E_NOINTERFACE;
    SOCltWorkerThread* worker = session->server->getWorkerThread();
    if (!worker || worker->isOwnThread() || !worker->isRunning()) return E_NOINTERFACE;
    uint32_t token = 0;
    {
        std::lock_guard<std::mutex> lock(state->mutex);
        if (state->async_pending) return E_PENDING;
        if (!state->async_next || state->async_next >= SODACITEM_TRANSACTIONID_ASYNC_NOT_STARTED) return E_OUTOFMEMORY;
        const OPCHANDLE handle = item->sdk->getClientHandle();
        if (!handle) return E_INVALIDARG;
        token = state->async_next++;
        state->async_pending = token; state->async_handle = handle;
        state->async_done = false; state->async_timeout = false; state->async_submit_done = false; state->async_task_done = false;
    }
    TestAsyncReadTask* task = new (std::nothrow) TestAsyncReadTask(session->group, item->sdk, max_age, token, state);
    if (!task) { std::lock_guard<std::mutex> lock(state->mutex); state->async_pending = 0; return E_OUTOFMEMORY; }
    BOOL queued = FALSE;
    try { queued = worker->addTask(task); }
    catch (...) {
        task->release();
        std::lock_guard<std::mutex> lock(state->mutex);
        state->async_timeout = true; // unknown enqueue; retain until SDK retirement
        throw;
    }
    task->release();
    if (!queued) {
        std::lock_guard<std::mutex> lock(state->mutex);
        state->async_pending = 0;
        return E_FAIL;
    }
    {
        std::unique_lock<std::mutex> lock(state->mutex);
        if (!state->async_ready.wait_for(lock, std::chrono::milliseconds(timeout_ms),
                                        [&] { return test_async_terminal(*state); })) {
            state->async_timeout = true;
            return HRESULT_FROM_WIN32(WAIT_TIMEOUT);
        }
    }
    return opcl_da_test_async_retire(session, out);
} catch (...) { return E_UNEXPECTED; }
extern "C" int32_t opcl_da_test_async_callback_state(void) try {
    auto state = std::make_shared<PushState>();
    RuntimeGroup* group = new (std::nothrow) RuntimeGroup();
    if (!group) return E_OUTOFMEMORY;
    SODaCItem* item = group->addItem(_T("N2.async.probe"));
    if (!item) { group->release(); return E_FAIL; }
    group->push = state;
    state->async_pending = 7; state->async_handle = item->getClientHandle();
    SODaCItem* items[] = {item}; SOCmnVariant value; value.setI4(42);
    WORD quality = OPC_QUALITY_GOOD; SOCmnDateTime time; time.now(); HRESULT error = S_OK;
    FILETIME expected_time{}; time.get(&expected_time);
    group->onReadAdvise(0, 6, S_OK, S_OK, 1, items, &value, &quality, &time, &error);
    bool ok = !state->async_done;
    group->onReadAdvise(0, 7, S_OK, S_OK, 1, items, &value, &quality, &time, &error);
    ok = ok && state->async_done && state->async_result == S_OK && state->async_value.value == 42 &&
         state->async_value.quality == quality && state->async_value.vartype == VT_I4 &&
         state->async_value.filetime_low == expected_time.dwLowDateTime &&
         state->async_value.filetime_high == expected_time.dwHighDateTime;
    value.setI4(99);
    group->onReadAdvise(0, 7, S_OK, S_OK, 1, items, &value, &quality, &time, &error);
    ok = ok && state->async_value.value == 42;
    state->async_pending = 0; state->async_done = false;
    group->onReadAdvise(0, 7, S_OK, S_OK, 1, items, &value, &quality, &time, &error);
    ok = ok && !state->async_done;
    // SDK OnReadComplete can deliver a correlated master failure with no
    // surviving item handles. Stale tokens and wrong handles remain ignored.
    state->async_pending = 11; state->async_done = false;
    state->async_handle = item->getClientHandle();
    SODaCItem* other = group->addItem(_T("N2.async.other"));
    if (!other) { item->release(); group->release(); return E_OUTOFMEMORY; }
    SODaCItem* wrong[] = {other};
    group->onReadAdvise(0, 11, S_OK, E_ACCESSDENIED, 1, wrong, &value, &quality, &time, &error);
    ok = ok && !state->async_done;
    group->onReadAdvise(0, 10, S_OK, E_ACCESSDENIED, 0, nullptr, nullptr, nullptr, nullptr, nullptr);
    ok = ok && !state->async_done;
    group->onReadAdvise(0, 11, S_OK, S_OK, 0, nullptr, nullptr, nullptr, nullptr, nullptr);
    ok = ok && !state->async_done;
    group->onReadAdvise(0, 11, S_OK, E_ACCESSDENIED, 0, nullptr, nullptr, nullptr, nullptr, nullptr);
    ok = ok && state->async_done && state->async_result == E_ACCESSDENIED &&
         state->async_value.service_result == E_ACCESSDENIED && state->async_value.item_result == E_FAIL &&
         state->async_value.quality == 0 && state->async_value.vartype == 0 &&
         state->async_value.filetime_low == 0 && state->async_value.filetime_high == 0;
    other->release();
    state->async_pending = 8; state->async_done = false;
    error = E_INVALIDARG;
    group->onReadAdvise(0, 8, S_OK, S_OK, 1, items, &value, &quality, &time, &error);
    ok = ok && state->async_done && state->async_result == E_INVALIDARG &&
         state->async_value.service_result == S_OK && state->async_value.item_result == E_INVALIDARG;
    state->async_pending = 9; state->async_done = false; error = S_OK;
    group->onReadAdvise(0, 9, S_OK, E_ACCESSDENIED, 1, items, &value, &quality, &time, &error);
    ok = ok && state->async_done && state->async_result == E_ACCESSDENIED &&
         state->async_value.service_result == E_ACCESSDENIED && state->async_value.item_result == S_OK;
    state->async_pending = 10; state->async_done = false; value.clear();
    group->onReadAdvise(0, 10, S_OK, S_OK, 1, items, &value, &quality, &time, &error);
    ok = ok && state->async_done && state->async_result == DISP_E_TYPEMISMATCH &&
         state->async_value.vartype == VT_EMPTY;
    item->release(); group->release();
    return ok ? S_OK : E_FAIL;
} catch (...) { return E_UNEXPECTED; }
#endif

class ReadScalarTask : public SOCltTask {
public:
    ReadScalarTask(SODaCGroup* group, SODaCItem* item, DWORD age, OpclDaScalar* out, char* text, uint32_t capacity, uint32_t* available)
        : SOCltTask(group), group_(group), item_(item), age_(age), out_(out), text_(text), capacity_(capacity), available_(available) {}
    void execute() override try {
        SODaCItem* items[] = {item_}; SOCmnVariant value; SOCmnDateTime time;
        WORD quality = 0; HRESULT item_result = E_FAIL;
        const HRESULT service = group_->syncRead(1, items, age_, &value, &quality, &time, &item_result);
        out_->service_result = service;
        if (available_) *available_ |= OPCL_DA_READ_SERVICE;
        if (SUCCEEDED(service)) {
            out_->item_result = item_result;
            if (available_) *available_ |= OPCL_DA_READ_ITEM;
        }
        if (SUCCEEDED(service) && SUCCEEDED(item_result)) {
            FILETIME ft{}; time.get(&ft);
            out_->filetime_low = ft.dwLowDateTime; out_->filetime_high = ft.dwHighDateTime;
            out_->quality = quality; out_->vartype = value.vt;
            if (available_) *available_ |= OPCL_DA_READ_SAMPLE;
            setResult(copy_sdk_value(value, out_, text_, capacity_));
        } else setResult(S_OK);
    } catch (...) { setResult(E_UNEXPECTED); }
private:
    SODaCGroup* group_; SODaCItem* item_; DWORD age_; OpclDaScalar* out_; char* text_; uint32_t capacity_; uint32_t* available_;
};
#ifdef OPCL_DA_TESTING
// A test-only real SDK group call, not the Gateway's per-key read path.
class TestMixedReadTask : public SOCltTask {
public:
    TestMixedReadTask(SODaCGroup* group, SODaCItem* first, SODaCItem* second, OpclDaReadOutcome* out)
        : SOCltTask(group), group_(group), first_(first), second_(second), out_(out) {}
    void execute() override try {
        ran_ = true;
        SODaCItem* items[] = {first_, second_}; SOCmnVariant values[2]; SOCmnDateTime times[2];
        WORD qualities[2]{}; HRESULT errors[2] = {E_FAIL, E_FAIL};
        const HRESULT service = group_->syncRead(2, items, 0, values, qualities, times, errors);
        for (unsigned i = 0; i != 2; ++i) {
            out_[i].value.service_result = service;
            out_[i].available = OPCL_DA_READ_SERVICE;
            if (SUCCEEDED(service)) {
                out_[i].value.item_result = errors[i]; out_[i].available |= OPCL_DA_READ_ITEM;
                if (SUCCEEDED(errors[i])) {
                    FILETIME ft{}; times[i].get(&ft);
                    out_[i].value.filetime_low = ft.dwLowDateTime; out_[i].value.filetime_high = ft.dwHighDateTime;
                    out_[i].value.quality = qualities[i]; out_[i].value.vartype = values[i].vt;
                    out_[i].available |= OPCL_DA_READ_SAMPLE;
                    const HRESULT converted = scalar_copy(values[i], &out_[i].value);
                    if (FAILED(converted)) { setResult(converted); return; }
                }
            }
        }
        setResult(S_OK);
    } catch (...) { setResult(E_UNEXPECTED); }
    bool ran() const { return ran_; }
private:
    bool ran_ = false;
    SODaCGroup* group_; SODaCItem* first_; SODaCItem* second_; OpclDaReadOutcome* out_;
};
extern "C" int32_t opcl_da_test_mixed_read(OpclDaSession* session, OpclDaItem* first,
                                              OpclDaItem* second, OpclDaReadOutcome out[2]) try {
    if (!out || !owns(session, first) || !owns(session, second) || first == second) return E_INVALIDARG;
    for (unsigned i = 0; i != 2; ++i) { out[i] = {}; out[i].value.service_result = E_FAIL; out[i].value.item_result = E_FAIL; }
    TestMixedReadTask* task = new (std::nothrow) TestMixedReadTask(session->group, first->sdk, second->sdk, out);
    if (!task) return E_OUTOFMEMORY;
    const HRESULT hr = session->group->executeTask(task); const bool ran = task->ran(); task->release();
    return ran || FAILED(hr) ? hr : E_UNEXPECTED;
} catch (...) { return E_UNEXPECTED; }
#endif
class WriteScalarTask : public SOCltTask {
public:
    WriteScalarTask(SODaCGroup* group, SODaCItem* item, uint16_t vt, uint32_t low, uint32_t high, int32_t* result)
        : SOCltTask(group), group_(group), item_(item), vt_(vt), low_(low), high_(high), result_(result), dispatched_(false) {}
    void execute() override try {
        SOCmnVariant value; const HRESULT converted = scalar_set(value, vt_, low_, high_);
        if (FAILED(converted)) { setResult(converted); return; }
        SODaCItem* items[] = {item_}; HRESULT error = E_FAIL;
        dispatched_ = true;
        const HRESULT service = group_->syncWrite(1, items, &value, nullptr, nullptr, &error);
#ifdef OPCL_DA_TESTING
        if (SUCCEEDED(service) && SUCCEEDED(error)) {
            int32_t armed = 1;
            if (test_write_exception.compare_exchange_strong(armed, 2)) throw 1;
        }
#endif
        *result_ = error; setResult(service);
    } catch (...) { setResult(E_UNEXPECTED); }
    bool dispatched() const { return dispatched_; }
private:
    SODaCGroup* group_; SODaCItem* item_; uint16_t vt_; uint32_t low_, high_; int32_t* result_; bool dispatched_;
};
extern "C" int32_t opcl_da_read_scalar(OpclDaSession* s, OpclDaItem* item, uint32_t max_age,
                                        OpclDaScalar* out, char* text, uint32_t text_capacity) try {
    if (!out) return E_INVALIDARG;
    OpclDaReadOutcome facts{};
    const HRESULT hr = opcl_da_read_scalar_facts(s, item, max_age, &facts, text, text_capacity);
    *out = facts.value;
    return hr;
} catch (...) { return E_UNEXPECTED; }
extern "C" int32_t opcl_da_read_scalar_facts(OpclDaSession* s, OpclDaItem* item, uint32_t max_age,
                                              OpclDaReadOutcome* out, char* text, uint32_t text_capacity) try {
    if (!out) return E_INVALIDARG;
    *out = {}; out->value.service_result = E_FAIL; out->value.item_result = E_FAIL;
    if (!owns(s, item)) return E_INVALIDARG;
    ReadScalarTask* task = new (std::nothrow) ReadScalarTask(s->group, item->sdk, max_age, &out->value, text, text_capacity, &out->available);
    if (!task) return E_OUTOFMEMORY;
    const HRESULT hr = s->group->executeTask(task); task->release(); return hr;
} catch (...) { return E_UNEXPECTED; }
extern "C" int32_t opcl_da_write_scalar(OpclDaSession* s, OpclDaItem* item, uint16_t vt,
                                         uint32_t low, uint32_t high, int32_t* item_result) try {
    if (!item_result || !owns(s, item)) return E_INVALIDARG;
    *item_result = E_FAIL;
    // Conversion is validated before SDK dispatch. It performs no OPC calls.
    SOCmnVariant check; const HRESULT conversion = scalar_set(check, vt, low, high);
    if (FAILED(conversion)) return conversion;
    WriteScalarTask* task = new (std::nothrow) WriteScalarTask(s->group, item->sdk, vt, low, high, item_result);
    if (!task) return E_OUTOFMEMORY;
    const HRESULT hr = s->group->executeTask(task); const bool sent = task->dispatched(); task->release();
    return sent || FAILED(hr) ? hr : E_UNEXPECTED;
} catch (...) { return E_UNEXPECTED; }

// Recursively construct SDK-owned values from bounded, validated wire records.
static HRESULT build_variant_wire(SOCmnVariant& output, uint16_t vt, const uint8_t* data, uint32_t bytes, unsigned depth) {
    if (depth >= 32 || bytes > 65536 || (bytes && !data)) return E_INVALIDARG;
    if (vt == VT_BSTR) {
        const char* input = reinterpret_cast<const char*>(data);
        const int size = bytes ? MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, input, static_cast<int>(bytes), nullptr, 0) : 0;
        if (bytes && size <= 0) return E_INVALIDARG;
        std::vector<WCHAR> wide(static_cast<size_t>(size) + 1, 0);
        if (bytes && MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, input, static_cast<int>(bytes), wide.data(), size) != size) return E_INVALIDARG;
        if (std::wcslen(wide.data()) != static_cast<size_t>(size)) return E_INVALIDARG;
        output.setBSTR(wide.data()); return output.bstrVal ? S_OK : E_OUTOFMEMORY;
    }
    if (!(vt & VT_ARRAY)) {
        if (bytes != 8) return E_INVALIDARG;
        uint64_t bits = 0; std::memcpy(&bits, data, 8);
        return scalar_set(output, vt, static_cast<uint32_t>(bits), static_cast<uint32_t>(bits >> 32));
    }
    const uint16_t element = vt & ~VT_ARRAY;
    if (element != VT_VARIANT && element != VT_BSTR) {
        const uint32_t width = primitive_array_width(element);
        if (!width || bytes % width || bytes / width > 256) return E_INVALIDARG;
        SAFEARRAY* array = SOCmnVariant::safeArrayCreateVector(static_cast<VARTYPE>(element), 0, bytes / width);
        if (!array) return E_OUTOFMEMORY;
        output.vt = vt; output.parray = array;
        void* borrowed = nullptr; HRESULT hr = SafeArrayAccessData(array, &borrowed);
        if (FAILED(hr)) return hr;
        if (bytes && !borrowed) { SafeArrayUnaccessData(array); return E_UNEXPECTED; }
        if (bytes) std::memcpy(borrowed, data, bytes);
        return SafeArrayUnaccessData(array);
    }
    const uint32_t header = element == VT_VARIANT ? 6u : 4u;
    uint32_t offset = 0, count = 0;
    while (offset < bytes) {
        if (bytes - offset < header || count >= 256) return E_INVALIDARG;
        uint32_t length = 0;
        std::memcpy(&length, data + offset + (element == VT_VARIANT ? 2 : 0), 4);
        offset += header;
        if (length > bytes - offset) return E_INVALIDARG;
        offset += length; ++count;
    }
    SAFEARRAY* array = SOCmnVariant::safeArrayCreateVector(static_cast<VARTYPE>(element), 0, count);
    if (!array) return E_OUTOFMEMORY;
    output.vt = vt; output.parray = array; offset = 0;
    for (uint32_t i = 0; i < count; ++i) {
        uint16_t child_vt = VT_BSTR; uint32_t length = 0;
        if (element == VT_VARIANT) std::memcpy(&child_vt, data + offset, 2);
        std::memcpy(&length, data + offset + (element == VT_VARIANT ? 2 : 0), 4);
        offset += header;
        SOCmnVariant child;
        HRESULT hr = build_variant_wire(child, child_vt, data + offset, length, depth + 1);
        if (FAILED(hr)) return hr;
        LONG index = static_cast<LONG>(i);
        hr = SafeArrayPutElement(array, &index, element == VT_BSTR ? static_cast<void*>(child.bstrVal) : static_cast<void*>(static_cast<VARIANT*>(&child)));
        if (FAILED(hr)) return hr;
        offset += length;
    }
    return S_OK;
}
class WriteVariantArrayTask : public SOCltTask {
public:
    WriteVariantArrayTask(SODaCGroup* group, SODaCItem* item, const uint8_t* data, uint32_t bytes, int32_t* result)
        : SOCltTask(group), group_(group), item_(item), data_(data), bytes_(bytes), result_(result) {}
    void execute() override try {
        SOCmnVariant value;
        HRESULT hr = build_variant_wire(value, VT_ARRAY | VT_VARIANT, data_, bytes_, 0);
        if (FAILED(hr)) { setResult(hr); return; }
        SODaCItem* items[] = {item_}; HRESULT item_result = E_FAIL;
        hr = group_->syncWrite(1, items, &value, nullptr, nullptr, &item_result);
        *result_ = item_result; setResult(hr);
    } catch (...) { setResult(E_UNEXPECTED); }
private:
    SODaCGroup* group_; SODaCItem* item_; const uint8_t* data_; uint32_t bytes_; int32_t* result_;
};
// BSTR allocation remains inside SDK variant and system SAFEARRAY copy.
class WriteStringArrayTask : public SOCltTask {
public:
    WriteStringArrayTask(SODaCGroup* group, SODaCItem* item, const uint8_t* data,
                         uint32_t bytes, uint32_t count, int32_t* result)
        : SOCltTask(group), group_(group), item_(item), data_(data), bytes_(bytes), count_(count), result_(result) {}
    void execute() override try {
        SOCmnVariant value;
        SAFEARRAY* array = SOCmnVariant::safeArrayCreateVector(VT_BSTR, 0, count_);
        if (!array) { setResult(E_OUTOFMEMORY); return; }
        value.vt = VT_ARRAY | VT_BSTR; value.parray = array;
        uint32_t offset = 0;
        for (uint32_t i = 0; i < count_; ++i) {
            uint32_t length = 0; std::memcpy(&length, data_ + offset, 4); offset += 4;
            const char* text = reinterpret_cast<const char*>(data_ + offset);
            const int wide_size = length ? MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, text,
                static_cast<int>(length), nullptr, 0) : 0;
            if (length && wide_size <= 0) { setResult(E_INVALIDARG); return; }
            std::vector<WCHAR> wide(static_cast<size_t>(wide_size) + 1, 0);
            if (length && MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, text,
                static_cast<int>(length), wide.data(), wide_size) != wide_size) { setResult(E_INVALIDARG); return; }
            if (std::wcslen(wide.data()) != static_cast<size_t>(wide_size)) { setResult(E_INVALIDARG); return; }
            SOCmnVariant sdk_string; sdk_string.setBSTR(wide.data());
            if (!sdk_string.bstrVal) { setResult(E_OUTOFMEMORY); return; }
            LONG index = static_cast<LONG>(i);
            const HRESULT hr = SafeArrayPutElement(array, &index, sdk_string.bstrVal);
            if (FAILED(hr)) { setResult(hr); return; }
            offset += length;
        }
        SODaCItem* items[] = {item_}; HRESULT item_result = E_FAIL;
        const HRESULT hr = group_->syncWrite(1, items, &value, nullptr, nullptr, &item_result);
        *result_ = item_result; setResult(hr);
    } catch (...) { setResult(E_UNEXPECTED); }
private:
    SODaCGroup* group_; SODaCItem* item_; const uint8_t* data_;
    uint32_t bytes_, count_; int32_t* result_;
};
// SDK owns the array through SOCmnVariant; this ABI only copies POD bytes.
class WriteArrayTask : public SOCltTask {
public:
    WriteArrayTask(SODaCGroup* group, SODaCItem* item, uint16_t vt,
                   const uint8_t* data, uint32_t bytes, int32_t* result)
        : SOCltTask(group), group_(group), item_(item), vt_(vt), data_(data), bytes_(bytes), result_(result) {}
    void execute() override try {
        const VARTYPE element = static_cast<VARTYPE>(vt_ & ~VT_ARRAY);
        const uint32_t size = primitive_array_width(element);
        SOCmnVariant value;
        SAFEARRAY* array = SOCmnVariant::safeArrayCreateVector(element, 0, bytes_ / size);
        if (!array) { setResult(E_OUTOFMEMORY); return; }
        value.vt = vt_;
        value.parray = array;
        void* data = nullptr;
        HRESULT hr = SafeArrayAccessData(array, &data);
        if (FAILED(hr)) { setResult(hr); return; }
        if (bytes_ && !data) { SafeArrayUnaccessData(array); setResult(E_UNEXPECTED); return; }
        if (bytes_) std::memcpy(data, data_, bytes_);
        hr = SafeArrayUnaccessData(array);
        if (FAILED(hr)) { setResult(hr); return; }
        SODaCItem* items[] = {item_}; HRESULT item_result = E_FAIL;
        hr = group_->syncWrite(1, items, &value, nullptr, nullptr, &item_result);
        *result_ = item_result; setResult(hr);
    } catch (...) { setResult(E_UNEXPECTED); }
private:
    SODaCGroup* group_; SODaCItem* item_; uint16_t vt_; const uint8_t* data_;
    uint32_t bytes_; int32_t* result_;
};
extern "C" int32_t opcl_da_write_array(OpclDaSession* s, OpclDaItem* item, uint16_t vt,
                                         const uint8_t* data, uint32_t bytes, int32_t* result) try {
    if (!result || !owns(s, item) || (bytes && !data)) return E_INVALIDARG;
    if (bytes > 65536) return E_INVALIDARG;
    if (vt == (VT_ARRAY | VT_VARIANT)) {
        *result = E_FAIL;
        WriteVariantArrayTask* task = new (std::nothrow) WriteVariantArrayTask(s->group, item->sdk, data, bytes, result);
        if (!task) return E_OUTOFMEMORY;
        const HRESULT hr = s->group->executeTask(task); task->release(); return hr;
    }
    if (vt == (VT_ARRAY | VT_BSTR)) {
        uint32_t count = 0, offset = 0;
        while (offset < bytes) {
            if (bytes - offset < 4 || count >= 256) return E_INVALIDARG;
            uint32_t length = 0; std::memcpy(&length, data + offset, 4); offset += 4;
            if (length > bytes - offset) return E_INVALIDARG;
            offset += length; ++count;
        }
        *result = E_FAIL;
        WriteStringArrayTask* task = new (std::nothrow) WriteStringArrayTask(s->group, item->sdk, data, bytes, count, result);
        if (!task) return E_OUTOFMEMORY;
        const HRESULT hr = s->group->executeTask(task); task->release(); return hr;
    }
    const uint32_t size = (vt & VT_ARRAY) ? primitive_array_width(vt & ~VT_ARRAY) : 0;
    if (!size) return DISP_E_TYPEMISMATCH;
    if (bytes > 65536 || (bytes % size) || bytes / size > 256) return E_INVALIDARG;
    if ((vt & ~VT_ARRAY) == VT_BOOL && !valid_bool_array(data, bytes)) return E_INVALIDARG;
    *result = E_FAIL;
    WriteArrayTask* task = new (std::nothrow) WriteArrayTask(s->group, item->sdk, vt, data, bytes, result);
    if (!task) return E_OUTOFMEMORY;
    const HRESULT hr = s->group->executeTask(task);
    task->release(); return hr;
} catch (...) { return E_UNEXPECTED; }

class WriteTextTask : public SOCltTask {
public:
    WriteTextTask(SODaCGroup* group, SODaCItem* item, const char* utf8, int32_t* result)
        : SOCltTask(group), group_(group), item_(item), utf8_(utf8), result_(result), dispatched_(false) {}
    void execute() override try {
        std::vector<WCHAR> wide;
        if (!to_wide(utf8_, wide)) { setResult(E_INVALIDARG); return; }
        SOCmnVariant value; value.setBSTR(wide.data());
        if (value.vt != VT_BSTR || !value.bstrVal) { setResult(E_OUTOFMEMORY); return; }
        SODaCItem* items[] = {item_}; HRESULT error = E_FAIL;
        dispatched_ = true;
        const HRESULT service = group_->syncWrite(1, items, &value, nullptr, nullptr, &error);
        *result_ = error; setResult(service);
    } catch (...) { setResult(E_UNEXPECTED); }
    bool dispatched() const { return dispatched_; }
private:
    SODaCGroup* group_; SODaCItem* item_; const char* utf8_; int32_t* result_; bool dispatched_;
};
extern "C" int32_t opcl_da_write_text(OpclDaSession* s, OpclDaItem* item, const char* text,
                                       int32_t* item_result) try {
    if (!item_result || !owns(s, item)) return E_INVALIDARG;
    *item_result = E_FAIL;
    std::vector<WCHAR> validated;
    if (!to_wide(text, validated)) return E_INVALIDARG;
    WriteTextTask* task = new (std::nothrow) WriteTextTask(s->group, item->sdk, text, item_result);
    if (!task) return E_OUTOFMEMORY;
    const HRESULT hr = s->group->executeTask(task); const bool sent = task->dispatched(); task->release();
    return sent || FAILED(hr) ? hr : E_UNEXPECTED;
} catch (...) { return E_UNEXPECTED; }

static HRESULT wide_to_utf8(LPCTSTR input, char* output, uint32_t capacity, uint32_t* length) {
    if (!input || !output || !length) return E_INVALIDARG;
    const size_t chars = std::wcslen(input);
    if (chars > 65536) return HRESULT_FROM_WIN32(ERROR_INSUFFICIENT_BUFFER);
    if (!chars) { *length = 0; return S_OK; }
    const int bytes = WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, input,
                                           static_cast<int>(chars), nullptr, 0, nullptr, nullptr);
    if (bytes <= 0) return E_INVALIDARG;
    if (static_cast<uint32_t>(bytes) > capacity) return HRESULT_FROM_WIN32(ERROR_INSUFFICIENT_BUFFER);
    if (WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, input, static_cast<int>(chars),
                            output, bytes, nullptr, nullptr) != bytes) return E_INVALIDARG;
    *length = static_cast<uint32_t>(bytes);
    return S_OK;
}
static void copy_guid(const CLSID& input, OpclDaGuid* output) {
    output->data1 = input.Data1; output->data2 = input.Data2; output->data3 = input.Data3;
    std::memcpy(output->data4, input.Data4, 8);
}
static bool local_node(const WCHAR* node) {
    return !*node || _wcsicmp(node, L"localhost") == 0 || _wcsicmp(node, L".") == 0;
}
// Every discovery/resolve gets an independent SDK server worker. A hung
// OPCEnum therefore cannot block a different session or discovery.
// Keep the SDK worker owner alive until its thread has finished releasing the
// completed task. Completion signalling happens before the worker's release.
// SOCltServer is sufficient for task dispatch and does not create an unused
// COM-server implementation or its shutdown sink.
class EphemeralContext {
public:
    explicit EphemeralContext(SOCltServer* server) : server_(server) {}
    ~EphemeralContext() { if (server_.isNotNull()) server_->getWorkerThread()->shutdown(); }
    SOCltServer* get() { return server_; }
    EphemeralContext(const EphemeralContext&) = delete;
    EphemeralContext& operator=(const EphemeralContext&) = delete;
private:
    SOCmnPointer<SOCltServer> server_;
};
#ifdef OPCL_DA_TESTING
class LifetimeProbeServer : public SOCltServer {
public:
    LifetimeProbeServer(std::atomic<DWORD>* destroyed, std::atomic<int>* running, HANDLE destructor_entered)
        : destroyed_(destroyed), running_(running), destructor_entered_(destructor_entered) {}
    ~LifetimeProbeServer() override {
        *destroyed_ = GetCurrentThreadId();
        *running_ = getWorkerThread()->isRunning() ? 1 : 0;
        SetEvent(destructor_entered_);
    }
private:
    std::atomic<DWORD>* destroyed_;
    std::atomic<int>* running_;
    HANDLE destructor_entered_;
};
class LifetimeProbeTask : public SOCltTask {
public:
    LifetimeProbeTask(SOCltServer* server, HANDLE entered, HANDLE resume, std::atomic<DWORD>* worker)
        : SOCltTask(server), entered_(entered), resume_(resume), worker_(worker) {}
    void execute() override { setResult(S_OK); }
    void reportCompletion() override {
        *worker_ = GetCurrentThreadId();
        SetEvent(entered_);
        SOCltTask::reportCompletion();
        WaitForSingleObject(resume_, INFINITE);
    }
private:
    HANDLE entered_, resume_;
    std::atomic<DWORD>* worker_;
};
extern "C" int32_t opcl_da_test_ephemeral_worker(uint32_t guarded) {
    HANDLE entered = CreateEvent(nullptr, TRUE, FALSE, nullptr);
    HANDLE resume = CreateEvent(nullptr, TRUE, FALSE, nullptr);
    HANDLE destructor_entered = CreateEvent(nullptr, TRUE, FALSE, nullptr);
    if (!entered || !resume || !destructor_entered) {
        if (entered) CloseHandle(entered);
        if (resume) CloseHandle(resume);
        if (destructor_entered) CloseHandle(destructor_entered);
        return E_OUTOFMEMORY;
    }
    std::atomic<DWORD> worker(0), destroyed(0);
    std::atomic<int> running(-1);
    const DWORD caller = GetCurrentThreadId();
    LifetimeProbeServer* server = new LifetimeProbeServer(&destroyed, &running, destructor_entered);
    LifetimeProbeTask* task = new LifetimeProbeTask(server, entered, resume, &worker);
    HRESULT hr = E_FAIL;
    if (guarded) {
        {
            EphemeralContext context(server);
            hr = context.get()->executeTask(task);
            const DWORD entered_result = WaitForSingleObject(entered, 5000);
            task->release();
            if (entered_result != WAIT_OBJECT_0) hr = E_FAIL;
            SetEvent(resume);
        }
        CloseHandle(entered); CloseHandle(resume); CloseHandle(destructor_entered);
        return SUCCEEDED(hr) && destroyed == caller && running == 0 ? S_OK : E_FAIL;
    }
    SOCmnPointer<SOCltServer> unguarded(server);
    hr = unguarded->executeTask(task);
    if (FAILED(hr) || WaitForSingleObject(entered, 5000) != WAIT_OBJECT_0) {
        SetEvent(resume); task->release();
        unguarded->getWorkerThread()->shutdown();
        return E_FAIL;
    }
    HANDLE worker_handle = OpenThread(SYNCHRONIZE, FALSE, worker.load());
    if (!worker_handle) {
        SetEvent(resume); task->release();
        unguarded->getWorkerThread()->shutdown();
        return E_FAIL;
    }
    task->release();
    unguarded.release();
    SetEvent(resume);
    if (WaitForSingleObject(destructor_entered, 2000) != WAIT_OBJECT_0 ||
        destroyed != worker || running != 1) return E_FAIL;
    std::fprintf(stderr, "lifetime baseline self-destructor reached\n"); std::fflush(stderr);
    // The original ordering can destroy the worker on its own thread and
    // self-wait; the parent process enforces a timeout for this baseline.
    WaitForSingleObject(worker_handle, INFINITE);
    CloseHandle(worker_handle); CloseHandle(entered); CloseHandle(resume); CloseHandle(destructor_entered);
    return E_FAIL;
}
#endif
class ResolveServerTask : public SOCltTask {
public:
    ResolveServerTask(SOCltServer* context, const WCHAR* node, const WCHAR* progid,
                      uint32_t kind, OpclDaGuid* out)
        : SOCltTask(context), node_(node), progid_(progid), kind_(kind), out_(out), ran_(false) {}
    void execute() override try {
        ran_ = true;
        CLSID clsid{}; HRESULT hr = E_FAIL;
        if (kind_ == 0) {
            // Windows class lookup is a system service, not an OPC data-plane call.
            OPCL_DISCOVERY_TRACE("CLSIDFromProgID");
            hr = CLSIDFromProgID(progid_, &clsid);
            OPCL_DISCOVERY_TRACE("CLSIDFromProgID returned");
        } else {
            SOCmnPointer<SOCltServerBrowser> browser = new SOCltServerBrowser();
            browser->setNodeName(node_);
            if (browser->getCLSIDFromProgID(progid_, &clsid)) hr = S_OK;
            else { hr = browser->getLastError(nullptr); if (SUCCEEDED(hr)) hr = E_FAIL; }
        }
        if (SUCCEEDED(hr)) copy_guid(clsid, out_);
        setResult(hr);
    } catch (...) { setResult(E_UNEXPECTED); }
    bool ran() const { return ran_; }
private:
    const WCHAR* node_; const WCHAR* progid_; uint32_t kind_; OpclDaGuid* out_; bool ran_;
};
class DiscoverServerTask : public SOCltTask {
public:
    DiscoverServerTask(SOCltServer* context, const WCHAR* node, OpclDaServerRecord* out,
                       uint32_t capacity, uint32_t* count, uint32_t* fallback)
        : SOCltTask(context), node_(node), out_(out), capacity_(capacity), count_(count),
          fallback_(fallback), ran_(false) {}
    void execute() override try {
        ran_ = true;
        OPCL_DISCOVERY_TRACE("construct browser");
        SOCmnPointer<SOCltServerBrowser> browser = new SOCltServerBrowser();
        OPCL_DISCOVERY_TRACE("set node");
        browser->setNodeName(local_node(node_) ? L"" : node_);
        SOCmnList<CLSID> classes;
        const DWORD categories = SOCLT_BROWSE_SERVER_CAT_DA10A | SOCLT_BROWSE_SERVER_DA20 | SOCLT_BROWSE_SERVER_DA30;
        OPCL_DISCOVERY_TRACE("browse categories");
        BOOL ok = browser->browseServer(categories, classes);
        OPCL_DISCOVERY_TRACE("browse categories returned");
        if (!ok && local_node(node_)) {
            classes.destroy();
            OPCL_DISCOVERY_TRACE("browse registry fallback");
            ok = browser->browseServer(SOCLT_BROWSE_SERVER_REG_DA10A, classes);
            OPCL_DISCOVERY_TRACE("browse registry returned");
            *fallback_ = 1;
        }
        if (!ok) {
            HRESULT hr = browser->getLastError(nullptr);
            setResult(FAILED(hr) ? hr : E_FAIL); return;
        }
        const DWORD count = classes.getCount();
        if (count > capacity_ || count > OPCL_DA_MAX_SERVERS || (count && !out_)) {
            setResult(HRESULT_FROM_WIN32(ERROR_INSUFFICIENT_BUFFER)); return;
        }
        std::vector<OpclDaServerRecord> pending;
        pending.reserve(count);
        SOCmnListPosition pos = classes.getStartPosition();
        while (pos) {
            CLSID* id = classes.getNext(pos);
            if (!id) { setResult(E_FAIL); return; }
            SOCmnString progid, description;
            if (!browser->getServerProgId(*id, progid)) {
                HRESULT hr = browser->getLastError(nullptr);
                setResult(FAILED(hr) ? hr : E_FAIL); return;
            }
            OpclDaServerRecord record{};
            copy_guid(*id, &record.clsid);
            HRESULT hr = wide_to_utf8((LPCTSTR)progid, record.progid, OPCL_DA_PROGID_BYTES, &record.progid_len);
            if (FAILED(hr)) { setResult(hr); return; }
            if (browser->getServerDescription(*id, description)) {
                hr = wide_to_utf8((LPCTSTR)description, record.description, OPCL_DA_DESCRIPTION_BYTES, &record.description_len);
                if (FAILED(hr)) { setResult(hr); return; }
            } else record.flags |= 1;
            pending.push_back(record);
        }
        if (!pending.empty()) std::memcpy(out_, pending.data(), pending.size() * sizeof(OpclDaServerRecord));
        *count_ = static_cast<uint32_t>(pending.size());
        setResult(S_OK);
    } catch (...) { setResult(E_UNEXPECTED); }
    bool ran() const { return ran_; }
private:
    const WCHAR* node_; OpclDaServerRecord* out_; uint32_t capacity_;
    uint32_t* count_; uint32_t* fallback_; bool ran_;
};
extern "C" int32_t opcl_da_resolve(OpclDaHost* host, const char* node, const char* progid,
                                     uint32_t kind, OpclDaGuid* out) try {
    if (!host || !out || kind > 1) return E_INVALIDARG;
    *out = {};
    std::vector<WCHAR> n, p;
    if (!to_wide(node, n) || !to_wide(progid, p)) return E_INVALIDARG;
    OPCL_DISCOVERY_TRACE("construct resolve context");
    EphemeralContext context(new SOCltServer());
    ResolveServerTask* task = new (std::nothrow) ResolveServerTask(context.get(), n.data(), p.data(), kind, out);
    if (!task) return E_OUTOFMEMORY;
    OPCL_DISCOVERY_TRACE("execute resolve task");
    const HRESULT hr = context.get()->executeTask(task); const bool ran = task->ran(); task->release();
    OPCL_DISCOVERY_TRACE("resolve task returned");
    return ran || FAILED(hr) ? hr : E_UNEXPECTED;
} catch (...) { return E_UNEXPECTED; }
extern "C" int32_t opcl_da_discover(OpclDaHost* host, const char* node,
                                      OpclDaServerRecord* out, uint32_t capacity,
                                      uint32_t* count, uint32_t* fallback) try {
    if (!count || !fallback) return E_INVALIDARG;
    *count = 0; *fallback = 0;
    if (!host || capacity > OPCL_DA_MAX_SERVERS || (capacity && !out)) return E_INVALIDARG;
    std::vector<WCHAR> n;
    if (!to_wide(node, n)) return E_INVALIDARG;
    OPCL_DISCOVERY_TRACE("construct discovery context");
    EphemeralContext context(new SOCltServer());
    DiscoverServerTask* task = new (std::nothrow) DiscoverServerTask(context.get(), n.data(), out, capacity, count, fallback);
    if (!task) return E_OUTOFMEMORY;
    OPCL_DISCOVERY_TRACE("execute discovery task");
    const HRESULT hr = context.get()->executeTask(task); const bool ran = task->ran(); task->release();
    OPCL_DISCOVERY_TRACE("discovery task returned");
    return ran || FAILED(hr) ? hr : E_UNEXPECTED;
} catch (...) { return E_UNEXPECTED; }

// Re-resolve a canonical branch path against the current SDK browser when a
// fresh connection has no cached object for it. Never reuse another session's object.
static HRESULT resolve_branch(OpclDaSession* session, const char* path,
                              SOCmnPointer<SODaCNameSpaceBrowser>& browser,
                              SOCltBrowseObject** out) {
    browser = session->server->getNameSpaceBrowser();
    if (!browser.isNotNull()) return E_NOINTERFACE;
    browser->setRetrieveItemID(TRUE);
    browser->setMaxElements(OPCL_DA_MAX_BROWSE);
    SOCltBrowseObject* parent = browser;
    const std::string full(path);
    std::string prefix;
    size_t start = 0;
    while (start < full.size()) {
        const size_t end = full.find('\x1f', start);
        const std::string component = full.substr(start, end == std::string::npos ? end : end - start);
        if (component.empty()) return E_INVALIDARG;
        if (!prefix.empty()) prefix.push_back('\x1f');
        prefix += component;
        auto existing = session->branches.find(prefix);
        if (existing != session->branches.end()) {
            parent = existing->second.p;
            browser = static_cast<SODaCNode*>(parent)->getNameSpaceBrowser();
        } else {
            if (session->branches.size() >= 5000) return HRESULT_FROM_WIN32(ERROR_INSUFFICIENT_BUFFER);
            browser->setContinuationPoint(_T(""));
            SOCmnList<SOCltBrowseObject> children;
            if (!parent->expand(SOCMNOBJECT_TYPE_NODE, children)) return E_FAIL;
            LPCTSTR continuation = browser->getContinuationPoint();
            if ((continuation && *continuation) || children.getCount() >= OPCL_DA_MAX_BROWSE)
                return HRESULT_FROM_WIN32(ERROR_INSUFFICIENT_BUFFER);
            SOCltBrowseObject* found = nullptr;
            SOCmnListPosition pos = children.getStartPosition();
            while (pos) {
                SOCltBrowseObject* child = children.getNext(pos);
                if (!child || !child->is(SOCMNOBJECT_TYPE_NODE)) continue;
                SOCmnString name = child->getDisplayName();
                char buffer[OPCL_DA_BROWSE_NAME_BYTES]{}; uint32_t length = 0;
                const HRESULT hr = wide_to_utf8((LPCTSTR)name, buffer, sizeof(buffer), &length);
                if (FAILED(hr)) return hr;
                if (component == std::string(buffer, length)) {
                    if (found) return E_FAIL;
                    found = child;
                }
            }
            if (!found) return E_INVALIDARG;
            auto inserted = session->branches.emplace(prefix, BrowseRef(found));
            parent = inserted.first->second.p;
            browser = static_cast<SODaCNode*>(parent)->getNameSpaceBrowser();
        }
        if (!parent || !browser.isNotNull()) return E_FAIL;
        if (end == std::string::npos) break;
        start = end + 1;
        if (start == full.size()) return E_INVALIDARG;
    }
    *out = parent;
    return S_OK;
}
class BrowseTask : public SOCltTask {
public:
    BrowseTask(OpclDaSession* session, const char* path, OpclDaBrowseRecord* out,
               uint32_t capacity, uint32_t* count)
        : SOCltTask(session->group), session_(session), path_(path), out_(out),
          capacity_(capacity), count_(count), ran_(false) {}
    void execute() override try {
        ran_ = true;
        SOCmnPointer<SODaCNameSpaceBrowser> browser;
        SOCltBrowseObject* parent = nullptr;
        const HRESULT resolved = resolve_branch(session_, path_, browser, &parent);
        if (FAILED(resolved)) { setResult(resolved); return; }
        browser->setRetrieveItemID(TRUE);
        browser->setMaxElements(OPCL_DA_MAX_BROWSE);
        browser->setContinuationPoint(_T(""));
        SOCmnList<SOCltBrowseObject> children;
        if (!parent->expand(SOCMNOBJECT_TYPE_NODE | SOCMNOBJECT_TYPE_TAG | SOCMNOBJECT_TYPE_ITEM, children)) {
            setResult(E_FAIL); return;
        }
        LPCTSTR continuation = browser->getContinuationPoint();
        if ((continuation && *continuation) || children.getCount() > capacity_ ||
            children.getCount() >= OPCL_DA_MAX_BROWSE) {
            setResult(HRESULT_FROM_WIN32(ERROR_INSUFFICIENT_BUFFER)); return;
        }
        std::vector<OpclDaBrowseRecord> pending;
        std::vector<std::pair<std::string, SOCltBrowseObject*>> branches;
        pending.reserve(children.getCount());
        SOCmnListPosition position = children.getStartPosition();
        while (position) {
            SOCltBrowseObject* child = children.getNext(position);
            if (!child) { setResult(E_FAIL); return; }
            OpclDaBrowseRecord record{};
            SOCmnString name = child->getDisplayName();
            HRESULT hr = wide_to_utf8((LPCTSTR)name, record.name, OPCL_DA_BROWSE_NAME_BYTES, &record.name_len);
            if (FAILED(hr)) { setResult(hr); return; }
            record.is_branch = child->is(SOCMNOBJECT_TYPE_NODE) ? 1 : 0;
            if (record.is_branch) {
                std::string component(record.name, record.name_len);
                if (component.empty()) { setResult(E_FAIL); return; }
                if (component.find('\x1f') != std::string::npos) { setResult(E_INVALIDARG); return; }
                std::string id(path_);
                if (!id.empty()) id.push_back('\x1f');
                id += component;
                if (id.size() > OPCL_DA_BROWSE_ID_BYTES) { setResult(HRESULT_FROM_WIN32(ERROR_INSUFFICIENT_BUFFER)); return; }
                record.node_id_len = static_cast<uint32_t>(id.size());
                if (!id.empty()) std::memcpy(record.node_id, id.data(), id.size());
                branches.emplace_back(id, child);
            } else {
                SOCmnString id = child->getItemID();
                hr = wide_to_utf8((LPCTSTR)id, record.node_id, OPCL_DA_BROWSE_ID_BYTES, &record.node_id_len);
                if (FAILED(hr) || !record.node_id_len) { setResult(FAILED(hr) ? hr : E_FAIL); return; }
            }
            pending.push_back(record);
            // DA3 browse elements can be both branches and readable items.
            // Return both views rather than silently dropping the item role.
            if (record.is_branch && static_cast<SODaCNode*>(child)->isItem()) {
                OpclDaBrowseRecord item = record;
                item.is_branch = 0;
                SOCmnString id = child->getItemID();
                hr = wide_to_utf8((LPCTSTR)id, item.node_id, OPCL_DA_BROWSE_ID_BYTES, &item.node_id_len);
                if (FAILED(hr) || !item.node_id_len) { setResult(FAILED(hr) ? hr : E_FAIL); return; }
                pending.push_back(item);
            }
        }
        if (pending.size() > capacity_ || pending.size() > OPCL_DA_MAX_BROWSE) {
            setResult(HRESULT_FROM_WIN32(ERROR_INSUFFICIENT_BUFFER)); return;
        }
        size_t new_branches = 0;
        std::unordered_set<std::string> names;
        for (const auto& entry : branches) {
            if (!names.insert(entry.first).second) { setResult(E_FAIL); return; }
            if (session_->branches.find(entry.first) == session_->branches.end()) ++new_branches;
        }
        if (new_branches > 5000 || session_->branches.size() > 5000 - new_branches) {
            setResult(HRESULT_FROM_WIN32(ERROR_INSUFFICIENT_BUFFER)); return;
        }
        for (const auto& entry : branches) {
            auto it = session_->branches.find(entry.first);
            if (it == session_->branches.end()) session_->branches.emplace(entry.first, BrowseRef(entry.second));
            else it->second = BrowseRef(entry.second);
        }
        if (!pending.empty()) std::memcpy(out_, pending.data(), pending.size() * sizeof(OpclDaBrowseRecord));
        *count_ = static_cast<uint32_t>(pending.size());
        setResult(S_OK);
    } catch (...) { setResult(E_UNEXPECTED); }
    bool ran() const { return ran_; }
private:
    OpclDaSession* session_; const char* path_; OpclDaBrowseRecord* out_;
    uint32_t capacity_; uint32_t* count_; bool ran_;
};
class ClearBrowseTask : public SOCltTask {
public:
    explicit ClearBrowseTask(OpclDaSession* session)
        : SOCltTask(session->group), session_(session), ran_(false) {}
    void execute() override try { ran_ = true; session_->branches.clear(); setResult(S_OK); }
    catch (...) { setResult(E_UNEXPECTED); }
    bool ran() const { return ran_; }
private:
    OpclDaSession* session_; bool ran_;
};
static HRESULT clear_browse(OpclDaSession* session) {
    if (session->branches.empty()) return S_OK;
    ClearBrowseTask* task = new (std::nothrow) ClearBrowseTask(session);
    if (!task) return E_OUTOFMEMORY;
    const HRESULT hr = session->group->executeTask(task); const bool ran = task->ran(); task->release();
    return ran || FAILED(hr) ? hr : E_UNEXPECTED;
}
extern "C" int32_t opcl_da_browse(OpclDaSession* s, const char* path, OpclDaBrowseRecord* out,
                                    uint32_t capacity, uint32_t* count) try {
    if (!count) return E_INVALIDARG;
    *count = 0;
    if (!s || !path || capacity > OPCL_DA_MAX_BROWSE || (capacity && !out)) return E_INVALIDARG;
    std::vector<WCHAR> validated;
    if (!to_wide(path, validated)) return E_INVALIDARG;
    BrowseTask* task = new (std::nothrow) BrowseTask(s, path, out, capacity, count);
    if (!task) return E_OUTOFMEMORY;
    const HRESULT hr = s->group->executeTask(task); const bool ran = task->ran(); task->release();
    return ran || FAILED(hr) ? hr : E_UNEXPECTED;
} catch (...) { return E_UNEXPECTED; }
