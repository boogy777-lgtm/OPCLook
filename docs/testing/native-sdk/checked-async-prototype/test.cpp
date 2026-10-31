#include "state.h"
#include "admission.h"
#include <cassert>
#include <chrono>
#include <condition_variable>
#include <mutex>
#include <thread>
using namespace checked_async_prototype;
struct Worker { bool own=false, running=true, accept=true; int calls=0;
 bool isOwnThread() {return own;} bool isRunning() {return running;}
 bool addTask(Task*) {++calls; return accept;} };
int main() {
    auto lease = std::make_shared<int>(42); std::weak_ptr<int> weak = lease;
    auto state = std::make_shared<State>(lease); lease.reset();
    std::uint64_t a = 0, b = 0; assert(state->begin(a) && !state->begin(b));
    Worker w; { Task t(state, 0); w.own=true; assert(enqueue(&w, &t)==Admission::unsupported && w.calls==0);
    w.own=false; w.running=false; assert(enqueue(&w, &t)==Admission::unsupported && w.calls==0);
    w.running=true; w.accept=false; assert(enqueue(&w, &t)==Admission::rejected && w.calls==1);
    w.accept=true; assert(enqueue(&w, &t)==Admission::accepted && w.calls==2); }
    // A queued task discarded before execute reports exactly one cancellation.
    { Task task(state, a); } assert(state->terminal == Outcome::cancelled);
    state->callback(a, 7); assert(state->terminal == Outcome::cancelled && state->retire(a));
    assert(state->begin(b) && b != a);
    // Controlled blocked worker; timeout does not release the session lease.
    std::mutex mutex; std::condition_variable cv; bool entered = false, release = false;
    std::thread worker([&] { Task task(state, b); { std::unique_lock<std::mutex> lock(mutex);
        entered = true; cv.notify_all(); cv.wait(lock, [&] { return release; }); }
        task.finished(true); });
    { std::unique_lock<std::mutex> lock(mutex); cv.wait(lock, [&] { return entered; }); }
    assert(state->wait_and_expire(b, std::chrono::milliseconds(10)) && state->terminal == Outcome::timeout);
    assert(!state->retire(b) && !state->begin(a) && !weak.expired());
    state->callback(b - 1, 11); assert(state->terminal == Outcome::timeout);
    { std::lock_guard<std::mutex> lock(mutex); release = true; cv.notify_all(); }
    worker.join(); assert(!state->retire(b));
    state->callback(b, 12); state->callback(b, 13);
    assert(state->terminal == Outcome::timeout && state->retire(b));
    assert(state->begin(a)); state->callback(a, 21); state->submit_result(a, true);
    assert(state->terminal == Outcome::value && state->value == 21 && state->retire(a));
    assert(state->begin(b)); state->callback(b, 33); state->submit_result(b, false);
    assert(state->terminal == Outcome::submit_error && state->retire(b));
    state.reset(); assert(weak.expired());
}
