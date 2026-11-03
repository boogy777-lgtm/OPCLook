#!/usr/bin/env python3
"""Guard O26 assumptions against the actual pinned client and fixture SDK sources."""
import os
from pathlib import Path
import subprocess

root = Path(__file__).resolve().parents[3]
sdk = Path(os.environ['SOFTING_SDK_ROOT'])
subprocess.run(['python3', str(Path(__file__).with_name('verify-softing-sdk.py')), '--core-clean'], check=True)
client = (sdk / 'development/core/src/SODaC/SODaCComGroup.cpp').read_text()
server = (sdk / 'development/core/src/SODaS/SODaSComGroup.cpp').read_text()
group = (sdk / 'development/core/src/SODaS/SODaSGroup.cpp').read_text()
request = (sdk / 'development/core/src/include/SODaSRequest.h').read_text()
runtime = (root / 'native/softing-da/runtime.cpp').read_text()
fixture = (root / 'docs/testing/native-sdk/server-fixture/server.cpp').read_text()
test = (root / 'crates/backend/tests/sdk_async_versions_live.rs').read_text()
start = client.index('HRESULT SODaCComGroupImpl::asyncReadImpl')
end = client.index('HRESULT SODaCComGroupImpl::asyncWriteImpl', start)
route = client[start:end]
for fragment in ['getReadConnection(&connectionId)', 'm_asyncIO3.getInterface(&asyncIO3)',
                 'm_asyncIO2.getInterface(&asyncIO2)', 'asyncIO3->ReadMaxAge(', 'asyncIO2->Read(']:
    assert fragment in route, fragment
assert route.index('m_asyncIO3.getInterface') < route.index('m_asyncIO2.getInterface')
assert 'm_asyncIO2 = (IOPCAsyncIO2*) mqi[3].pItf' in client
assert 'm_asyncIO3 = (IOPCAsyncIO3*) mqi[7].pItf' in client
v2 = client[client.index('case SODAC_VERSION_OPC_DAV20:'):client.index('m_itemMgt =', client.index('case SODAC_VERSION_OPC_DAV20:'))]
assert 'mqi[7].pItf = NULL' in v2 and 'mqi[3].pItf = NULL' not in v2
assert 'IOPCAsyncIO2::Read' in server and 'OPC_DS_DEVICE, srv->getWorkEvent(), dwNumItems' in server
assert 'IOPCAsyncIO3::ReadMaxAge' in server and 'count, phServer, NULL, NULL, pMaxAge' in server
assert 'reqType = SODaSRequest::asyncDevice;' in group and 'reqType = SODaSRequest::asyncMaxAge;' in group
assert 'asyncInitNotify' in request and 'cyclic ' in request
assert 'test_version && !test_started ? group->connect(FALSE) : group->start(FALSE)' in runtime
assert 'server->getForcedOPCVersion() != test_version' in runtime and 'server->getSupportedOPCVersion() != test_version' in runtime
assert 'Fixture.AsyncRoute' in fixture and 'type == SODaSRequest::asyncDevice || type == SODaSRequest::asyncMaxAge' in fixture
assert 'let before = lease.route_counts()' in test and 'let after = lease.route_counts()' in test
assert '(0x0200, (1, 0)), (0x0300, (0, 1))' in test
print('pinned SDK client/server route and fixture guards passed; no COM execution')
