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
extern "C" int32_t opcl_da_host_create(OpclDaHost** out) try {
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
} catch (...) { return E_UNEXPECTED; }
extern "C" int32_t opcl_da_host_destroy(OpclDaHost* host) try {
    if (!host || host->owner_thread != GetCurrentThreadId()) return E_INVALIDARG;
    if (host->sessions.load() != 0) return E_PENDING;
    const BOOL ok = host->entry->terminate();
    host->entry->release(); delete host; exitSOCmn(); host_exists.store(false);
    return ok ? S_OK : E_FAIL;
} catch (...) { return E_UNEXPECTED; }
extern "C" int32_t opcl_da_session_open(OpclDaHost* host, const char* name, const char* node,
                                           const OpclDaGuid* id, OpclDaSession** out) try {
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
} catch (...) { return E_UNEXPECTED; }
extern "C" int32_t opcl_da_item_add(OpclDaSession* s, const char* id, OpclDaItem** out,
                                     uint16_t* type, uint32_t* rights) try {
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
} catch (...) { return E_UNEXPECTED; }
extern "C" int32_t opcl_da_item_remove(OpclDaSession* s, OpclDaItem* item) try {
    if (!owns(s, item)) return E_INVALIDARG;
    s->items.erase(item);
    const HRESULT hr = item->sdk->disconnect();
    const BOOL removed = s->group->removeLeaf(item->sdk);
    item->sdk->release(); delete item;
    return FAILED(hr) ? hr : (removed ? S_OK : E_FAIL);
} catch (...) { return E_UNEXPECTED; }
extern "C" int32_t opcl_da_session_close(OpclDaSession* s) try {
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
} catch (...) { return E_UNEXPECTED; }
// Dispatch synchronous SDK operations through its own per-server worker. The
// worker initializes COM; the session caller need not own a COM apartment.
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
static HRESULT scalar_copy(const SOCmnVariant& value, OpclDaScalar* out) {
    int64_t signed_value = 0; uint64_t unsigned_value = 0; double float_value = 0;
    switch (value.vt) {
    case VT_EMPTY: put_bits(out, 0); return S_OK;
    case VT_I1: signed_value = value.cVal; break;
    case VT_I2: signed_value = value.iVal; break;
    case VT_I4: signed_value = value.lVal; break;
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
class ReadScalarTask : public SOCltTask {
public:
    ReadScalarTask(SODaCGroup* group, SODaCItem* item, DWORD age, OpclDaScalar* out)
        : SOCltTask(group), group_(group), item_(item), age_(age), out_(out) {}
    void execute() override try {
        SODaCItem* items[] = {item_}; SOCmnVariant value; SOCmnDateTime time;
        WORD quality = 0; HRESULT item_result = E_FAIL;
        const HRESULT service = group_->syncRead(1, items, age_, &value, &quality, &time, &item_result);
        FILETIME ft{}; time.get(&ft);
        out_->service_result = service; out_->item_result = item_result;
        out_->filetime_low = ft.dwLowDateTime; out_->filetime_high = ft.dwHighDateTime;
        out_->quality = quality; out_->vartype = value.vt;
        if (SUCCEEDED(service) && SUCCEEDED(item_result)) (void)scalar_copy(value, out_);
        // The raw VARTYPE crosses the ABI even when no scalar conversion is
        // supported; Rust maps it to the common explicit Unsupported error.
        setResult(S_OK);
    } catch (...) { setResult(E_UNEXPECTED); }
private:
    SODaCGroup* group_; SODaCItem* item_; DWORD age_; OpclDaScalar* out_;
};
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
        *result_ = error; setResult(service);
    } catch (...) { setResult(E_UNEXPECTED); }
    bool dispatched() const { return dispatched_; }
private:
    SODaCGroup* group_; SODaCItem* item_; uint16_t vt_; uint32_t low_, high_; int32_t* result_; bool dispatched_;
};
extern "C" int32_t opcl_da_read_scalar(OpclDaSession* s, OpclDaItem* item, uint32_t max_age,
                                        OpclDaScalar* out) try {
    if (!out) return E_INVALIDARG;
    *out = {}; out->service_result = E_FAIL; out->item_result = E_FAIL;
    if (!owns(s, item)) return E_INVALIDARG;
    ReadScalarTask* task = new (std::nothrow) ReadScalarTask(s->group, item->sdk, max_age, out);
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
