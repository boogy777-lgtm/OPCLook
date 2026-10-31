#!/usr/bin/env python3
"""Exercise actual version-route live driver with deterministic SDK/ABI stubs."""
from pathlib import Path
import subprocess
import tempfile
root = Path(__file__).resolve().parents[3]
s = (root / 'docs/testing/native-sdk/core-probe/main.cpp').read_text()
a = s.index('static int live_version_routes_fixture(')
b = s.index('\nstatic int live_async_fixture()', a)
driver = s[a:b]
runtime = (root / 'native/softing-da/runtime.cpp').read_text()
assert 'server->forceOPCVersion(static_cast<WORD>(test_version))' in runtime
assert runtime.index('server->forceOPCVersion(static_cast<WORD>(test_version))') < runtime.index('HRESULT hr = server->connect(FALSE)')
assert 'server->getForcedOPCVersion() != test_version' in runtime
assert 'server->getSupportedOPCVersion() != test_version' in runtime
assert 'revised_rate_ms, out, false)' in runtime
pre = r'''
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <cassert>
using HRESULT=int32_t; constexpr int S_OK=0, E_FAIL=-1, VT_I4=3;
constexpr uint32_t SODAC_VERSION_OPC_DAV20=0x200, SODAC_VERSION_OPC_DAV30=0x300;
#define SUCCEEDED(x) ((x)>=0)
#define FAILED(x) ((x)<0)
struct OpclDaGuid {uint32_t a; uint16_t b,c; uint8_t d[8];};
struct OpclDaHost {}; struct OpclDaSession {}; struct OpclDaItem {int kind;};
struct OpclDaReadI4 {int32_t service_result=0,item_result=0,value=0;uint32_t filetime_low=1,filetime_high=0;uint16_t quality=192,vartype=3;};
OpclDaHost host; OpclDaSession session; OpclDaItem items[4]{{0},{1},{2},{3}};
int counts[3]={}; uint32_t selected; bool wrong=false;
int opcl_da_host_create(OpclDaHost** p){*p=&host;return S_OK;}
int opcl_da_test_version_open(OpclDaHost*,const char*,const char*,const OpclDaGuid*,uint32_t v,uint32_t*,OpclDaSession** p){selected=v; *p=&session;return S_OK;}
int opcl_da_item_add(OpclDaSession*,const char*,uint32_t n,uint32_t,OpclDaItem** p,uint16_t*,uint32_t*){*p=&items[n-1];return S_OK;}
int opcl_da_read_i4(OpclDaSession*,OpclDaItem* item,uint32_t age,OpclDaReadI4* out){
 if(item->kind) out->value=counts[item->kind-1];
 else {out->value=42; ++counts[2]; if(selected==SODAC_VERSION_OPC_DAV20 || wrong) ++counts[age==UINT32_MAX?0:1];}
 return S_OK;
}
int opcl_da_session_close(OpclDaSession*){return S_OK;} int opcl_da_host_destroy(OpclDaHost*){return S_OK;}
'''
post = r'''
int main(){ assert(live_version_routes_fixture(SODAC_VERSION_OPC_DAV20)==0); assert(live_version_routes_fixture(SODAC_VERSION_OPC_DAV30)==0); wrong=true; assert(live_version_routes_fixture(SODAC_VERSION_OPC_DAV30)==72); }
'''
with tempfile.TemporaryDirectory() as d:
    src = Path(d) / 'test.cpp'; exe = Path(d) / 'test'; src.write_text(pre + driver + post)
    subprocess.run(['c++', '-std=c++17', '-Wall', '-Wextra', '-Werror', str(src), '-o', str(exe)], check=True)
    subprocess.run([str(exe)], check=True, timeout=10)
print('actual live driver with deterministic ABI stubs; not SDK or COM execution')
