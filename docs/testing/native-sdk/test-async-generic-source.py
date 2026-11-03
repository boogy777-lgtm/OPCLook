#!/usr/bin/env python3
"""Compile the actual generic result handoff using SDK/queue stubs, not COM."""
from pathlib import Path
import subprocess
import tempfile
s=(Path(__file__).resolve().parents[3]/'native/softing-da/runtime.cpp').read_text()
a=s.index('extern "C" int32_t opcl_da_test_async_read_value('); b=s.index('\nextern "C" int32_t opcl_da_test_async_callback_state',a);func=s[a:b]
pre=r'''
#include <cstdint>
#include <mutex>
#include <memory>
#include <vector>
#include <cstring>
#include <cassert>
using HRESULT=int32_t;
constexpr HRESULT S_OK=0,E_FAIL=-1,E_PENDING=-2,E_INVALIDARG=-3,E_UNEXPECTED=-4,E_OUTOFMEMORY=-5,E_ABORT=-6;
constexpr uint32_t OPCL_DA_VALUE_BYTES=65536, OPCL_DA_READ_SERVICE=1, OPCL_DA_READ_ITEM=2;
#define FAILED(x) ((x)<0)
#define SUCCEEDED(x) ((x)>=0)
struct OpclDaScalar{HRESULT service_result=0,item_result=0;uint32_t text_len=0;uint16_t quality=0,vartype=0;};
struct OpclDaReadOutcome {OpclDaScalar value;uint32_t available=0;};
struct PushState {std::mutex mutex;bool async_timeout=false,async_sdk_returned=true,async_sdk_item_known=true;uint32_t async_pending=1; HRESULT async_submit_result=S_OK,async_submit_item=S_OK,async_copy_result=S_OK;OpclDaReadOutcome async_outcome{};std::vector<char> async_payload;};
struct OpclDaSession {std::shared_ptr<PushState> push;}; struct OpclDaItem{};
HRESULT test_async_submit_wait(OpclDaSession*,OpclDaItem*,uint32_t,uint32_t){return S_OK;}
bool test_async_terminal(const PushState&){return true;}
'''
post=r'''
int main(){auto p=std::make_shared<PushState>();OpclDaSession s{p};OpclDaItem item;OpclDaReadOutcome out{};std::vector<char> buf(OPCL_DA_VALUE_BYTES);
 p->async_outcome.available=7;p->async_outcome.value.vartype=8;p->async_outcome.value.text_len=5;p->async_outcome.value.quality=192;p->async_payload={'h','e','l','l','o'};
 assert(opcl_da_test_async_read_value(&s,&item,0,100,&out,buf.data(),buf.size())==S_OK);assert(out.available==7&&out.value.vartype==8&&std::memcmp(buf.data(),"hello",5)==0&&p->async_pending==0&&p->async_payload.empty());
 p->async_pending=1;p->async_copy_result=E_OUTOFMEMORY;p->async_outcome.available=7;p->async_outcome.value.vartype=0x2003;p->async_payload.clear();
 assert(opcl_da_test_async_read_value(&s,&item,0,100,&out,buf.data(),buf.size())==E_OUTOFMEMORY);assert(out.available==7&&out.value.vartype==0x2003&&p->async_pending==0);
 p->async_pending=1;p->async_sdk_returned=false;p->async_submit_result=E_ABORT;
 assert(opcl_da_test_async_read_value(&s,&item,0,100,&out,buf.data(),buf.size())==E_ABORT && out.available==0);
 p->async_pending=1;p->async_submit_result=E_UNEXPECTED;
 assert(opcl_da_test_async_read_value(&s,&item,0,100,&out,buf.data(),buf.size())==E_UNEXPECTED && out.available==0);
 p->async_pending=1;p->async_sdk_returned=true;p->async_submit_result=E_FAIL;
 assert(opcl_da_test_async_read_value(&s,&item,0,100,&out,buf.data(),buf.size())==S_OK && out.available==1 && out.value.service_result==E_FAIL);
 p->async_pending=1;p->async_submit_result=S_OK;p->async_submit_item=E_FAIL;
 assert(opcl_da_test_async_read_value(&s,&item,0,100,&out,buf.data(),buf.size())==S_OK && out.available==3 && out.value.item_result==E_FAIL);
 assert(opcl_da_test_async_read_value(&s,&item,0,100,&out,buf.data(),42)==E_INVALIDARG);
}
'''
with tempfile.TemporaryDirectory() as td:
 cpp=Path(td)/'test.cpp';exe=Path(td)/'test';cpp.write_text(pre+func+post)
 subprocess.run(['c++','-std=c++17','-Wall','-Wextra','-Werror','-pthread',str(cpp),'-o',str(exe)],check=True)
 subprocess.run([str(exe)],check=True,timeout=10)
print('actual generic async result handoff: success, copied payload, bounded failure, capacity guard; SDK/queue stub')
