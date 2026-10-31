#include "stdafx.h"
#include <atomic>
#include <cstdio>
#include <cstring>
#include <cstdint>
#include <memory>
#include <string>

HINSTANCE getInstanceHandle() { return ::GetModuleHandle(nullptr); }
static HANDLE isolation_entered = nullptr;
static HANDLE isolation_release = nullptr;
// SDK cache is owned until server stop; this counts writes on one fixture tag.
static SOCmnPointer<SODaSCache> exception_count_cache;
static std::atomic<unsigned> exception_write_count{0};
static SOCmnPointer<SODaSCache> device_read_count_cache;
static std::atomic<unsigned> device_read_count{0};
// Count only actual synchronous device callback requests, excluding the SDK
// server's own cyclic subscription refreshes and initial-notify requests.
static SOCmnPointer<SODaSCache> sync_device_read_count_cache;
static std::atomic<unsigned> sync_device_read_count{0};
// Request-creation count also sees cache-served Sync reads, unlike the device
// callback count. The SDK still creates and owns every request.
static SOCmnPointer<SODaSCache> sync_read_request_count_cache;
static std::atomic<unsigned> sync_read_request_count{0};

static const CLSID fixture_clsid = {0x7d4bd1b1, 0x7422, 0x48c0, {0xad, 0x58, 0x60, 0x1b, 0x70, 0x14, 0x49, 0xc6}};

// This is application tag behavior through the SDK's public extension point.
// SDK complete() performs cache updates and OPC request completion.
class FixtureTag : public SODaSTagDefault {
public:
    FixtureTag(LPCTSTR name, VARTYPE type, bool delayed = false, bool state = false, bool counted = false, bool count_reads = false, bool read_only = false, HRESULT read_result = S_OK) : SODaSTagDefault(name), delayed_(delayed), state_(state), counted_(counted), count_reads_(count_reads), read_result_(read_result) {
        setNativeDatatype(type);
        setAccessRights(read_only ? OPC_READABLE : OPC_READABLE | OPC_WRITEABLE);
        setDeviceIOMode(SODaSItemTag::direct);
    }
    void handleRequest(SODaSRequest* request) override {
        if (!request) return;
        if (request->getOperation() == SODaSRequest::read && FAILED(read_result_)) {
            request->complete(read_result_, FALSE, OPC_QUALITY_UNCERTAIN, FALSE, FALSE);
            return;
        }
        if (count_reads_ && request->getOperation() == SODaSRequest::read) {
            unsigned before = device_read_count.load();
            do {
                if (before >= 1000 || !device_read_count_cache.isNotNull()) {
                    request->complete(E_FAIL, FALSE, OPC_QUALITY_UNCERTAIN, FALSE, FALSE);
                    return;
                }
            } while (!device_read_count.compare_exchange_weak(before, before + 1));
            SOCmnVariant count;
            count.setI4(static_cast<LONG>(before + 1));
            if (!device_read_count_cache->setValue(count)) {
                request->complete(E_FAIL, FALSE, OPC_QUALITY_UNCERTAIN, FALSE, FALSE);
                return;
            }
            const auto type = request->getType();
            if (type == SODaSRequest::syncDevice || type == SODaSRequest::syncMaxAge) {
                unsigned sync_before = sync_device_read_count.load();
                do {
                    if (sync_before >= 1000 || !sync_device_read_count_cache.isNotNull()) {
                        request->complete(E_FAIL, FALSE, OPC_QUALITY_UNCERTAIN, FALSE, FALSE);
                        return;
                    }
                } while (!sync_device_read_count.compare_exchange_weak(sync_before, sync_before + 1));
                SOCmnVariant sync_count; sync_count.setI4(static_cast<LONG>(sync_before + 1));
                if (!sync_device_read_count_cache->setValue(sync_count)) {
                    request->complete(E_FAIL, FALSE, OPC_QUALITY_UNCERTAIN, FALSE, FALSE);
                    return;
                }
            }
        }
        if (delayed_ && request->getOperation() == SODaSRequest::write) {
            if (!isolation_entered || !isolation_release || !SetEvent(isolation_entered) ||
                WaitForSingleObject(isolation_release, 60000) != WAIT_OBJECT_0) {
                request->complete(E_FAIL, FALSE, OPC_QUALITY_UNCERTAIN, FALSE, FALSE);
                return;
            }
        }
        if (request->getOperation() == SODaSRequest::write) {
            SOCmnVariant value;
            if (!request->getWriteValue(&value)) {
                request->complete(E_FAIL, FALSE, OPC_QUALITY_UNCERTAIN, FALSE, FALSE);
                return;
            }
            if (counted_) {
                unsigned before = exception_write_count.load();
                do {
                    if (before >= 1000 || !exception_count_cache.isNotNull()) {
                        request->complete(E_FAIL, FALSE, OPC_QUALITY_UNCERTAIN, FALSE, FALSE);
                        return;
                    }
                } while (!exception_write_count.compare_exchange_weak(before, before + 1));
                SOCmnVariant count;
                count.setI4(static_cast<LONG>(before + 1));
                if (!exception_count_cache->setValue(count)) {
                    request->complete(E_FAIL, FALSE, OPC_QUALITY_UNCERTAIN, FALSE, FALSE);
                    return;
                }
            }
            if (state_) {
                SOCmnPointer<SODaSEntry> entry = getSODaSEntry();
                if (value.vt != VT_I4 || (value.lVal != 1 && value.lVal != 2) ||
                    !entry.isNotNull() || !entry->setServerState(static_cast<OPCSERVERSTATE>(value.lVal))) {
                    request->complete(E_INVALIDARG, FALSE, OPC_QUALITY_UNCERTAIN, FALSE, FALSE);
                    return;
                }
            }
            if (value.vt & VT_ARRAY) {
                // This fixture models a device that replaces the whole value.
                // SDK default updateValue instead merges array index ranges.
                SOCmnPointer<SODaSCache> cache = getCache();
                const HRESULT result = cache.isNotNull() && cache->setValue(value) ? S_OK : E_FAIL;
                request->complete(result, FALSE, OPC_QUALITY_UNCERTAIN, FALSE, FALSE);
                return;
            }
        }
        request->complete();
    }
private:
    bool delayed_;
    bool state_;
    bool counted_;
    bool count_reads_;
    HRESULT read_result_;
};

class FixtureCreator : public SODaSCreator {
public:
    SODaSRequest* createRequest(SODaSServer* server, SODaSItem* item, SODaSItemTag* tag,
                               BYTE operation, WORD type) override {
        SODaSRequest* request = SODaSCreator::createRequest(server, item, tag, operation, type);
        if (!request) return nullptr;
        if (tag && operation == SODaSRequest::read &&
            (type == SODaSRequest::syncCache || type == SODaSRequest::syncDevice || type == SODaSRequest::syncMaxAge) &&
            tag->getItemID() == _T("Fixture.DeviceValue")) {
            unsigned before = sync_read_request_count.load();
            do {
                if (before >= 1000 || !sync_read_request_count_cache.isNotNull()) { request->release(); return nullptr; }
            } while (!sync_read_request_count.compare_exchange_weak(before, before + 1));
            SOCmnVariant count; count.setI4(static_cast<LONG>(before + 1));
            if (!sync_read_request_count_cache->setValue(count)) { request->release(); return nullptr; }
        }
        return request;
    }
};

static bool add_tag(SODaSNode* node, LPCTSTR name, SOCmnVariant& value, bool delayed = false, bool state = false, bool counted = false,
                    SOCmnPointer<SODaSCache>* saved_cache = nullptr, bool count_reads = false, bool read_only = false,
                    WORD quality = OPC_QUALITY_GOOD, HRESULT read_result = S_OK) {
    SOCmnPointer<SODaSTag> tag = new FixtureTag(name, value.vt, delayed, state, counted, count_reads, read_only, read_result);
    if (!node->addLeaf(tag)) return false;
    SOCmnPointer<SODaSCache> cache = tag->getCache();
    if (saved_cache) *saved_cache = cache;
    SOCmnDateTime now;
    now.now();
    return cache.isNotNull() && cache->setValue(value) &&
           cache->setQuality(quality) && cache->setTimeStamp(now);
}

static bool make_array(SOCmnVariant& value, VARTYPE type, LONG lower, ULONG count) {
    SAFEARRAY* array = SOCmnVariant::safeArrayCreateVector(type, lower, count);
    if (!array) return false;
    value.clear();
    value.vt = static_cast<VARTYPE>(VT_ARRAY | type);
    value.parray = array;
    return true;
}

static bool fill_array(SOCmnVariant& value, const void* input, size_t bytes) {
    void* data = nullptr;
    const HRESULT access = SafeArrayAccessData(value.parray, &data);
    if (FAILED(access)) return false;
    if (!data && bytes) { SafeArrayUnaccessData(value.parray); return false; }
    if (bytes) std::memcpy(data, input, bytes);
    return SUCCEEDED(SafeArrayUnaccessData(value.parray));
}

static bool add_array_fixtures(SODaSNode* node) {
    SOCmnVariant value;
    const BYTE bytes[] = {0, 127, 255};
    if (!make_array(value, VT_UI1, 0, 3) || SafeArrayGetElemsize(value.parray) != sizeof(BYTE) ||
        !fill_array(value, bytes, sizeof(bytes)) || !add_tag(node, _T("Bytes"), value)) return false;
    const LONG ints[] = {-2, 0, 42};
    if (!make_array(value, VT_I4, 5, 3) || SafeArrayGetElemsize(value.parray) != sizeof(LONG) ||
        !fill_array(value, ints, sizeof(ints)) || !add_tag(node, _T("Ints"), value)) return false;
    const double floats[] = {-1.25, 0.5, 42.0};
    if (!make_array(value, VT_R8, -3, 3) || SafeArrayGetElemsize(value.parray) != sizeof(double) ||
        !fill_array(value, floats, sizeof(floats)) || !add_tag(node, _T("Float64Array"), value)) return false;
    if (!add_tag(node, _T("Float64ArrayReadOnly"), value, false, false, false, nullptr, false, true)) return false;
    const VARIANT_BOOL flags[] = {VARIANT_FALSE, VARIANT_TRUE, VARIANT_FALSE};
    if (!make_array(value, VT_BOOL, 0, 3) || SafeArrayGetElemsize(value.parray) != sizeof(VARIANT_BOOL) ||
        !fill_array(value, flags, sizeof(flags)) || !add_tag(node, _T("BoolArray"), value)) return false;
    const LONGLONG signed_values[] = {INT64_MIN, -9007199254740993LL, INT64_MAX};
    if (!make_array(value, VT_I8, 2, 3) || SafeArrayGetElemsize(value.parray) != sizeof(LONGLONG) ||
        !fill_array(value, signed_values, sizeof(signed_values)) || !add_tag(node, _T("Int64Array"), value)) return false;
    const ULONGLONG unsigned_values[] = {0, 9007199254740993ULL, UINT64_MAX};
    if (!make_array(value, VT_UI8, 0, 3) || SafeArrayGetElemsize(value.parray) != sizeof(ULONGLONG) ||
        !fill_array(value, unsigned_values, sizeof(unsigned_values)) || !add_tag(node, _T("UInt64Array"), value)) return false;
    // Valid SDK array deliberately exceeds the adapter's 256-element bound.
    // It exercises codec failure after a successful SDK device/cache read.
    LONG many[257]{};
    if (!make_array(value, VT_I4, 0, 257) ||
        !fill_array(value, many, sizeof(many)) || !add_tag(node, _T("LargeInts"), value)) return false;
    if (!make_array(value, VT_BSTR, 0, 2)) return false;
    SOCmnVariant element;
    element.setBSTR(_T("\u0420\u0423\u0413 \u4e2d\u6587"));
    LONG index = 0;
    if (FAILED(SafeArrayPutElement(value.parray, &index, element.bstrVal))) return false;
    element.setBSTR(_T(""));
    index = 1;
    if (FAILED(SafeArrayPutElement(value.parray, &index, element.bstrVal)) ||
        !add_tag(node, _T("Strings"), value)) return false;
    if (!make_array(value, VT_VARIANT, 0, 3)) return false;
    element.setI4(-42);
    index = 0;
    if (FAILED(SafeArrayPutElement(value.parray, &index, static_cast<LPVARIANT>(element)))) return false;
    element.setBSTR(_T("\u0420\u0423\u0413 \u4e2d\u6587"));
    index = 1;
    if (FAILED(SafeArrayPutElement(value.parray, &index, static_cast<LPVARIANT>(element)))) return false;
    const ULONGLONG inner[] = {2, 255};
    if (!make_array(element, VT_UI8, 0, 2) ||
        SafeArrayGetElemsize(element.parray) != sizeof(ULONGLONG) ||
        !fill_array(element, inner, sizeof(inner))) return false;
    index = 2;
    return SUCCEEDED(SafeArrayPutElement(value.parray, &index, static_cast<LPVARIANT>(element))) &&
           add_tag(node, _T("Mixed"), value);
}

static int run(int argc, char** argv) {
    SOCmnPointer<SODaSEntry> entry = getSODaSEntry();
    if (!entry.isNotNull()) return 10;
    // Nonzero STA mode is supported by this SDK. Its server worker initializes
    // its own MTA. The separate client SDK MTA-balance regression is N1's gate.
    if (!entry->initialize(SOSrvEntry::OutProc, fixture_clsid, getInstanceHandle(), COINIT_APARTMENTTHREADED)) return 11;
    entry->setRegisterInfo(_T("OPCLook.NativeDaFixture.1"), _T("OPCLook.NativeDaFixture"), _T("OPCLook local SDK interop fixture"));
    entry->setVendorInfo(_T("OPCLook test fixture using Softing OPC Classic SDK"));
    entry->setVersionInfo(1, 0, 0, 0);
    int result = 0;
    if (argc == 2 && std::strcmp(argv[1], "--register") == 0) {
        result = FAILED(entry->registerServer()) ? 12 : 0;
    } else if (argc == 2 && std::strcmp(argv[1], "--unregister") == 0) {
        result = FAILED(entry->unregisterServer()) ? 13 : 0;
    } else if (argc == 3 || argc == 4) {
        SOCmnPointer<FixtureCreator> creator = new FixtureCreator();
        if (!creator.isNotNull()) result = 22;
        else entry->setCreator(creator);
        if (argc == 4) {
            const std::string prefix(argv[3]);
            isolation_entered = CreateEventA(nullptr, TRUE, FALSE, (prefix + "-entered").c_str());
            if (!isolation_entered || GetLastError() == ERROR_ALREADY_EXISTS) result = 21;
            isolation_release = CreateEventA(nullptr, TRUE, FALSE, (prefix + "-release").c_str());
            if (!isolation_release || GetLastError() == ERROR_ALREADY_EXISTS) result = 21;
        }
        SOCmnPointer<SODaSNameSpaceRoot> root = entry->getNameSpaceRoot();
        SOCmnPointer<SODaSNode> node = new SODaSNodeDefault(_T("Fixture"));
        if (result == 0 && (!root.isNotNull() || !root->addBranch(node))) result = 14;
        if (result == 0) {
            SOCmnVariant value;
            value.setI4(42);
            bool tags = add_tag(node, _T("Value"), value);
            value.setI4(0);
            tags = add_tag(node, _T("ExceptionCount"), value, false, false, false, std::addressof(exception_count_cache)) && tags;
            tags = add_tag(node, _T("DeviceReadCount"), value, false, false, false, std::addressof(device_read_count_cache)) && tags;
            tags = add_tag(node, _T("SyncDeviceReadCount"), value, false, false, false, std::addressof(sync_device_read_count_cache)) && tags;
            tags = add_tag(node, _T("SyncReadRequestCount"), value, false, false, false, std::addressof(sync_read_request_count_cache)) && tags;
            value.setI4(42);
            tags = add_tag(node, _T("DeviceValue"), value, false, false, false, nullptr, true) && tags;
            tags = add_tag(node, _T("ReadDenied"), value, false, false, false, nullptr, false, false, OPC_QUALITY_GOOD, E_ACCESSDENIED) && tags;
            value.setI4(51);
            tags = add_tag(node, _T("ReadUncertain"), value, false, false, false, nullptr, false, false, OPC_QUALITY_UNCERTAIN) && tags;
            value.setI4(52);
            tags = add_tag(node, _T("ReadBad"), value, false, false, false, nullptr, false, false, OPC_QUALITY_BAD) && tags;
            value.setI4(42);
            tags = add_tag(node, _T("ExceptionValue"), value, false, false, true) && tags;
            if (argc == 4) tags = add_tag(node, _T("Delayed"), value, true) && tags;
            value.setI4(1);
            tags = add_tag(node, _T("ServerState"), value, false, true) && tags;
            value.setBOOL(true);
            tags = add_tag(node, _T("Bool"), value) && tags;
            value.setI8(static_cast<LONGLONG>(-9007199254740993LL));
            tags = add_tag(node, _T("Int64"), value) && tags;
            value.setUI8(static_cast<ULONGLONG>(18446744073709551600ULL));
            tags = add_tag(node, _T("UInt64"), value) && tags;
            value.setR8(12.5);
            tags = add_tag(node, _T("Float64"), value) && tags;
            value.setBSTR(_T("initial text"));
            tags = add_tag(node, _T("Text"), value) && tags;
            value.setBSTR(_T("\u0420\u0423\u0413 \u4e2d\u6587"));
            tags = add_tag(node, _T("Unicode"), value) && tags;
            value.setI4(314);
            tags = add_tag(node, _T("\u0422\u0435\u0433"), value) && tags;
            value.setR8(-1.25);
            value.vt = VT_DATE;
            tags = add_tag(node, _T("DateBeforeEpoch"), value) && tags;
            tags = add_array_fixtures(node) && tags;
            if (!tags) result = 15;
        }
        if (result == 0 && !entry->start()) result = 16;
        if (result == 0 && !entry->setServerState(OPC_STATUS_RUNNING)) result = 22;
        if (result == 0) {
            FILE* ready = nullptr;
            if (fopen_s(&ready, argv[1], "wb") != 0 || !ready) result = 17;
            else { std::fputs("SDK server process started\n", ready); std::fclose(ready); }
            const ULONGLONG started = GetTickCount64();
            while (result == 0 && GetFileAttributesA(argv[2]) == INVALID_FILE_ATTRIBUTES) {
                if (GetTickCount64() - started > 180000) { result = 18; break; }
                Sleep(25);
            }
        }
        entry->stop();
        exception_count_cache.release();
        device_read_count_cache.release();
        sync_device_read_count_cache.release();
        sync_read_request_count_cache.release();
        if (isolation_entered) CloseHandle(isolation_entered);
        if (isolation_release) CloseHandle(isolation_release);
    } else result = 19;
    if (!entry->terminate()) result = 20;
    entry.release();
    return result;
}
int main(int argc, char** argv) {
    initSOCmn();
    const int result = run(argc, argv);
    exitSOCmn();
    std::printf("DA fixture server exit=%d\n", result);
    return result;
}
