#pragma once
// Build configuration for the upstream public DA core in the test executable.
// No SDK source, licensing, or protocol logic is replaced.
#define SOFLAG_NO_DEFAULT_FEATURES
#define SOFLAG_NO_DLLS
#define SOFLAG_NO_FORCE_LIBS
#define SOFEATURE_DCOM
#define SOCLT
#define SODAC
#include <windows.h>
#include <tchar.h>
#include <atlbase.h>
#include "SOCmnCfg.h"
// This isolated probe covers native DA/DCOM only; it does not exercise
// XML-DA or SDK Watch. SOCmnCfg.h permits custom feature undefs.
// BEGIN N1 DCOM-ONLY FEATURES
#undef SOFEATURE_WATCH
#undef SOFEATURE_XML_METHOD
// END N1 DCOM-ONLY FEATURES
#if !defined(SOFEATURE_DCOM) || defined(SOFEATURE_SOAP) || \
    defined(SOFEATURE_WATCH) || defined(SOFEATURE_XML_METHOD) || !defined(SOFEATURE_CMN_XML) || \
    defined(XML_UNICODE) || defined(XML_UNICODE_WCHAR_T)
#error Invalid feature set for the DA/DCOM-only core probe
#endif
#include "SOCmnOsCompat.h"
#include "opcda.h"
#include "SOCmn.h"
#include "SOClt.h"
#include "SODaC.h"
