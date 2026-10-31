#!/usr/bin/env python3
"""Compile actual test-only callback and retirement branches against SDK stubs."""
from pathlib import Path
import subprocess
import tempfile

source = (Path(__file__).resolve().parents[3] / 'native/softing-da/runtime.cpp').read_text()
start = source.index('    if (mode == 0) {', source.index('void RuntimeGroup::onReadAdvise('))
end = source.index('#else', start)
callback = source[start:end]
start = source.index('static bool test_async_terminal(')
end = source.index('extern "C" int32_t opcl_da_test_async_is_pending', start)
retirement = source[start:end]
stub = r'''
#include <cstdint>
#include <mutex>
#include <condition_variable>
#include <memory>
#include <cassert>
using HRESULT=int32_t; using DWORD=uint32_t; using WORD=uint16_t; using BYTE=uint8_t; using OPCHANDLE=uint32_t;
constexpr HRESULT S_OK=0,S_FALSE=1,E_FAIL=-1,E_INVALIDARG=-2,E_PENDING=-3,E_UNEXPECTED=-4,E_ACCESSDENIED=-5,DISP_E_TYPEMISMATCH=-6;
#define FAILED(x) ((x)<0)
#define SUCCEEDED(x) ((x)>=0)
constexpr uint16_t VT_I4=3;
struct FILETIME {uint32_t dwLowDateTime=0,dwHighDateTime=0;};
struct SODaCItem { OPCHANDLE h; OPCHANDLE getClientHandle() {return h;} };
struct SOCmnVariant {uint16_t vt=VT_I4; int32_t lVal=0;};
struct SOCmnDateTime {void get(FILETIME*) {}};
struct OpclDaReadI4 { HRESULT service_result=0,item_result=0; int32_t value=0; uint32_t filetime_low=0,filetime_high=0; uint16_t quality=0,vartype=0; };
struct PushState { std::mutex mutex; std::condition_variable async_ready; uint32_t async_pending=0,async_handle=0; bool async_done=false,async_timeout=false,async_task_done=false,async_submit_done=false; HRESULT async_result=E_FAIL,async_submit_result=E_FAIL,async_submit_item=E_FAIL; OpclDaReadI4 async_value{}; };
struct OpclDaSession {std::shared_ptr<PushState> push;};
void callback(std::shared_ptr<PushState> state, BYTE mode, DWORD transaction, HRESULT master_error, DWORD n, SODaCItem** items, SOCmnVariant* values, WORD* qualities, SOCmnDateTime* times, HRESULT* errors) {
'''+callback+r'''
}
'''+retirement+r'''
int main() {
 auto state=std::make_shared<PushState>(); state->async_pending=7; state->async_handle=13;
 state->async_submit_done=state->async_task_done=true; state->async_submit_result=state->async_submit_item=S_OK;
 SODaCItem other{14}; SODaCItem* items[]={&other}; SOCmnVariant value; WORD q=0xc0; SOCmnDateTime time; HRESULT error=S_OK;
 callback(state,0,7,E_ACCESSDENIED,1,items,&value,&q,&time,&error); assert(!state->async_done);
 callback(state,0,6,E_ACCESSDENIED,0,nullptr,nullptr,nullptr,nullptr,nullptr); assert(!state->async_done);
 callback(state,0,7,S_OK,0,nullptr,nullptr,nullptr,nullptr,nullptr); assert(!state->async_done);
 callback(state,0,7,E_ACCESSDENIED,0,nullptr,nullptr,nullptr,nullptr,nullptr);
 assert(state->async_done && test_async_terminal(*state)); OpclDaSession session{state}; OpclDaReadI4 out{};
 assert(opcl_da_test_async_retire(&session,&out)==E_ACCESSDENIED);
 assert(out.service_result==E_ACCESSDENIED && out.item_result==E_FAIL && out.vartype==0 && out.quality==0 && out.filetime_low==0 && out.filetime_high==0);
 assert(opcl_da_test_async_retire(&session,&out)==S_FALSE);
}
'''
with tempfile.TemporaryDirectory() as d:
    cpp = Path(d)/'test.cpp'; exe = Path(d)/'test'; cpp.write_text(stub)
    subprocess.run(['c++', '-std=c++17', '-Wall', '-Wextra', '-Werror', '-pthread', str(cpp), '-o', str(exe)], check=True)
    subprocess.run([str(exe)], check=True, timeout=10)
print('actual callback/retire branches: empty failed master terminal; wrong handle, stale token and empty success ignored')
print('SDK types are stubs; Windows SDK callback remains a separate CI gate')
