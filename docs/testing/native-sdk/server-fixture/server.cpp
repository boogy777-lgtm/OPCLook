#include "stdafx.h"
#include <cstdio>
#include <cstring>

HINSTANCE getInstanceHandle() { return ::GetModuleHandle(nullptr); }
static const CLSID fixture_clsid = {0x7d4bd1b1, 0x7422, 0x48c0, {0xad, 0x58, 0x60, 0x1b, 0x70, 0x14, 0x49, 0xc6}};

// This is application tag behavior through the SDK's public extension point.
// SDK complete() performs cache updates and OPC request completion.
class FixtureTag : public SODaSTagDefault {
public:
    FixtureTag(LPCTSTR name, VARTYPE type) : SODaSTagDefault(name) {
        setNativeDatatype(type);
        setAccessRights(OPC_READABLE | OPC_WRITEABLE);
        setDeviceIOMode(SODaSItemTag::direct);
    }
    void handleRequest(SODaSRequest* request) override {
        if (!request) return;
        if (request->getOperation() == SODaSRequest::write) {
            SOCmnVariant value;
            if (!request->getWriteValue(&value)) {
                request->complete(E_FAIL, FALSE, OPC_QUALITY_UNCERTAIN, FALSE, FALSE);
                return;
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
};

static bool add_tag(SODaSNode* node, LPCTSTR name, SOCmnVariant& value) {
    SOCmnPointer<SODaSTag> tag = new FixtureTag(name, value.vt);
    if (!node->addLeaf(tag)) return false;
    SOCmnPointer<SODaSCache> cache = tag->getCache();
    SOCmnDateTime now;
    now.now();
    return cache.isNotNull() && cache->setValue(value) &&
           cache->setQuality(OPC_QUALITY_GOOD) && cache->setTimeStamp(now);
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
    } else if (argc == 3) {
        SOCmnPointer<SODaSNameSpaceRoot> root = entry->getNameSpaceRoot();
        SOCmnPointer<SODaSNode> node = new SODaSNodeDefault(_T("Fixture"));
        if (!root.isNotNull() || !root->addBranch(node)) result = 14;
        if (result == 0) {
            SOCmnVariant value;
            value.setI4(42);
            bool tags = add_tag(node, _T("Value"), value);
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
