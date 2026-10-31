#!/usr/bin/env python3
"""Compile the real fixture SDK factory and device-counter branch with stubs."""
from pathlib import Path
import subprocess
import tempfile
src=(Path(__file__).resolve().parent/'server-fixture/server.cpp').read_text()
# SDK-held cache references must be released while common is still alive.
stop=src.index('entry->stop();')
term=src.index('entry->terminate()',stop)
for name in ('sync_device_read_count_cache','sync_read_request_count_cache'):
 assert stop < src.index(name + '.release();', stop) < term
a=src.index('class FixtureCreator : public SODaSCreator {')
b=src.index('\nstatic bool add_tag(',a)
creator=src[a:b]
a=src.index('        if (count_reads_ && request->getOperation()')
b=src.index('        if (delayed_ && request->getOperation()',a)
device=src[a:b]
stub=r'''
#include <atomic>
#include <cassert>
#include <string>
using BYTE=unsigned char; using WORD=unsigned short; using LONG=long;
constexpr int E_FAIL=-1,FALSE=0,OPC_QUALITY_UNCERTAIN=0;
#define _T(x) x
struct SOCmnVariant {LONG n=0; void setI4(LONG v){n=v;}};
struct SODaSCache {LONG count=0; bool setValue(SOCmnVariant& v){count=v.n;return true;}};
template<class T> struct SOCmnPointer { T* p=nullptr; bool isNotNull()const{return p!=nullptr;} T* operator->(){return p;} };
struct SODaSRequest {
 enum requestOperation {read,write};
 enum requestType {syncCache=0x1006,syncDevice=0x1005,syncMaxAge=0x1014,asyncDevice=0x1009,cyclic=0x9009,asyncInitNotify=0x120a};
 BYTE op; WORD type; bool failed=false; SODaSRequest(BYTE o,WORD t):op(o),type(t){}
 requestOperation getOperation(){return static_cast<requestOperation>(op);} requestType getType(){return static_cast<requestType>(type);}
 void complete(int,int,int,int,int){failed=true;} void release(){delete this;}
};
struct SODaSServer {}; struct SODaSItem {}; struct SODaSItemTag {std::string id; std::string getItemID(){return id;}};
struct SODaSCreator {virtual ~SODaSCreator()=default; virtual SODaSRequest* createRequest(SODaSServer*,SODaSItem*,SODaSItemTag*,BYTE op,WORD type){return new SODaSRequest(op,type);}};
SOCmnPointer<SODaSCache> device_read_count_cache, sync_device_read_count_cache,sync_read_request_count_cache;
std::atomic<unsigned> device_read_count{0},sync_device_read_count{0},sync_read_request_count{0};
'''+creator+r'''
void actual_device_branch(SODaSRequest* request) { bool count_reads_=true;
'''+device+r'''
}
int main(){ SODaSCache total,devices,requests; device_read_count_cache.p=&total; sync_device_read_count_cache.p=&devices; sync_read_request_count_cache.p=&requests;
 FixtureCreator creator; SODaSItemTag main{"Fixture.DeviceValue"},other{"Fixture.Value"},self{"Fixture.SyncReadRequestCount"};
 auto call=[&](SODaSItemTag* tag, BYTE op, WORD type, bool device){ SODaSRequest* r=creator.createRequest(nullptr,nullptr,tag,op,type); assert(r); if(device)actual_device_branch(r); assert(!r->failed); r->release();};
 call(&main,SODaSRequest::read,SODaSRequest::syncCache,false); assert(requests.count==1 && devices.count==0 && total.count==0);
 call(&main,SODaSRequest::read,SODaSRequest::syncDevice,true); assert(requests.count==2 && devices.count==1 && total.count==1);
 call(&main,SODaSRequest::read,SODaSRequest::syncMaxAge,true); assert(requests.count==3 && devices.count==2 && total.count==2);
 call(&main,SODaSRequest::read,SODaSRequest::cyclic,true); assert(requests.count==3 && devices.count==2 && total.count==3);
 call(&main,SODaSRequest::read,SODaSRequest::asyncDevice,true); assert(requests.count==3 && devices.count==2 && total.count==4);
 call(&main,SODaSRequest::read,SODaSRequest::asyncInitNotify,false); call(&other,SODaSRequest::read,SODaSRequest::syncCache,false);
 call(&self,SODaSRequest::read,SODaSRequest::syncDevice,false); call(&main,SODaSRequest::write,SODaSRequest::syncDevice,false);
 assert(requests.count==3 && devices.count==2 && total.count==4);
 sync_read_request_count.store(1000); assert(creator.createRequest(nullptr,nullptr,&main,SODaSRequest::read,SODaSRequest::syncCache)==nullptr);
 sync_device_read_count.store(1000); SODaSRequest exhausted(SODaSRequest::read,SODaSRequest::syncDevice);
 actual_device_branch(&exhausted); assert(exhausted.failed);
}
'''
with tempfile.TemporaryDirectory() as d:
 cpp=Path(d)/'test.cpp';exe=Path(d)/'test';cpp.write_text(stub)
 subprocess.run(['c++','-std=c++17','-Wall','-Wextra','-Werror',str(cpp),'-o',str(exe)],check=True)
 subprocess.run([str(exe)],check=True,timeout=10)
print('actual SDK fixture factory/device counter branches: sync cache/device/max-age vs cyclic/async/other/self')
print('SDK classes are stubs; actual Win32 fixture remains a separate gate')
