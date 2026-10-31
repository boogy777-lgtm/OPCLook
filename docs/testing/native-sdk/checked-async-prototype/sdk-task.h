#pragma once
// SDK-backed task proposal. Include after the SDK SODaC headers and state.h.
// Not wired into runtime: a true native session lease and callback bridge are
// required before this can be used. No caller stack storage is retained.
namespace checked_async_prototype {
class SdkReadTask final : public SOCltTask {
public:
    SdkReadTask(SODaCGroup* group, SODaCItem* item, DWORD age, DWORD transaction,
                std::shared_ptr<State> state, std::uint64_t token)
        : SOCltTask(group), group_(group), item_(item), age_(age),
          transaction_(transaction), state_(std::move(state)), token_(token) { item_->addRef(); }
    ~SdkReadTask() override {
        if (!executed_) state_->removed(token_);
        item_->release();
    }
    void execute() override {
        executed_ = true;
        HRESULT hr = E_UNEXPECTED, item_result = E_FAIL;
        DWORD cancel = 0;
        try {
            SODaCItem* items[] = {item_};
            hr = group_->asyncRead(1, items, age_, transaction_, &cancel, &item_result);
        } catch (...) { hr = E_UNEXPECTED; }
        setResult(hr);
        state_->submit_result(token_, SUCCEEDED(hr) && SUCCEEDED(item_result));
    }
private:
    SODaCGroup* group_;
    SODaCItem* item_;
    DWORD age_, transaction_;
    std::shared_ptr<State> state_;
    std::uint64_t token_;
    bool executed_ = false;
};
}
