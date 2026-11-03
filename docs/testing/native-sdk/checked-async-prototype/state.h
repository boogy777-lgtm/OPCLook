#pragma once
// Portable prototype only. No SDK, COM, production runtime, or retry loop.
#include <condition_variable>
#include <chrono>
#include <cstdint>
#include <limits>
#include <memory>
#include <mutex>

namespace checked_async_prototype {
enum class Outcome { pending, value, submit_error, cancelled, timeout };
struct State {
    std::mutex mutex;
    std::condition_variable changed;
    std::uint64_t next = 1, token = 0;
    Outcome terminal = Outcome::pending;
    bool worker_done = false, callback_done = false, submit_accepted = false;
    int value = 0;
    // Placeholder for the SDK host/session lease. The real SDK adapter must
    // own a native session lease; this stand uses a shared lifetime marker.
    std::shared_ptr<void> lease;
    explicit State(std::shared_ptr<void> l) : lease(std::move(l)) {}
    bool begin(std::uint64_t& out) {
        std::lock_guard<std::mutex> lock(mutex);
        if (token || next == 0 || next == std::numeric_limits<std::uint64_t>::max()) return false;
        out = token = next++; terminal = Outcome::pending;
        worker_done = callback_done = submit_accepted = false; return true;
    }
    void submit_result(std::uint64_t id, bool accepted) {
        std::lock_guard<std::mutex> lock(mutex);
        if (id != token) return;
        worker_done = true; submit_accepted = accepted;
        if (!accepted) { callback_done = true; complete(Outcome::submit_error); }
        else if (callback_done) complete(Outcome::value);
        changed.notify_all();
    }
    void removed(std::uint64_t id) {
        std::lock_guard<std::mutex> lock(mutex);
        if (id != token) return;
        worker_done = callback_done = true; complete(Outcome::cancelled); changed.notify_all();
    }
    void callback(std::uint64_t id, int v) {
        std::lock_guard<std::mutex> lock(mutex);
        if (id != token || callback_done) return;
        callback_done = true;
        if (terminal == Outcome::pending) { value = v; if (worker_done && submit_accepted) complete(Outcome::value); }
        changed.notify_all();
    }
    bool wait_and_expire(std::uint64_t id, std::chrono::milliseconds budget) {
        std::unique_lock<std::mutex> lock(mutex);
        if (id != token) return false;
        if (!changed.wait_for(lock, budget, [&] { return terminal != Outcome::pending; })) {
            complete(Outcome::timeout); changed.notify_all();
        }
        return true;
    }
    bool expire(std::uint64_t id) {
        std::lock_guard<std::mutex> lock(mutex);
        if (id != token || terminal != Outcome::pending) return false;
        complete(Outcome::timeout); changed.notify_all(); return true;
    }
    // Callback might never arrive; quarantine is intentional in that case.
    bool retire(std::uint64_t id) {
        std::lock_guard<std::mutex> lock(mutex);
        if (id != token || terminal == Outcome::pending || !worker_done || !callback_done) return false;
        token = 0; return true;
    }
private:
    void complete(Outcome result) { if (terminal == Outcome::pending) terminal = result; }
};
// Mirrors the extension-point obligation: queue destruction without execution
// must announce removal. Ownership is transferred by the SDK queue.
struct Task {
    std::shared_ptr<State> state;
    std::uint64_t token;
    bool executed = false;
    Task(std::shared_ptr<State> s, std::uint64_t t) : state(std::move(s)), token(t) {}
    ~Task() { if (!executed) state->removed(token); }
    void finished(bool accepted) { executed = true; state->submit_result(token, accepted); }
};
}
