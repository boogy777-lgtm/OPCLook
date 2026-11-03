#!/usr/bin/env python3
"""Compile the actual read-only slot snapshot against deterministic SDK stubs."""
from pathlib import Path
import subprocess
import tempfile

root = Path(__file__).resolve().parents[3]
source = (root / "native/softing-da/runtime.cpp").read_text()
start = source.index('extern "C" int32_t opcl_da_test_async_facts(')
end = source.index('// Nonblocking poll:', start)
function = source[start:end]
code = r'''
#include <cassert>
#include <cstdint>
#include <memory>
#include <mutex>
using HRESULT = int32_t;
constexpr HRESULT E_INVALIDARG = -1, E_UNEXPECTED = -2, S_OK = 0;
struct PushState {
 std::mutex mutex; bool async_call_attempted = false, async_sdk_returned = false;
 bool async_sdk_item_known = false, async_done = false;
 HRESULT async_submit_result = -9, async_submit_item = -8;
};
struct OpclDaSession { std::shared_ptr<PushState> push; };
''' + function + r'''
int main() {
 OpclDaSession s{std::make_shared<PushState>()}; uint32_t flags=99; int32_t submit=0, item=0;
 assert(opcl_da_test_async_facts(&s,&flags,&submit,&item)==0 && flags==0);
 s.push->async_call_attempted=true; s.push->async_sdk_returned=true;
 s.push->async_submit_result=1; s.push->async_submit_item=-5;
 assert(opcl_da_test_async_facts(&s,&flags,&submit,&item)==0 && flags==3 && submit==1 && item==-5);
 s.push->async_sdk_item_known=true; s.push->async_done=true;
 assert(opcl_da_test_async_facts(&s,&flags,&submit,&item)==0 && flags==15);
 assert(opcl_da_test_async_facts(&s,nullptr,&submit,&item)==E_INVALIDARG);
}
'''
with tempfile.TemporaryDirectory() as tmp:
    src = Path(tmp) / 'facts.cpp'
    exe = Path(tmp) / 'facts'
    src.write_text(code)
    subprocess.run(['c++', '-std=c++17', '-pthread', str(src), '-o', str(exe)], check=True)
    subprocess.run([str(exe)], check=True)
print('actual slot snapshot: unknown, attempted/returned, known item and callback flags; SDK stub')
