#!/usr/bin/env python3
"""Compile actual test-only callback and retirement branches against SDK stubs."""
from pathlib import Path
import subprocess
import tempfile

source = (Path(__file__).resolve().parents[3] / 'native/softing-da/runtime.cpp').read_text()
start = source.index('    if (mode == 0) {', source.index('void RuntimeGroup::onReadAdvise('))
end = source.index('#else', start)
callback = source[start:end]
start = source.index('static bool test_async_admission_unknown(')
end = source.index('extern "C" int32_t opcl_da_test_async_is_pending', start)
retirement = source[start:end]
stub = r'''
#include <cstdint>
#include <mutex>
#include <condition_variable>
#include <memory>
#include <cassert>
#include <vector>
#include <cstring>
#include <string>
using HRESULT=int32_t; using DWORD=uint32_t; using WORD=uint16_t; using BYTE=uint8_t; using OPCHANDLE=uint32_t;
constexpr HRESULT S_OK=0,S_FALSE=1,E_FAIL=-1,E_INVALIDARG=-2,E_PENDING=-3,E_UNEXPECTED=-4,E_ACCESSDENIED=-5,DISP_E_TYPEMISMATCH=-6,E_OUTOFMEMORY=-7;
constexpr uint32_t OPCL_DA_READ_SERVICE=1,OPCL_DA_READ_ITEM=2,OPCL_DA_READ_SAMPLE=4,OPCL_DA_VALUE_BYTES=65536;
#define FAILED(x) ((x)<0)
#define SUCCEEDED(x) ((x)>=0)
constexpr uint16_t VT_I4=3;
struct FILETIME {uint32_t dwLowDateTime=0,dwHighDateTime=0;};
struct SODaCItem { OPCHANDLE h; OPCHANDLE getClientHandle() {return h;} };
struct SOCmnVariant {uint16_t vt=VT_I4; int32_t lVal=0;};
struct SOCmnDateTime {void get(FILETIME*) {}};
struct OpclDaReadI4 { HRESULT service_result=0,item_result=0; int32_t value=0; uint32_t filetime_low=0,filetime_high=0; uint16_t quality=0,vartype=0; };
struct OpclDaScalar {HRESULT service_result=0,item_result=0;uint32_t filetime_low=0,filetime_high=0,text_len=0;uint16_t quality=0,vartype=0;};
struct OpclDaReadOutcome {OpclDaScalar value;uint32_t available=0;};
HRESULT copy_sdk_value(SOCmnVariant& v,OpclDaScalar* out,char* data,uint32_t capacity) {assert(data && capacity==65536);out->vartype=v.vt; if (v.vt==8) {std::memcpy(data,"hello",5);out->text_len=5;} if(v.vt==0x2003)return E_OUTOFMEMORY; if(v.vt==0x4003)return DISP_E_TYPEMISMATCH; return S_OK;}
struct PushState { std::mutex mutex; std::condition_variable async_ready; uint32_t async_pending=0,async_handle=0; bool async_done=false,async_timeout=false,async_task_done=false,async_submit_done=false,async_call_attempted=false,async_sdk_returned=false,async_sdk_item_known=true; HRESULT async_result=E_FAIL,async_submit_result=E_FAIL,async_submit_item=E_FAIL; OpclDaReadI4 async_value{}; OpclDaReadOutcome async_outcome{}; std::vector<char> async_payload; HRESULT async_copy_result=S_OK; };
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
 assert(state->async_done && test_async_terminal(*state)); assert(state->async_outcome.available==OPCL_DA_READ_SERVICE && state->async_payload.empty()); OpclDaSession session{state}; OpclDaReadI4 out{};
 assert(opcl_da_test_async_retire(&session,&out)==E_ACCESSDENIED);
 assert(out.service_result==E_ACCESSDENIED && out.item_result==E_FAIL && out.vartype==0 && out.quality==0 && out.filetime_low==0 && out.filetime_high==0);
 assert(opcl_da_test_async_retire(&session,&out)==S_FALSE);
 // SDK accepted the call but left pErrors untouched: no invented item failure
 // and no early retirement before a callback.
 state->async_pending=8; state->async_done=false; state->async_submit_done=true; state->async_task_done=true;
 state->async_submit_result=S_OK; state->async_submit_item=E_FAIL; state->async_sdk_item_known=false;
 assert(!test_async_terminal(*state));
 state->async_sdk_item_known=true; state->async_submit_item=S_OK;
 // A matching successful callback copies payload before SDK-owned arrays disappear.
 SODaCItem good{13}; items[0]=&good; state->async_pending=8; state->async_done=false; value.vt=8;
 state->async_submit_done=false; state->async_task_done=false;
 callback(state,0,8,S_OK,1,items,&value,&q,&time,&error);
 assert(!test_async_terminal(*state)); // Callback precedes SDK return and task retirement.
 state->async_submit_done=true; assert(!test_async_terminal(*state));
 state->async_task_done=true; assert(test_async_terminal(*state));
 assert(state->async_done && state->async_outcome.available==7 && state->async_outcome.value.vartype==8);
 assert(std::string(state->async_payload.begin(),state->async_payload.end())=="hello");
 state->async_pending=9; state->async_done=false; value.vt=0x2003;
 callback(state,0,9,S_OK,1,items,&value,&q,&time,&error);
 assert(state->async_done && state->async_copy_result==E_OUTOFMEMORY && state->async_outcome.available==7 && state->async_payload.empty());
 state->async_pending=11; state->async_done=false; value.vt=0x4003;
 callback(state,0,11,S_OK,1,items,&value,&q,&time,&error);
 assert(state->async_done && state->async_copy_result==DISP_E_TYPEMISMATCH && state->async_outcome.available==7 && state->async_outcome.value.vartype==0x4003 && state->async_payload.empty());
 state->async_pending=12; state->async_done=false; error=S_OK;
 callback(state,0,12,E_ACCESSDENIED,1,items,&value,&q,&time,&error);
 assert(state->async_done && state->async_outcome.available==1 && state->async_payload.empty());
 state->async_pending=10; state->async_done=false; error=E_ACCESSDENIED;
 callback(state,0,10,S_OK,1,items,&value,&q,&time,&error);
 assert(state->async_done && state->async_outcome.available==3 && state->async_outcome.value.item_result==E_ACCESSDENIED && state->async_payload.empty());
}
'''
with tempfile.TemporaryDirectory() as d:
    cpp = Path(d)/'test.cpp'; exe = Path(d)/'test'; cpp.write_text(stub)
    subprocess.run(['c++', '-std=c++17', '-Wall', '-Wextra', '-Werror', '-pthread', str(cpp), '-o', str(exe)], check=True)
    subprocess.run([str(exe)], check=True, timeout=10)
print('actual callback/retire: empty master, wrong handle, stale token, BSTR copy, oversized/BYREF and failed item')
print('SDK types are stubs; Windows SDK callback remains a separate CI gate')
