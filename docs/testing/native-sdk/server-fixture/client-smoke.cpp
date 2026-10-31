#include "stdafx.h"
#include <cstdio>

HINSTANCE getInstanceHandle() { return ::GetModuleHandle(nullptr); }
static const CLSID fixture_clsid = {0x7d4bd1b1, 0x7422, 0x48c0, {0xad, 0x58, 0x60, 0x1b, 0x70, 0x14, 0x49, 0xc6}};
static int run() {
    SOCmnPointer<SODaCEntry> entry = getSODaCEntry();
    if (!entry.isNotNull() || !entry->initialize(COINIT_APARTMENTTHREADED)) return 10;
    int result = 0;
    {
        SOCmnPointer<SODaCServer> server = entry->addServer(_T("fixture"), _T(""), fixture_clsid, CLSCTX_LOCAL_SERVER);
        SOCmnPointer<SODaCGroup> group;
        SOCmnPointer<SODaCItem> item;
        if (server.isNotNull()) group = server->addGroup(_T("interop"), 100);
        if (group.isNotNull()) item = group->addItem(_T("Fixture.Value"));
        if (!item.isNotNull() || FAILED(server->connect(TRUE))) result = 11;
        if (result == 0) {
            SODaCItem* items[] = {item};
            SOCmnVariant value;
            WORD quality = 0;
            SOCmnDateTime timestamp;
            HRESULT item_result = E_FAIL;
            HRESULT service = group->syncRead(1, items, SODAC_MAXAGE_DEVICE, &value, &quality, &timestamp, &item_result);
            FILETIME time{}; timestamp.get(&time);
            if (FAILED(service) || FAILED(item_result) || value.vt != VT_I4 || value.lVal != 42 ||
                (quality & 0xc0) != OPC_QUALITY_GOOD || (time.dwLowDateTime == 0 && time.dwHighDateTime == 0)) result = 12;
            SOCmnVariant written(static_cast<LONG>(73));
            if (result == 0 && (FAILED(group->syncWrite(1, items, &written, nullptr, nullptr, &item_result)) || FAILED(item_result))) result = 13;
            if (result == 0 && (FAILED(group->syncRead(1, items, SODAC_MAXAGE_DEVICE, &value, &quality, &timestamp, &item_result)) ||
                               FAILED(item_result) || value.vt != VT_I4 || value.lVal != 73)) result = 14;
        }
        if (server.isNotNull()) server->disconnect();
        item.release(); group.release(); server.release();
    }
    entry->disconnect();
    if (!entry->terminate()) result = 15;
    entry.release();
    return result;
}
int main() {
    initSOCmn();
    const int result = run();
    exitSOCmn();
    std::printf("Independent SDK processes actual COM Read/Write: %s (%d)\n", result ? "FAIL" : "PASS", result);
    return result;
}
