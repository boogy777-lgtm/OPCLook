#include "stdafx.h"
#include <cstdio>
#include <cstring>

HINSTANCE getInstanceHandle() { return ::GetModuleHandle(nullptr); }
static const CLSID fixture_clsid = {0x7d4bd1b1, 0x7422, 0x48c0, {0xad, 0x58, 0x60, 0x1b, 0x70, 0x14, 0x49, 0xc6}};

// This is application tag behavior through the SDK's public extension point.
// SDK complete() performs cache updates and OPC request completion.
class FixtureTag : public SODaSTagDefault {
public:
    FixtureTag() : SODaSTagDefault(_T("Value")) {
        setNativeDatatype(VT_I4);
        setAccessRights(OPC_READABLE | OPC_WRITEABLE);
        setDeviceIOMode(SODaSItemTag::direct);
    }
    void handleRequest(SODaSRequest* request) override {
        if (request) request->complete();
    }
};

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
        SOCmnPointer<SODaSTag> tag = new FixtureTag;
        if (!root.isNotNull() || !root->addBranch(node) || !node->addLeaf(tag)) result = 14;
        if (result == 0) {
            SOCmnPointer<SODaSCache> cache = tag->getCache();
            SOCmnVariant initial(static_cast<LONG>(42));
            SOCmnDateTime now;
            now.now();
            if (!cache.isNotNull() || !cache->setValue(initial) || !cache->setQuality(OPC_QUALITY_GOOD) || !cache->setTimeStamp(now)) result = 15;
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
