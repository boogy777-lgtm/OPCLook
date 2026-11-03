#!/usr/bin/env python3
"""Compile pinned SDK version interface filter and sync dispatch with stubs."""
import os
from pathlib import Path
import subprocess
import tempfile
sdk = Path(os.environ['SOFTING_SDK_ROOT'])
subprocess.run(['python3', str(Path(__file__).with_name('verify-softing-sdk.py')), '--core-clean'], check=True)
s = (sdk / 'development/core/src/SODaC/SODaCComGroup.cpp').read_text()
a = s.index('switch (server->getForcedOPCVersion())')
b = s.index('\n\t\t\t}\n\t\t}', a)
version_switch = s[a:b]
a = s.index('for (i = 0; i < numItems; i++)', s.index('HRESULT SODaCComGroupImpl::syncReadImpl'))
b = s.index('\n\t\tpDaCGroup->afterInterfaceCall();', a)
max_age = s[a:b].replace('pDaCGroup->beforeInterfaceCall();', '')
a = s.index('res = syncIO->Read((maxAge == SODAC_MAXAGE_CACHE)')
b = s.index(';', a) + 1
fallback = s[a:b]
pre = r'''
#include <cassert>
#include <cstdint>
#include <cstddef>
using DWORD=uint32_t;
constexpr int SODAC_VERSION_OPC_DAV10A=0x101,SODAC_VERSION_OPC_DAV20=0x200,SODAC_VERSION_OPC_DAV30=0x300,E_NOINTERFACE=-1,OPC_DS_CACHE=1,OPC_DS_DEVICE=2;
constexpr DWORD SODAC_MAXAGE_CACHE=UINT32_MAX;
struct Interface {void Release(){}}; struct QI {Interface* pItf;int hr;};
struct Server {int version; int getForcedOPCVersion(){return version;}};
void filter(Server* server,QI* mqi){
'''
mid = r'''
}
struct Sync {int source=0; int Read(int src,DWORD,void*,void**,void**){source=src;return 0;}};
struct Sync2 {DWORD age=0; int ReadMaxAge(DWORD,void*,DWORD* ages,void**,void**,void**,void**){age=ages[0];return 0;}};
int fallback_call(DWORD maxAge){ Sync obj; Sync* syncIO=&obj; DWORD numItems=1; void *itemHandles=nullptr,*iItemState=nullptr,*errors=nullptr; int res;
'''
maxpre = r'''
return res==0 ? obj.source : -1; }
DWORD max_call(DWORD maxAge){ Sync2 obj; Sync2* syncIO2=&obj; DWORD numItems=1,i=0,ageStorage[1]={}; DWORD* iMaxAge=ageStorage; void *itemHandles=nullptr,*iValues=nullptr,*iQualities=nullptr,*iTimeStamps=nullptr,*errors=nullptr; int res;
'''
post = r'''
return res==0 ? obj.age : 0; }
int main(){ Interface obj; QI q[11]; Server server{}; for(auto& x:q)x={&obj,0}; server.version=SODAC_VERSION_OPC_DAV20; filter(&server,q); assert(q[1].pItf && !q[6].pItf && !q[7].pItf); for(auto& x:q)x={&obj,0}; server.version=SODAC_VERSION_OPC_DAV30; filter(&server,q); assert(q[1].pItf && q[6].pItf); assert(fallback_call(UINT32_MAX)==OPC_DS_CACHE && fallback_call(0)==OPC_DS_DEVICE && fallback_call(1234)==OPC_DS_DEVICE); assert(max_call(UINT32_MAX)==UINT32_MAX && max_call(0)==0 && max_call(1234)==1234); }
'''
with tempfile.TemporaryDirectory() as d:
    src=Path(d)/'test.cpp'; exe=Path(d)/'test'; src.write_text(pre+version_switch+mid+fallback+maxpre+max_age+post)
    subprocess.run(['c++','-std=c++17','-Wall','-Wextra','-Werror',str(src),'-o',str(exe)],check=True)
    subprocess.run([str(exe)],check=True,timeout=10)
print('pinned SDK source filter and read dispatch executed with COM stubs; no actual COM')
