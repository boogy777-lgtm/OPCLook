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
#include "SOCmnOsCompat.h"
#include "opcda.h"
#include "SOCmn.h"
#include "SOClt.h"
#include "SODaC.h"
