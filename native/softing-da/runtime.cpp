#include "stdafx.h"
#include "runtime.h"
#include <atomic>
#include <new>
#include <unordered_set>

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
struct OpclDaItem { SODaCItem* sdk; };
struct OpclDaSession {
    OpclDaHost* host;
    SODaCServer* server;
    SODaCGroup* group;
    std::unordered_set<OpclDaItem*> items;
};
static std::atomic<bool> host_exists(false);
static bool ascii(const char* s) {
    if (!s) return false;
    for (; *s; ++s) if (static_cast<unsigned char>(*s) >= 128) return false;
    return true;
}
static bool owns(OpclDaSession* s, OpclDaItem* i) {
    return s && i && s->items.find(i) != s->items.end();
}
extern "C" int32_t opcl_da_host_create(OpclDaHost** out) {
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
    OpclDaHost* host = new (std::nothrow) OpclDaHost(entry);
    if (!host) {
        entry->terminate(); entry->release(); exitSOCmn(); host_exists.store(false);
        return E_OUTOFMEMORY;
    }
    *out = host; return S_OK;
}
extern "C" int32_t opcl_da_host_destroy(OpclDaHost* host) {
    if (!host || host->owner_thread != GetCurrentThreadId()) return E_INVALIDARG;
    if (host->sessions.load() != 0) return E_PENDING;
    const BOOL ok = host->entry->terminate();
    host->entry->release(); delete host; exitSOCmn(); host_exists.store(false);
    return ok ? S_OK : E_FAIL;
}
extern "C" int32_t opcl_da_session_open(OpclDaHost* host, const char* name, const char* node,
                                           const OpclDaGuid* id, OpclDaSession** out) {
    if (!out) return E_INVALIDARG;
    *out = nullptr;
    if (!host || !id || !ascii(name) || !ascii(node)) return E_INVALIDARG;
    GUID clsid{id->data1, id->data2, id->data3, {}};
    for (unsigned i = 0; i != 8; ++i) clsid.Data4[i] = id->data4[i];
    SODaCServer* server = host->entry->addServer(name, node, clsid);
    if (!server) return E_OUTOFMEMORY;
    HRESULT hr = server->connect(FALSE);
    if (SUCCEEDED(hr) && !server->isConnected()) hr = E_FAIL;
    SODaCGroup* group = SUCCEEDED(hr) ? server->addGroup() : nullptr;
    if (SUCCEEDED(hr) && !group) hr = E_OUTOFMEMORY;
    if (SUCCEEDED(hr)) {
        // Keep the group active for cache reads while leaving scheduling to
        // Engine: this SDK mode does not install its cyclic sync-read task.
        group->setDataRetrieval(SODaCGroup::drPolledRefresh);
        hr = group->start(FALSE);
        if (SUCCEEDED(hr) && !group->isStarted()) hr = E_FAIL;
    }
    if (FAILED(hr)) {
        if (group) { group->disconnect(); server->removeBranch(group); group->release(); }
        server->disconnect(); host->entry->removeBranch(server); server->release();
        return hr;
    }
    OpclDaSession* session = new (std::nothrow) OpclDaSession{host, server, group, {}};
    if (!session) {
        group->disconnect(); server->removeBranch(group); group->release();
        server->disconnect(); host->entry->removeBranch(server); server->release();
        return E_OUTOFMEMORY;
    }
    ++host->sessions; *out = session; return S_OK;
}
extern "C" int32_t opcl_da_item_add(OpclDaSession* s, const char* id, OpclDaItem** out,
                                     uint16_t* type, uint32_t* rights) {
    if (!out || !type || !rights) return E_INVALIDARG;
    *out = nullptr; *type = 0; *rights = 0;
    if (!s || !ascii(id)) return E_INVALIDARG;
    SODaCItem* sdk = s->group->addItem(id);
    if (!sdk) return E_OUTOFMEMORY;
    HRESULT hr = sdk->start(FALSE);
    if (SUCCEEDED(hr) && !sdk->isStarted()) hr = E_FAIL;
    if (FAILED(hr)) { s->group->removeLeaf(sdk); sdk->release(); return hr; }
    OpclDaItem* item = new (std::nothrow) OpclDaItem{sdk};
    if (!item) { sdk->disconnect(); s->group->removeLeaf(sdk); sdk->release(); return E_OUTOFMEMORY; }
    try { s->items.insert(item); }
    catch (...) { sdk->disconnect(); s->group->removeLeaf(sdk); sdk->release(); delete item; return E_OUTOFMEMORY; }
    *type = sdk->getNativeDatatype(); *rights = sdk->getAccessRights(); *out = item;
    return S_OK;
}
extern "C" int32_t opcl_da_item_remove(OpclDaSession* s, OpclDaItem* item) {
    if (!owns(s, item)) return E_INVALIDARG;
    s->items.erase(item);
    const HRESULT hr = item->sdk->disconnect();
    const BOOL removed = s->group->removeLeaf(item->sdk);
    item->sdk->release(); delete item;
    return FAILED(hr) ? hr : (removed ? S_OK : E_FAIL);
}
extern "C" int32_t opcl_da_session_close(OpclDaSession* s) {
    if (!s) return E_INVALIDARG;
    HRESULT result = S_OK;
    while (!s->items.empty()) {
        const HRESULT hr = opcl_da_item_remove(s, *s->items.begin());
        if (FAILED(hr)) result = hr;
    }
    HRESULT hr = s->group->disconnect(); if (FAILED(hr)) result = hr;
    if (!s->server->removeBranch(s->group)) result = E_FAIL;
    s->group->release();
    hr = s->server->disconnect(); if (FAILED(hr)) result = hr;
    if (!s->host->entry->removeBranch(s->server)) result = E_FAIL;
    s->server->release(); --s->host->sessions; delete s;
    return result;
}
// Dispatch synchronous SDK operations through its own per-server worker. The
// worker initializes COM; the session caller need not own a COM apartment.
class ReadI4Task : public SOCltTask {
public:
    ReadI4Task(SODaCGroup* group, SODaCItem* item, DWORD age, OpclDaReadI4* out)
        : SOCltTask(group), group_(group), item_(item), age_(age), out_(out) {}
    void execute() override {
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
    }
private:
    SODaCGroup* group_;
    SODaCItem* item_;
    DWORD age_;
    OpclDaReadI4* out_;
};
class WriteI4Task : public SOCltTask {
public:
    WriteI4Task(SODaCGroup* group, SODaCItem* item, int32_t v, int32_t* result)
        : SOCltTask(group), group_(group), item_(item), value_(v), result_(result) {}
    void execute() override {
        SOCmnVariant value; value.setI4(static_cast<LONG>(value_));
        SODaCItem* items[] = {item_}; HRESULT error = E_FAIL;
        const HRESULT hr = group_->syncWrite(1, items, &value, nullptr, nullptr, &error);
        *result_ = error; setResult(hr);
    }
private:
    SODaCGroup* group_;
    SODaCItem* item_;
    int32_t value_;
    int32_t* result_;
};
extern "C" int32_t opcl_da_read_i4(OpclDaSession* s, OpclDaItem* item, uint32_t max_age,
                                    OpclDaReadI4* out) {
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
}
extern "C" int32_t opcl_da_write_i4(OpclDaSession* s, OpclDaItem* item, int32_t v,
                                     int32_t* item_result) {
    if (!item_result || !owns(s, item)) return E_INVALIDARG;
    *item_result = E_FAIL;
    WriteI4Task* task = new (std::nothrow) WriteI4Task(s->group, item->sdk, v, item_result);
    if (!task) return E_OUTOFMEMORY;
    const HRESULT hr = s->group->executeTask(task);
    task->release();
    return hr;
}
