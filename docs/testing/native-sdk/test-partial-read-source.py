#!/usr/bin/env python3
"""Compile the actual test-only group task against a deterministic SDK stub."""
from pathlib import Path
import subprocess
import tempfile

root = Path(__file__).resolve().parents[3]
src = (root / 'native/softing-da/runtime.cpp').read_text()
start = src.index('// A test-only real SDK group call')
end = src.index('\n#endif', start)
fixture = (root / 'docs/testing/native-sdk/server-fixture/server.cpp').read_text()
fstart = fixture.index('        if (request->getOperation() == SODaSRequest::read && FAILED(read_result_))')
fend = fixture.index('        if (count_reads_', fstart)
prelude = r'''
#include <cassert>
#include <cstdint>
#include <new>
using HRESULT=int32_t; using WORD=uint16_t;
constexpr HRESULT S_OK=0, S_FALSE=1, E_FAIL=int32_t(0x80004005u), E_INVALIDARG=int32_t(0x80070057u), E_UNEXPECTED=int32_t(0x8000ffffu), E_OUTOFMEMORY=int32_t(0x8007000eu), E_ACCESSDENIED=int32_t(0x80070005u);
#define FAILED(x) ((x)<0)
#define SUCCEEDED(x) ((x)>=0)
constexpr int FALSE=0, OPC_QUALITY_UNCERTAIN=64;
struct FILETIME { uint32_t dwLowDateTime=0, dwHighDateTime=0; };
struct SOCmnVariant { uint16_t vt=3; int v=42; };
struct SOCmnDateTime { void get(FILETIME* f) { f->dwLowDateTime=17; } };
struct OpclDaScalar { int32_t service_result=0,item_result=0; uint32_t filetime_low=0,filetime_high=0; uint16_t quality=0,vartype=0; int bits=0; };
struct OpclDaReadOutcome { OpclDaScalar value; uint32_t available=0; };
constexpr unsigned OPCL_DA_READ_SERVICE=1, OPCL_DA_READ_ITEM=2, OPCL_DA_READ_SAMPLE=4;
struct SODaCItem {};
struct SODaCGroup;
struct SOCltTask { HRESULT result=E_FAIL; SOCltTask(SODaCGroup*) {} virtual ~SOCltTask()=default; virtual void execute()=0; void setResult(HRESULT h){result=h;} void release(){delete this;} };
struct SODaCGroup { int calls=0; bool reject=false; HRESULT syncRead(int n,SODaCItem**,int,SOCmnVariant*,WORD* q,SOCmnDateTime*,HRESULT* e) { ++calls; assert(n==2); q[0]=192; e[0]=S_OK; e[1]=E_ACCESSDENIED; return S_FALSE; } HRESULT executeTask(SOCltTask* t) {if(reject) return S_FALSE; t->execute(); return t->result;} };
struct OpclDaSession { SODaCGroup* group; };
struct OpclDaItem { SODaCItem* sdk; OpclDaSession* owner; };
bool owns(OpclDaSession* s,OpclDaItem* i){return s && i && i->owner==s;}
HRESULT scalar_copy(const SOCmnVariant& v,OpclDaScalar* out){out->bits=v.v;return S_OK;}
'''
postlude = r'''
struct SODaSRequest { enum Op {read,write}; Op op; int count=0; HRESULT result=0; Op getOperation(){return op;} void complete(HRESULT h,int,int,int,int){count++;result=h;} };
void fixture_branch(SODaSRequest* request, HRESULT read_result_) {
'''+fixture[fstart:fend]+r'''
}
int main(){
 SODaCGroup group; OpclDaSession session{&group}, other{&group}; SODaCItem a,b; OpclDaItem x{&a,&session},y{&b,&session},foreign{&b,&other}; OpclDaReadOutcome out[2];
 assert(opcl_da_test_mixed_read(&session,&x,&y,out)==S_OK); assert(group.calls==1);
 assert(out[0].available==7 && out[0].value.service_result==S_FALSE && out[0].value.item_result==S_OK && out[0].value.quality==192 && out[0].value.bits==42 && out[0].value.filetime_low==17);
 assert(out[1].available==3 && out[1].value.service_result==S_FALSE && out[1].value.item_result==E_ACCESSDENIED);
 assert(opcl_da_test_mixed_read(&session,&x,&foreign,out)==E_INVALIDARG && group.calls==1);
 group.reject=true; assert(opcl_da_test_mixed_read(&session,&x,&y,out)==E_UNEXPECTED && group.calls==1);
 SODaSRequest denied{SODaSRequest::read}, good{SODaSRequest::read}, write{SODaSRequest::write}; fixture_branch(&denied,E_ACCESSDENIED); fixture_branch(&good,S_OK); fixture_branch(&write,E_ACCESSDENIED); assert(denied.count==1 && denied.result==E_ACCESSDENIED && good.count==0 && write.count==0);
}
'''
with tempfile.TemporaryDirectory() as td:
    source = Path(td)/'stand.cpp'; exe = Path(td)/'stand'
    source.write_text(prelude + src[start:end] + postlude)
    subprocess.run(['c++','-std=c++17','-Wall','-Wextra','-Werror',str(source),'-o',str(exe)],check=True)
    subprocess.run([str(exe)],check=True)
print('actual source group task + fixture branch: SDK stub passed (not a Win32 SDK run)')
