#!/usr/bin/env python3
"""Check pinned SDK source path and compile actual fixture rights selector (not COM)."""
import argparse
from pathlib import Path
import subprocess
import tempfile

root = Path(__file__).resolve().parents[3]
parser = argparse.ArgumentParser()
parser.add_argument('--sdk', required=True, type=Path)
args = parser.parse_args()
head = subprocess.check_output(['git', '-C', str(args.sdk), 'rev-parse', 'HEAD'], text=True).strip()
assert head == '5130e763998189c822882434c43e6be2eb0174cf', head
sdk = args.sdk / 'development/core/src'
group = (sdk / 'SODaS/SODaSGroup.cpp').read_text()
com = (sdk / 'SODaS/SODaSComGroup.cpp').read_text()
client = (sdk / 'SODaC/SODaCComGroup.cpp').read_text()
fixture = (root / 'docs/testing/native-sdk/server-fixture/server.cpp').read_text()
assert 'accessRight = OPC_READABLE;' in group
rights = group[group.index('case SODaSRequest::asyncCache:'):]
assert 'tag->getAccessRights(srv) & accessRight' in rights
assert 'req->setResult(OPC_E_BADRIGHTS);' in rights
assert '(*errors)[i] = req->getResult();' in rights
assert 'addReqToList = FALSE;' in rights and 'res = S_FALSE;' in rights
read = com[com.index('STDMETHODIMP SODaSComGroup::ReadMaxAge(', com.index('IOPCAsyncIO2::Read')):]
assert 'createTransaction(TRUE' in read
assert 'tac->getRequestCount() > 0' in read
client = client[client.index('HRESULT SODaCComGroupImpl::asyncReadImpl('):]
assert 'pErrors[j] = errors[i];' in client
assert '_T("WriteOnly")' in fixture
start = fixture.index('        setAccessRights(write_only ?')
end = fixture.index('\n', start)
selector = fixture[start:end]
source = '''#include <cassert>\nconstexpr int OPC_READABLE=1, OPC_WRITEABLE=2;\nint rights; void setAccessRights(int r) { rights=r; }\nvoid select(bool write_only, bool read_only) {\n''' + selector + '''\n}\nint main(){select(true,false);assert(rights==OPC_WRITEABLE);select(false,true);assert(rights==OPC_READABLE);select(false,false);assert(rights==(OPC_READABLE|OPC_WRITEABLE));}\n'''
with tempfile.TemporaryDirectory() as temp:
    source_path = Path(temp) / 'fixture.cpp'
    executable = Path(temp) / 'fixture'
    source_path.write_text(source)
    subprocess.run(['c++', '-std=c++17', '-Wall', '-Wextra', '-Werror', str(source_path), '-o', str(executable)], check=True)
    subprocess.run([str(executable)], check=True)
print('fixture access selector compiled; pinned SDK admission/callback source path checked; no COM execution')
