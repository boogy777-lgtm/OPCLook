#pragma once
#define SOFLAG_NO_DEFAULT_FEATURES
#define SOFLAG_NO_DLLS
#define SOFLAG_NO_FORCE_LIBS
#define SOFEATURE_DCOM
#ifdef OPCL_FIXTURE_CLIENT
#define SOCLT
#define SODAC
#else
#define SOSRV
#define SODAS
#endif
#include <windows.h>
#include <tchar.h>
#include <atlbase.h>
extern CComModule _Module;
#include <atlcom.h>
#include "SOCmnCfg.h"
#undef SOFEATURE_WATCH
#undef SOFEATURE_XML_METHOD
#include "SOCmnOsCompat.h"
#include "opcda.h"
#include "SOCmn.h"
#ifdef OPCL_FIXTURE_CLIENT
#include "SOClt.h"
#include "SODaC.h"
#else
#include "SOSAllocHelpers.h"
#include "SOSrv.h"
#include "SODaS.h"
#endif
