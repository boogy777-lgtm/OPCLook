#!/usr/bin/env python3
"""Check actual adapter task and pinned SDK base destructor against SDK stubs."""
import argparse
import hashlib
from pathlib import Path
import subprocess
import tempfile

parser = argparse.ArgumentParser()
parser.add_argument('--task-source', type=Path, required=True)
args = parser.parse_args()
raw = args.task_source.read_bytes()
blob = hashlib.sha1(b'blob ' + str(len(raw)).encode() + b'\0' + raw).hexdigest()
if blob != 'c158c67ff4cd50f59258ec1f46525b883a103580':
    raise SystemExit(f'SDK task source blob mismatch: {blob}')
sdk = raw.decode('utf-8')
start = sdk.index('SOCltTask::~SOCltTask()')
end = sdk.index('\nvoid SOCltTask::reportCompletion()', start)
base_destructor = sdk[start:end]
source = (Path(__file__).resolve().parents[3] / 'native/softing-da/runtime.cpp').read_text()
start = source.index('// The SDK submits AsyncIO on its own worker;')
end = source.index('static bool test_async_terminal(', start)
task = source[start:end]
stub = r'''
#include <cassert>
#include <cstdint>
#include <condition_variable>
#include <memory>
#include <mutex>
#include <thread>
#include <utility>
using HRESULT = int32_t; using DWORD = uint32_t; using HANDLE = void*;
constexpr HRESULT E_FAIL=-1,E_ABORT=-2,E_UNEXPECTED=-3,S_OK=0;
constexpr DWORD WAIT_OBJECT_0=0;
struct PushState {std::mutex mutex; std::condition_variable async_ready; uint32_t async_pending=7; bool async_submit_done=false,async_task_done=false; HRESULT async_submit_result=E_FAIL,async_submit_item=E_FAIL;};
std::shared_ptr<PushState> observed;
struct Gate { std::mutex mutex; std::condition_variable cv; bool entered=false, release=false; };
Gate* gate=nullptr; int* item_refs=nullptr;
struct GroupRef { void release() {
 assert(observed && !observed->async_task_done && item_refs && *item_refs==1);
 std::unique_lock<std::mutex> lock(gate->mutex); gate->entered=true; gate->cv.notify_all();
 gate->cv.wait(lock, []{return gate->release;}); group_released=true;
} static bool group_released; };
bool GroupRef::group_released=false;
struct SODaCItem {int refs=1; void addRef(){++refs;} void release(){--refs;}};
struct SODaCGroup {HRESULT asyncRead(int,SODaCItem**,DWORD,DWORD,DWORD*,HRESULT* result){*result=S_OK; return S_OK;}};
struct SOCltTask {GroupRef m_element; explicit SOCltTask(SODaCGroup*){} virtual ~SOCltTask(); virtual void execute()=0; void release(){delete this;} void setResult(HRESULT){}};
'''+base_destructor+r'''
HANDLE test_async_entered=nullptr, test_async_release=nullptr;
int SetEvent(HANDLE){return 1;} DWORD WaitForSingleObject(HANDLE,DWORD){return WAIT_OBJECT_0;}
'''+task+r'''
int main() {
 for (int executed=0;executed!=2;executed++) {
  observed=std::make_shared<PushState>(); GroupRef::group_released=false; SODaCGroup group; SODaCItem item;
  Gate current; gate=&current; item_refs=&item.refs;
  SOCltTask* task=new TestAsyncReadTask(&group,&item,0,7,observed); assert(item.refs==2);
  if (executed) task->execute();
  std::thread worker([&]{task->release();});
  { std::unique_lock<std::mutex> lock(current.mutex); current.cv.wait(lock,[&]{return current.entered;}); }
  { std::lock_guard<std::mutex> lock(observed->mutex); assert(!observed->async_task_done); }
  { std::lock_guard<std::mutex> lock(current.mutex); current.release=true; current.cv.notify_all(); }
  worker.join();
  assert(item.refs==1 && GroupRef::group_released && observed->async_task_done);
  assert(observed->async_submit_done);
  assert(observed->async_submit_result==(executed ? S_OK : E_ABORT));
 }
}
'''
with tempfile.TemporaryDirectory() as d:
    cpp=Path(d)/'test.cpp'; exe=Path(d)/'test'; cpp.write_text(stub)
    subprocess.run(['c++','-std=c++17','-Wall','-Wextra','-Werror','-pthread',str(cpp),'-o',str(exe)],check=True)
    subprocess.run([str(exe)],check=True,timeout=10)
print('actual adapter task + pinned SDK destructor: item/group released before completion, executed and queued-removal branches')
print('SDK types are stubs; Windows SDK execution remains a separate gate')
