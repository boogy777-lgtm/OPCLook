#!/usr/bin/env python3
"""Compile the actual test-only SDK token observation against a stub slot."""
from pathlib import Path
import subprocess
import tempfile
s=(Path(__file__).resolve().parents[3]/'native/softing-da/runtime.cpp').read_text()
a=s.index('extern "C" int32_t opcl_da_test_async_last_token(')
b=s.index('extern "C" int32_t opcl_da_test_async_wait_retire(',a)
func=s[a:b]
pre=r'''
#include <cstdint>
#include <mutex>
#include <memory>
#include <cassert>
using HRESULT=int32_t;constexpr HRESULT S_OK=0,E_INVALIDARG=-1,E_UNEXPECTED=-2;
struct PushState{std::mutex mutex;uint32_t async_next=1;};
struct OpclDaSession{std::shared_ptr<PushState> push;};
'''
post=r'''
int main(){auto slot=std::make_shared<PushState>();OpclDaSession session{slot};uint32_t value=77;
 assert(opcl_da_test_async_last_token(&session,&value)==S_OK&&value==0);
 slot->async_next=4;assert(opcl_da_test_async_last_token(&session,&value)==S_OK&&value==3);
 assert(opcl_da_test_async_last_token(nullptr,&value)==E_INVALIDARG);
 assert(opcl_da_test_async_last_token(&session,nullptr)==E_INVALIDARG);
}
'''
with tempfile.TemporaryDirectory() as tmp:
 p=Path(tmp)/'test.cpp'; exe=Path(tmp)/'test';p.write_text(pre+func+post)
 subprocess.run(['c++','-std=c++17','-Wall','-Wextra','-Werror','-pthread',str(p),'-o',str(exe)],check=True)
 subprocess.run([str(exe)],check=True,timeout=10)
print('actual token-observation function: slot stub passed; not real SDK or COM')
