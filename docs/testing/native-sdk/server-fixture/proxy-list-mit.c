// RPC glue for the opt-in MIDL outputs named from official OPCF MIT IDLs.
#define PROXY_DELEGATION
#include <rpcproxy.h>
EXTERN_PROXY_FILE(opccomn)
EXTERN_PROXY_FILE(opcda)
PROXYFILE_LIST_START
    REFERENCE_PROXY_FILE(opccomn),
    REFERENCE_PROXY_FILE(opcda),
PROXYFILE_LIST_END
DLLDATA_ROUTINES(aProxyFileList, GET_DLL_CLSID)
