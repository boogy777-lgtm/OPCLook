#!/usr/bin/env python3
"""Compile the pinned SDK's unmodified executeTask against controlled stubs."""
import argparse
import hashlib
from pathlib import Path
import subprocess
import tempfile

p = argparse.ArgumentParser()
p.add_argument('--element-source', required=True, type=Path)
p.add_argument('--thread-source', required=True, type=Path)
p.add_argument('--task-source', required=True, type=Path)
a = p.parse_args()
def read(path, blob):
    data = path.read_bytes()
    actual = hashlib.sha1(b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest()
    if actual != blob:
        raise SystemExit(f'{path}: SDK source blob mismatch: {actual}')
    return data.decode('utf-8')
element = read(a.element_source, '990515c8d9e83d35bf039a44bdc4a5a0baac5676')
thread = read(a.thread_source, 'b94736d43dd247feffbeecea0976339c05f71a3f')
task = read(a.task_source, 'c158c67ff4cd50f59258ec1f46525b883a103580')
start = element.index('LONG SOCltElement::executeTask(')
end = element.index('} // executeTask', start) + 1
function = element[start:end]
assert 'm_taskQueue.removeAll();' in thread[thread.index('void SOCltWorkerThread::shutdown'):thread.index('void SOCltWorkerThread::cleanUp')]
assert 'element->addRef();' in task and 'm_element.release();' in task
harness = r'''
#include <cassert>
#include <atomic>
#include <condition_variable>
#include <mutex>
#include <thread>
using LONG = long; using BOOL = int;
#define IN
#define OPTIONAL
#define _ASSERT_PTR(x) assert(x)
#define _ASSERTION(x,y) assert(x)
#define S_OK 0
#define S_FALSE 1
#define SOCLT_EXEC_AUTO_SYNC 1
#define SOCLT_EXEC_ASYNC 0
struct SOCmnEvent;
struct SOCmnEventPointer { bool isInit() const {return false;} void set(SOCmnEventPointer) {} void waitFor() {} SOCmnEventPointer() {} SOCmnEventPointer(SOCmnEvent*) {} };
struct SOCmnEvent { void create() {} };
struct SOCltTask { virtual void execute() = 0; LONG getResult() {return 17;} void setCompletionEvent(SOCmnEventPointer) {} };
struct SOCltWorkerThread { bool own = false; SOCltTask* queued = nullptr; bool isOwnThread() {return own;} bool addTask(SOCltTask* t) {queued = t; return true;} };
struct SOCltElement { SOCltWorkerThread* worker = nullptr; SOCltWorkerThread* getWorkerThread() {return worker;} LONG executeTask(SOCltTask*, BOOL, SOCmnEventPointer); };
'''+function+r'''
struct Blocked : SOCltTask {
 std::mutex m; std::condition_variable cv; bool entered=false, released=false;
 void execute() override { std::unique_lock<std::mutex> l(m); entered=true; cv.notify_all(); cv.wait(l,[&]{return released;}); }
 void wait_entered() { std::unique_lock<std::mutex> l(m); cv.wait(l,[&]{return entered;}); }
 void release() { std::lock_guard<std::mutex> l(m); released=true; cv.notify_all(); }
};
int main() {
 SOCltElement e; SOCltWorkerThread w; e.worker=&w; Blocked queued;
 assert(e.executeTask(&queued, SOCLT_EXEC_ASYNC, {})==S_OK);
 assert(w.queued==&queued && !queued.entered);
 for (int mode=0;mode<2;mode++) {
  Blocked inline_task; w.own=(mode==0); e.worker=mode==0?&w:nullptr;
  std::atomic<bool> returned{false};
  std::thread caller([&]{assert(e.executeTask(&inline_task, SOCLT_EXEC_ASYNC, {})==17); returned=true;});
  inline_task.wait_entered(); assert(!returned.load()); inline_task.release(); caller.join(); assert(returned.load());
 }
}
'''
with tempfile.TemporaryDirectory() as d:
    cpp = Path(d) / 'probe.cpp'; exe = Path(d) / 'probe'
    cpp.write_text(harness)
    subprocess.run(['c++', '-std=c++17', '-pthread', str(cpp), '-o', str(exe)], check=True)
    subprocess.run([str(exe)], check=True, timeout=10)
print('pinned executeTask: queued async returns; same-worker and absent-worker execute inline before return; shutdown removes queued tasks')
print('controlled SDK-source/stub test only; no Win32 or OPC server was exercised')
