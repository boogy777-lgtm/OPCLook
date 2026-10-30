#include "stdafx.h"
#include "runtime.h"
#include <atomic>
#include <new>
#include <unordered_set>
#include <unordered_map>
#include <string>
#include <vector>
#include <cwchar>

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
    if (!host || !id) return E_INVALIDARG;
    std::vector<WCHAR> wide_name, wide_node;
    if (!to_wide(name, wide_name) || !to_wide(node, wide_node)) return E_INVALIDARG;
    GUID clsid{id->data1, id->data2, id->data3, {}};
    for (unsigned i = 0; i != 8; ++i) clsid.Data4[i] = id->data4[i];
    SODaCServer* server = host->entry->addServer(wide_name.data(), wide_node.data(), clsid);
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
    OpclDaSession* session = new (std::nothrow) OpclDaSession{host, server, group, {}, {}};
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
    if (!s) return E_INVALIDARG;
    std::vector<WCHAR> wide_id;
    if (!to_wide(id, wide_id)) return E_INVALIDARG;
    SODaCItem* sdk = s->group->addItem(wide_id.data());
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
static HRESULT clear_browse(OpclDaSession* s);
extern "C" int32_t opcl_da_session_close(OpclDaSession* s) try {
    if (!s) return E_INVALIDARG;
    const HRESULT cleared = clear_browse(s);
    if (FAILED(cleared)) return cleared;
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
    ReadScalarTask(SODaCGroup* group, SODaCItem* item, DWORD age, OpclDaScalar* out, char* text, uint32_t capacity)
        : SOCltTask(group), group_(group), item_(item), age_(age), out_(out), text_(text), capacity_(capacity) {}
    void execute() override try {
        SODaCItem* items[] = {item_}; SOCmnVariant value; SOCmnDateTime time;
        WORD quality = 0; HRESULT item_result = E_FAIL;
        const HRESULT service = group_->syncRead(1, items, age_, &value, &quality, &time, &item_result);
        FILETIME ft{}; time.get(&ft);
        out_->service_result = service; out_->item_result = item_result;
        out_->filetime_low = ft.dwLowDateTime; out_->filetime_high = ft.dwHighDateTime;
        out_->quality = quality; out_->vartype = value.vt;
        if (SUCCEEDED(service) && SUCCEEDED(item_result) && value.vt == VT_BSTR) {
            // SDK helper owns its copy. The original BSTR is only borrowed to
            // reject embedded NUL before it could be silently truncated.
            const UINT original = value.bstrVal ? SysStringLen(value.bstrVal) : 0;
            if (original > 65536) { setResult(HRESULT_FROM_WIN32(ERROR_INSUFFICIENT_BUFFER)); return; }
            SOCmnString storage; LPCTSTR wide = value.getBSTR(storage);
            const size_t length = wide ? std::wcslen(wide) : 0;
            if (length != original) { setResult(DISP_E_TYPEMISMATCH); return; }
            if (length) {
                const int needed = WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, wide,
                    static_cast<int>(length), nullptr, 0, nullptr, nullptr);
                if (needed <= 0) { setResult(E_INVALIDARG); return; }
                if (static_cast<uint32_t>(needed) > capacity_ || !text_) { setResult(HRESULT_FROM_WIN32(ERROR_INSUFFICIENT_BUFFER)); return; }
                if (WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, wide, static_cast<int>(length),
                    text_, needed, nullptr, nullptr) != needed) { setResult(E_INVALIDARG); return; }
                out_->text_len = static_cast<uint32_t>(needed);
            }
        } else if (SUCCEEDED(service) && SUCCEEDED(item_result)) (void)scalar_copy(value, out_);
        // The raw VARTYPE crosses the ABI even when no scalar conversion is
        // supported; Rust maps it to the common explicit Unsupported error.
        setResult(S_OK);
    } catch (...) { setResult(E_UNEXPECTED); }
private:
    SODaCGroup* group_; SODaCItem* item_; DWORD age_; OpclDaScalar* out_; char* text_; uint32_t capacity_;
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
                                        OpclDaScalar* out, char* text, uint32_t text_capacity) try {
    if (!out) return E_INVALIDARG;
    *out = {}; out->service_result = E_FAIL; out->item_result = E_FAIL;
    if (!owns(s, item)) return E_INVALIDARG;
    ReadScalarTask* task = new (std::nothrow) ReadScalarTask(s->group, item->sdk, max_age, out, text, text_capacity);
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
class ResolveServerTask : public SOCltTask {
public:
    ResolveServerTask(SODaCServer* context, const WCHAR* node, const WCHAR* progid,
                      uint32_t kind, OpclDaGuid* out)
        : SOCltTask(context), node_(node), progid_(progid), kind_(kind), out_(out), ran_(false) {}
    void execute() override try {
        ran_ = true;
        CLSID clsid{}; HRESULT hr = E_FAIL;
        if (kind_ == 0) {
            // Windows class lookup is a system service, not an OPC data-plane call.
            hr = CLSIDFromProgID(progid_, &clsid);
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
    DiscoverServerTask(SODaCServer* context, const WCHAR* node, OpclDaServerRecord* out,
                       uint32_t capacity, uint32_t* count, uint32_t* fallback)
        : SOCltTask(context), node_(node), out_(out), capacity_(capacity), count_(count),
          fallback_(fallback), ran_(false) {}
    void execute() override try {
        ran_ = true;
        SOCmnPointer<SOCltServerBrowser> browser = new SOCltServerBrowser();
        browser->setNodeName(local_node(node_) ? L"" : node_);
        SOCmnList<CLSID> classes;
        const DWORD categories = SOCLT_BROWSE_SERVER_CAT_DA10A | SOCLT_BROWSE_SERVER_DA20 | SOCLT_BROWSE_SERVER_DA30;
        BOOL ok = browser->browseServer(categories, classes);
        if (!ok && local_node(node_)) {
            classes.destroy();
            ok = browser->browseServer(SOCLT_BROWSE_SERVER_REG_DA10A, classes);
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
    SOCmnPointer<SODaCComServerDefault> context = new SODaCComServerDefault();
    ResolveServerTask* task = new (std::nothrow) ResolveServerTask(context, n.data(), p.data(), kind, out);
    if (!task) return E_OUTOFMEMORY;
    const HRESULT hr = context->executeTask(task); const bool ran = task->ran(); task->release();
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
    SOCmnPointer<SODaCComServerDefault> context = new SODaCComServerDefault();
    DiscoverServerTask* task = new (std::nothrow) DiscoverServerTask(context, n.data(), out, capacity, count, fallback);
    if (!task) return E_OUTOFMEMORY;
    const HRESULT hr = context->executeTask(task); const bool ran = task->ran(); task->release();
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
        if (!parent->expand(SOCMNOBJECT_TYPE_NODE | SOCMNOBJECT_TYPE_LEAF, children)) {
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
