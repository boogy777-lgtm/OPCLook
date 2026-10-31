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
end = source.index('// If the SDK call threw after entry,', start)
task = source[start:end]
start = source.index('static bool test_async_admission_unknown(')
end = source.index('extern "C" int32_t opcl_da_test_async_is_pending', start)
terminal = source[start:end]
stub = r'''
#include <cassert>
#include <cstdint>
#include <condition_variable>
#include <memory>
#include <mutex>
#include <thread>
#include <utility>
using HRESULT = int32_t; using DWORD = uint32_t; using HANDLE = void*;
constexpr HRESULT E_FAIL=-1,E_ABORT=-2,E_UNEXPECTED=-3,S_OK=0,S_FALSE=1,E_PENDING=-4,E_INVALIDARG=-5;
constexpr DWORD WAIT_OBJECT_0=0;
#define SUCCEEDED(x) ((x)>=0)
#define FAILED(x) ((x)<0)
struct PushState {std::mutex mutex; std::condition_variable async_ready; uint32_t async_pending=7; bool async_submit_done=false,async_call_attempted=false,async_sdk_returned=false,async_sdk_item_known=false,async_task_done=false,async_done=false,async_timeout=false; HRESULT async_submit_result=E_FAIL,async_submit_item=E_FAIL,async_result=E_FAIL; struct Value {int32_t service_result=0,item_result=0;} async_value{};};
using OpclDaReadI4=PushState::Value; struct OpclDaSession {std::shared_ptr<PushState> push;};
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
struct SODaCGroup {bool throws=false, missing_item=false,accepted=false; HRESULT asyncRead(int,SODaCItem**,DWORD,DWORD,DWORD*,HRESULT* result){accepted=true;if(throws)throw 1;if(!missing_item)*result=S_OK; return S_OK;}};
struct SOCltTask {GroupRef m_element; explicit SOCltTask(SODaCGroup*){} virtual ~SOCltTask(); virtual void execute()=0; void release(){delete this;} void setResult(HRESULT){}};
'''+base_destructor+r'''
HANDLE test_async_entered=nullptr, test_async_release=nullptr;
int SetEvent(HANDLE){return 1;} DWORD WaitForSingleObject(HANDLE,DWORD){return WAIT_OBJECT_0;}
'''+task+terminal+r'''
int main() {
 for (int executed=0;executed!=4;executed++) {
  observed=std::make_shared<PushState>(); GroupRef::group_released=false; SODaCGroup group; SODaCItem item;
  group.throws=executed==2; group.missing_item=executed==3; Gate current; gate=&current; item_refs=&item.refs;
  SOCltTask* task=new TestAsyncReadTask(&group,&item,0,7,observed); assert(item.refs==2);
  if (executed) task->execute();
  std::thread worker([&]{task->release();});
  { std::unique_lock<std::mutex> lock(current.mutex); current.cv.wait(lock,[&]{return current.entered;}); }
  { std::lock_guard<std::mutex> lock(observed->mutex); assert(!observed->async_task_done); }
  { std::lock_guard<std::mutex> lock(current.mutex); current.release=true; current.cv.notify_all(); }
  worker.join();
  assert(item.refs==1 && GroupRef::group_released && observed->async_task_done);
  assert(observed->async_submit_done);
  assert(observed->async_submit_result==(executed==0 ? E_ABORT : executed==2 ? E_UNEXPECTED : S_OK));
  assert(observed->async_call_attempted==(executed!=0));
  assert(observed->async_sdk_returned==(executed==1 || executed==3));
  assert(observed->async_sdk_item_known==(executed==1));
  if (executed==0) assert(test_async_terminal(*observed));
  if (executed==2) {
   assert(group.accepted && test_async_admission_unknown(*observed));
   assert(!test_async_terminal(*observed));
   OpclDaSession session{observed}; OpclDaReadI4 out{};
   assert(opcl_da_test_async_retire(&session,&out)==E_PENDING && observed->async_pending==7);
   observed->async_timeout=true; observed->async_done=true;
   assert(opcl_da_test_async_retire(&session,&out)==S_OK && observed->async_pending==0);
   assert(out.service_result==0 && out.item_result==0);
  }
 }
}
'''
with tempfile.TemporaryDirectory() as d:
    cpp=Path(d)/'test.cpp'; exe=Path(d)/'test'
    # Counterfactual restores only the previous terminal predicate.
    new_predicate = "(test_async_admission_unknown(state) ? state.async_done :\n         (FAILED(state.async_submit_result) || (state.async_sdk_item_known && FAILED(state.async_submit_item)) || state.async_done))"
    old_predicate = "(FAILED(state.async_submit_result) || (state.async_sdk_item_known && FAILED(state.async_submit_item)) || state.async_done)"
    assert new_predicate in stub
    cpp.write_text(stub.replace(new_predicate, old_predicate, 1))
    subprocess.run(['c++','-std=c++17','-Wall','-Wextra','-Werror','-pthread',str(cpp),'-o',str(exe)],check=True)
    import resource
    baseline = subprocess.run([str(exe)],capture_output=True,timeout=10,preexec_fn=lambda: resource.setrlimit(resource.RLIMIT_CORE,(0,0)))
    assert baseline.returncode != 0 and b'!test_async_terminal(*observed)' in baseline.stderr, baseline.stderr
    cpp.write_text(stub)
    subprocess.run(['c++','-std=c++17','-Wall','-Wextra','-Werror','-pthread',str(cpp),'-o',str(exe)],check=True)
    subprocess.run([str(exe)],check=True,timeout=10)
print('actual task + pinned SDK destructor: old terminal predicate fails accepted-then-throw; new waits for callback and cleanup ACK')
print('SDK types are stubs; Windows SDK execution remains a separate gate')
