#include "state.h"
#include <cassert>
using DWORD = unsigned long; using HRESULT = long;
constexpr HRESULT E_FAIL = -1, E_UNEXPECTED = -2;
#define SUCCEEDED(x) ((x) >= 0)
struct SODaCItem { int refs=1; void addRef(){++refs;} void release(){--refs;} };
struct SODaCGroup { long asyncRead(int, SODaCItem**, DWORD, DWORD, DWORD*, HRESULT* item) { *item=0; return 0; } };
struct SOCltTask { explicit SOCltTask(SODaCGroup*) {} virtual ~SOCltTask()=default; virtual void execute()=0; void setResult(HRESULT) {} };
#include "sdk-task.h"
int main() {
    auto state=std::make_shared<checked_async_prototype::State>(std::make_shared<int>(1));
    std::uint64_t token=0; assert(state->begin(token)); SODaCGroup group; SODaCItem item;
    { checked_async_prototype::SdkReadTask task(&group,&item,0,1,state,token); assert(item.refs==2); }
    assert(item.refs==1 && state->terminal==checked_async_prototype::Outcome::cancelled && state->retire(token));
    assert(state->begin(token));
    { checked_async_prototype::SdkReadTask task(&group,&item,0,2,state,token); task.execute(); }
    assert(item.refs==1); state->callback(token,42);
    assert(state->terminal==checked_async_prototype::Outcome::value && state->retire(token));
}
