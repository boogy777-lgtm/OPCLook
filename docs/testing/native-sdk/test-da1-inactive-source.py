#!/usr/bin/env python3
"""Compile actual DA1 control helper and live driver with deterministic SDK stubs."""
from pathlib import Path
import subprocess
import tempfile
root=Path(__file__).resolve().parents[3]
s=(root/'native/softing-da/runtime.cpp').read_text()
a=s.index('extern "C" int32_t opcl_da_test_da1_is_inactive(');b=s.index('\n#endif',a); helper=s[a:b]
s=(root/'docs/testing/native-sdk/core-probe/main.cpp').read_text()
a=s.index('static int live_da1_inactive_fixture()');b=s.index('\nstatic int live_async_fixture()',a);driver=s[a:b]
pre=r'''
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <vector>
#include <cassert>
using HRESULT=int32_t; constexpr HRESULT S_OK=0,E_FAIL=-1,E_INVALIDARG=-2,E_UNEXPECTED=-3; constexpr int FALSE=0, SODAC_VERSION_OPC_DAV10A=10, VT_I4=3, OPC_QUALITY_OUT_OF_SERVICE=28, OPC_QUALITY_GOOD=192;
#define SUCCEEDED(x) ((x)>=0)
#define FAILED(x) ((x)<0)
struct Server {int forced=0; int getForcedOPCVersion(){return forced;} bool forceOPCVersion(int v){forced=v;return true;}};
struct Group {bool connected=true,started=true; HRESULT disconnect(){connected=false;started=false;return S_OK;} HRESULT connect(int){connected=true;started=false;return S_OK;} bool isConnected(){return connected;} bool isStarted(){return started;}};
struct OpclDaHost{}; struct OpclDaItem {int kind;}; struct OpclDaSession {bool subscription=false; Server* server; Group* group; std::vector<int> items;};
struct OpclDaGuid {uint32_t a;uint16_t b,c;uint8_t d[8];};
struct OpclDaReadI4 {int32_t service_result=0,item_result=0,value=0;uint32_t filetime_low=1,filetime_high=0;uint16_t quality=192,vartype=3;};
static Server server; static Group group; static OpclDaSession session{false,&server,&group,{}}; static OpclDaHost host;
static OpclDaItem target{0}, cache{1}, device{2}; static int cache_count=0,device_count=0; static bool wrong_path=false;
HRESULT opcl_da_host_create(OpclDaHost** p){*p=&host;return S_OK;}
HRESULT opcl_da_session_open(OpclDaHost*,const char*,const char*,const OpclDaGuid*,int,int,int,uint32_t*,OpclDaSession** p){*p=&session;return S_OK;}
HRESULT opcl_da_item_add(OpclDaSession*,const char* id,uint32_t,uint32_t,OpclDaItem** out,uint16_t*,uint32_t*){*out=std::strstr(id,"CacheKind")?&cache:std::strstr(id,"DeviceKind")?&device:&target;return S_OK;}
HRESULT opcl_da_read_i4(OpclDaSession*,OpclDaItem* item,uint32_t age,OpclDaReadI4* out){if(item==&cache)out->value=cache_count;else if(item==&device)out->value=device_count;else {out->value=42;if(age==UINT32_MAX){if(wrong_path) ++device_count; else ++cache_count;out->quality=28;}else{++device_count;out->quality=192;}}return S_OK;}
HRESULT opcl_da_session_close(OpclDaSession*){return S_OK;} HRESULT opcl_da_host_destroy(OpclDaHost*){return S_OK;}
'''
post=r'''
int main(){ assert(live_da1_inactive_fixture()==0); assert(cache_count==1 && device_count==1); wrong_path=true; assert(live_da1_inactive_fixture()==62); session.subscription=true; assert(opcl_da_test_da1_inactive(&session)==E_INVALIDARG); session.subscription=false; session.items.push_back(1); assert(opcl_da_test_da1_inactive(&session)==E_INVALIDARG); session.items.clear(); group.started=true; assert(opcl_da_test_da1_is_inactive(&session)==E_FAIL); }
'''
with tempfile.TemporaryDirectory() as d:
 p=Path(d)/'test.cpp';exe=Path(d)/'test';p.write_text(pre+helper+driver+post)
 subprocess.run(['c++','-std=c++17','-Wall','-Wextra','-Werror',str(p),'-o',str(exe)],check=True)
 subprocess.run([str(exe)],check=True,timeout=10)
print('actual inactive helper + live driver: deterministic SDK stub (not Win32)')
