# Held subscription callback across a new Connect epoch

N3.LIFETIME/A08 uses the existing SDK callback barrier and the existing Gateway,
router, native session, manager and SDK worker. It does not add an owner or
production Async capability. Router Disconnect detaches the old session worker
before queuing teardown; a following Connect may start a new worker for the same
SessionId. SDK/COM internal locks may still prevent the intended interleaving.

`sdk_late_reconnect_live` requires old value88 captured in the actual subscription
callback, Disconnect and new Connect sent without waiting for old Disconnected,
then new Connected and item registration while `held()` still succeeds. A writer
connection without a subscription changes the current fixture value to99 before
reconnect. After release and old callback processing, new77→99 pushes and a fresh
Read99 must have the new epoch; any old88 attributed to that epoch fails. Session
shutdown must join and SDK/fixture cleanup must pass. Release-on-drop unblocks
the callback before Gateway cleanup on assertion failure. Timeouts or a reconnect
that succeeds only after retirement are not accepted as this gate.

The test-only held snapshot was tightened to require a claimed, unprocessed
callback as well as an unsignalled release event. The new regression failed with
the old helper (held before callback claim) and passed with the new actual C++
fragment using SDK/Windows event stubs. The real-file Linux runtime stand passed
54/54. The new live test body was typechecked against the real contract and
API-shaped stubs; this is not actual COM or real Gateway behavior. Windows i686
build, both COM inputs, overlap and strict cleanup are pending CI.

This is a local SDK fixture subscription case, distinct from accepted same-epoch
remove/re-add and Async Gateway reconnect after retirement. It does not establish
remote/vendor teardown, arbitrary crashes, production Async or bounded release
when SDK retirement never arrives.
