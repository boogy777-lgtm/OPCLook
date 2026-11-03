#pragma once
// Proposal using the SDK worker's public methods; callers MUST serialize
// admission with session close/worker shutdown and hold their session lease.
// This does not provide an absolute OS scheduling or queue-lock deadline.
namespace checked_async_prototype {
enum class Admission { accepted, unsupported, rejected };
template<class Worker, class Task>
Admission enqueue(Worker* worker, Task* task) {
    if (!worker || !task || worker->isOwnThread() || !worker->isRunning()) return Admission::unsupported;
    // The SDK synchronized task queue takes its own reference on successful
    // add; the caller retains its own reference across this call.
    return worker->addTask(task) ? Admission::accepted : Admission::rejected;
}
}
