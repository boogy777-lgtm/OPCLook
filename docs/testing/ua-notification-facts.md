# R06/R07: UA notification facts

The producer wraps the locked `async-opcua-client 0.19.0` public
`OnSubscriptionNotificationCore`, reads the SDK `NotificationMessage`, and
then delegates the unchanged message and monitored-item map to the SDK's
`DataChangeCallback`. The SDK continues to own Publish, acknowledgement,
reconnect, transfer/recreation and any replay; OPCLook adds none of those loops.

OPC UA Part 4 §5.14.1.1 specifies notification sequences 1 through
`u32::MAX` and rollover to 1. A keepalive has no notifications and advertises
the next sequence; it does not advance the observed notification cursor. A
nonempty status-change notification does advance it. For each SDK callback
instance, this adapter stores one cursor scoped by recovery revision. The first
message establishes a baseline; duplicate and adjacent wrap are not gaps.
A forward jump emits `SequenceGap` with observed endpoints, without a lost-value
count or claim that the server cannot republish. A zero, backwards or
non-adjacent wrap emits `SequenceUncertain`, without rewinding the cursor.
Recovery revision changes reset this comparison baseline; interruption itself
still means history is unknown. No fresh snapshot repairs earlier history.

For monitored DataValues, the SDK's typed `StatusCode::info_type()` and
`overflow()` are both checked. The raw status and ItemKey are emitted separately
from the normal value, including when its severity is Good. Notification facts
and values carry `Origin::UaSession { connection, revision }`; generation is a
bounded adapter callback-instance identifier, not an OPC server subscription ID
or a source-clock identity. The existing recovery gate and registration set
protect retired keys. The raw callback captures the notification revision
before SDK delegation; the delegated per-item callback checks that same revision
again, so a loss and reconnect between them cannot relabel old data. Consumers
must discard stale scopes and retain incomplete
history independently of current quality, including facts before Add ACK.

The live tests use an in-process SDK sample server. One writes a Good DataValue
with overflow info bits; another test-only None-security proxy alters the
sequence of one real PublishResponse using SDK codecs and exercises the real
SDK callback/delegate path. These are sampled tests, not proof of automatic
Republish, exact loss count, secure-endpoint behaviour or complete history.
The existing strict first post-reactivation notification test remains a known
failure/ignored observation; a later value or a successful snapshot does not
repair it. Full integration and Windows CI belong to their exact published head.
