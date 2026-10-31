#!/usr/bin/env python3
"""Exercise exact pinned SDK asyncCancel function with fake interface access.

This is a controlled source-branch reproduction, not a COM/server test.
No SDK interface or original code is modified by this script.
"""
import argparse
import hashlib
from pathlib import Path
import subprocess
import tempfile

BLOB = 'b6add51b268f0dc7a77a4873aa300ebed8c62d9e'
BEGIN = 'HRESULT SODaCComGroupImpl::asyncCancelImpl('
END = '} // asyncCancel'
OLD = 'res = asyncIO2->Cancel2(cancelID);'
NEW = 'res = (!!asyncIO3) ? asyncIO3->Cancel2(cancelID) : asyncIO2->Cancel2(cancelID);'

PRELUDE = r'''
#include <stdexcept>
using DWORD = unsigned long;
using HRESULT = long;
const HRESULT S_OK=0, E_NOINTERFACE=-1, CONNECT_E_NOCONNECTION=-2, E_ACCESSDENIED=-3;
#define IN
#define SOCLT_COM_CHECK_DEMO false
#define SODAC_ERR_GRP_NO_ASYNC_CON 1
#define SODAC_ERR_GRP_ASYNC_CANCEL 2
#define FAILED(x) ((x)<0)
#define _TRACEO(...) ((void)0)
struct IOPCAsyncIO { int calls=0; HRESULT Cancel(DWORD) { ++calls; return 0; } };
struct IOPCAsyncIO2 { int calls=0; HRESULT Cancel2(DWORD) { ++calls; return 0; } };
struct IOPCAsyncIO3 : IOPCAsyncIO2 {};
template<class T> struct CComPtr {
    T* p = nullptr;
    T* operator->() { if (!p) throw std::runtime_error("null asyncIO2"); return p; }
    bool operator!() const { return !p; }
};
template<class T> struct Access {
    T* p = nullptr;
    HRESULT getInterface(CComPtr<T>* out) { out->p=p; return p ? S_OK : E_NOINTERFACE; }
};
struct SODaCGroup {
    void setLastError(HRESULT, int) {}
    void beforeInterfaceCall() {}
    void afterInterfaceCall() {}
};
struct SODaCComGroupImpl {
    SODaCGroup group;
    void* m_dataCallback = nullptr;
    Access<IOPCAsyncIO> m_asyncIO;
    Access<IOPCAsyncIO2> m_asyncIO2;
    Access<IOPCAsyncIO3> m_asyncIO3;
    SODaCGroup* getObjectDaCGroup() { return &group; }
    bool getReadConnection(void*) { return true; }
    bool getWriteConnection(void*) { return true; }
    HRESULT asyncCancelImpl(DWORD);
};
'''
MAIN = r'''
int main() {
    IOPCAsyncIO io1; IOPCAsyncIO2 io2; IOPCAsyncIO3 io3;
    SODaCComGroupImpl g;
    g.m_dataCallback = &g;
    g.m_asyncIO2.p = &io2;
    if (g.asyncCancelImpl(23) != S_OK || io2.calls != 1) return 2;
    g.m_asyncIO3.p = &io3;
    try {
        if (g.asyncCancelImpl(24) != S_OK || io3.calls != 1 || io2.calls != 1) return 3;
    } catch (const std::runtime_error&) {
        return 42;
    }
    g.m_dataCallback = nullptr;
    g.m_asyncIO.p = &io1;
    if (g.asyncCancelImpl(25) != S_OK || io1.calls != 1) return 4;
    return 0;
}
'''


def blob(data):
    return hashlib.sha1(b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest()


def extract(data):
    if blob(data) != BLOB:
        raise ValueError('expected exact pinned SODaCComGroup.cpp Git blob')
    source = data.decode('utf-8')
    begin = source.index(BEGIN)
    end = source.index(END, begin) + len('} ')
    original = source[begin:end]
    if original.count(OLD) != 1:
        raise ValueError('unexpected cancellation implementation')
    return original


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', required=True, type=Path)
    parser.add_argument('--cxx', default='c++')
    args = parser.parse_args()
    upstream = args.source.read_bytes()
    original = extract(upstream)
    with tempfile.TemporaryDirectory(prefix='opcl-async-cancel-') as directory:
        root = Path(directory)
        sdk_file = root / 'development/core/src/SODaC/SODaCComGroup.cpp'
        sdk_file.parent.mkdir(parents=True)
        sdk_file.write_bytes(upstream)
        patch = Path(__file__).with_name('softing-async-cancel.patch').resolve()
        subprocess.run(['git', '-c', 'core.autocrlf=false', 'apply', '--unidiff-zero', '--whitespace=nowarn', str(patch)], cwd=root, check=True)
        patched = sdk_file.read_bytes()
        if patched != upstream.replace(OLD.encode(), NEW.encode(), 1):
            raise ValueError('patch changed more than the reviewed call')
        patched_source = patched.decode('utf-8')
        begin = patched_source.index(BEGIN)
        end = patched_source.index(END, begin) + len('} ')
        changed = patched_source[begin:end]
        for label, body, expected in [('original', original, 42), ('patched', changed, 0)]:
            source = root / (label + '.cpp')
            binary = root / label
            source.write_text(PRELUDE + body + '\n' + MAIN)
            subprocess.run([args.cxx, '-std=c++14', str(source), '-o', str(binary)], check=True)
            result = subprocess.run([str(binary)], check=False)
            if result.returncode != expected:
                raise RuntimeError(f'{label}: expected branch result {expected}, got {result.returncode}')
            print(f'{label}: controlled result {result.returncode}')
    print('Source-exact branch reproduced with stubs; real Windows COM behavior remains unmeasured')


if __name__ == '__main__':
    main()
