#!/usr/bin/env python3
"""Exercise the actual test-only gate code against local Windows/SDK stubs."""
from pathlib import Path
import subprocess
import tempfile
source = (Path(__file__).resolve().parents[3] / 'native/softing-da/runtime.cpp').read_text()
a = source.index('// One-shot callback barrier, before adapter registration lookup')
b = source.index('#endif\nvoid RuntimeGroup::onReadAdvise', a)
gate = source[a:b]
a = source.index('    TestLateCompletion late;', b)
b = source.index('    if (mode == 0) {', a)
entry = source[a:b]
stub = r'''
#include <atomic>
#include <cassert>
#include <chrono>
#include <condition_variable>
#include <cstdint>
#include <mutex>
#include <thread>
using DWORD=uint32_t; using LONG=int32_t; using BYTE=uint8_t; using HRESULT=int32_t;
constexpr HRESULT S_OK=0,E_FAIL=-1,E_PENDING=-2,E_UNEXPECTED=-3,E_OUTOFMEMORY=-4,E_INVALIDARG=-5;
constexpr DWORD WAIT_OBJECT_0=0,WAIT_TIMEOUT=258; constexpr int TRUE=1,FALSE=0,VT_I4=3;
struct Event {std::mutex mutex; std::condition_variable cv; bool set=false;}; using HANDLE=Event*;
HANDLE CreateEventW(void*,int,int,void*) {return new Event;}
int CloseHandle(HANDLE h) {delete h; return 1;}
int SetEvent(HANDLE h) {if(!h)return 0; std::lock_guard<std::mutex> lock(h->mutex); h->set=true; h->cv.notify_all(); return 1;}
DWORD WaitForSingleObject(HANDLE h,DWORD timeout) {if(!h)return 999; std::unique_lock<std::mutex> lock(h->mutex); return h->cv.wait_for(lock,std::chrono::milliseconds(timeout),[&]{return h->set;})?WAIT_OBJECT_0:WAIT_TIMEOUT;}
struct Item {}; struct Variant {int vt; int32_t lVal;};
'''+gate+r'''
void callback(BYTE mode, DWORD transaction, DWORD n, Item** items, Variant* values) {
'''+entry+r'''
}
int main() {
 assert(opcl_da_test_late_arm(88)==S_OK); assert(opcl_da_test_late_entered(0)==E_INVALIDARG); Item item; Item* items[]={&item}; Variant not_target{VT_I4,99};
 callback(1,0,1,items,&not_target); assert(opcl_da_test_late_entered(1)==E_FAIL);
 Variant target{VT_I4,88}; std::thread worker([&]{callback(1,0,1,items,&target);});
 assert(opcl_da_test_late_entered(1000)==S_OK); assert(opcl_da_test_late_held()==S_OK);
 assert(opcl_da_test_late_processed(1)==E_PENDING); assert(opcl_da_test_late_close()==E_PENDING);
 assert(opcl_da_test_late_release()==S_OK); assert(opcl_da_test_late_processed(1000)==S_OK);
 worker.join(); assert(opcl_da_test_late_close()==S_OK);
}
'''
with tempfile.TemporaryDirectory() as d:
    cpp=Path(d)/'gate.cpp'; exe=Path(d)/'gate'; cpp.write_text(stub)
    subprocess.run(['c++','-std=c++17','-Wall','-Wextra','-Werror','-pthread',str(cpp),'-o',str(exe)],check=True)
    subprocess.run([str(exe)],check=True,timeout=10)
print('actual test-only callback gate: wrong value ignored; entered/held/release/processed/close')
print('Windows events and SDK callback are stubs; real Windows fixture remains pending')
