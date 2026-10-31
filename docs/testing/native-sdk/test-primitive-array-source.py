#!/usr/bin/env python3
"""Run the actual C ABI pre-dispatch array validation against SDK stubs."""
from pathlib import Path
import subprocess
import tempfile
source = (Path(__file__).resolve().parents[3] / 'native/softing-da/runtime.cpp').read_text()
start = source.index('static uint32_t primitive_array_width(')
end = source.index('static HRESULT copy_sdk_value(', start)
width = source[start:end]
start = source.index('extern "C" int32_t opcl_da_write_array(')
end = source.index('\nclass WriteTextTask', start)
write = source[start:end]
# Include additional local POD validators introduced alongside width if present.
stub = r'''
#include <cstdint>
#include <cstring>
#include <new>
#include <cassert>
using HRESULT=int32_t; using BOOL=int;
constexpr int S_OK=0,E_FAIL=-1,E_INVALIDARG=-2,E_OUTOFMEMORY=-3,E_UNEXPECTED=-4,DISP_E_TYPEMISMATCH=-5;
constexpr uint16_t VT_ARRAY=0x2000,VT_VARIANT=12,VT_BSTR=8,VT_I1=16,VT_UI1=17,VT_I2=2,VT_UI2=18,VT_BOOL=11,VT_I4=3,VT_UI4=19,VT_INT=22,VT_UINT=23,VT_R4=4,VT_ERROR=10,VT_I8=20,VT_UI8=21,VT_R8=5,VT_CY=6,VT_DATE=7,VT_FILETIME=64;
struct Task { void release() {delete this;} virtual ~Task()=default; };
struct Group { int calls=0; HRESULT executeTask(Task*) {++calls; return S_OK;} };
struct OpclDaItem {void* sdk=nullptr;}; struct OpclDaSession {Group* group;};
bool owns(OpclDaSession* s, OpclDaItem* i){return s&&i;}
struct WriteVariantArrayTask:Task {WriteVariantArrayTask(Group*,void*,const uint8_t*,uint32_t,int32_t*){}};
struct WriteStringArrayTask:Task {WriteStringArrayTask(Group*,void*,const uint8_t*,uint32_t,uint32_t,int32_t*){}};
struct WriteArrayTask:Task {WriteArrayTask(Group*,void*,uint16_t,const uint8_t*,uint32_t,int32_t*){}};
'''+width+write+r'''
int main(){ Group group; OpclDaSession session{&group}; OpclDaItem item; int32_t result=123;
 uint8_t bad[]={1,0}; assert(opcl_da_write_array(&session,&item,VT_ARRAY|VT_BOOL,bad,2,&result)==E_INVALIDARG); assert(group.calls==0);
 uint8_t valid[]={0xff,0xff,0,0}; assert(opcl_da_write_array(&session,&item,VT_ARRAY|VT_BOOL,valid,4,&result)==S_OK); assert(group.calls==1);
 assert(opcl_da_write_array(&session,&item,VT_ARRAY|VT_I8,valid,4,&result)==E_INVALIDARG); assert(group.calls==1);
 uint8_t large[514]{}; assert(opcl_da_write_array(&session,&item,VT_ARRAY|VT_BOOL,large,sizeof(large),&result)==E_INVALIDARG); assert(group.calls==1);
}
'''
with tempfile.TemporaryDirectory() as d:
    cpp=Path(d)/'test.cpp'; exe=Path(d)/'test'; cpp.write_text(stub)
    subprocess.run(['c++','-std=c++17','-Wall','-Wextra','-Werror',str(cpp),'-o',str(exe)],check=True)
    subprocess.run([str(exe)],check=True,timeout=10)
print('actual native ABI pre-dispatch checks passed; SDK task/COM are stubs')
