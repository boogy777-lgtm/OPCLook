// Compile-only probe for Softing OPC Classic SDK v4.47.1 public headers.
// This does not link the SDK, connect to a server, or test COM lifecycle.
#include "OSCompat.h"
#include "client/Da/ClientDaSession.h"
#include "client/Da/ClientDaSubscription.h"
#include "client/Da/ClientDaItem.h"
#include "OTClient.h"
#include <type_traits>
#include <utility>
#include <vector>

using namespace SoftingOPCToolboxClient;
using Values = std::vector<ValueQT*>;
using Results = std::vector<long>;
using Items = std::vector<DaItem*>;
using Strings = std::vector<tstring>;
using BrowseElements = std::vector<DaAddressSpaceElement*>;

static_assert(MAX_AGE_DEVICE == 0, "source/device selector changed");
static_assert(MAX_AGE_CACHE == 0xffffffffUL, "cache selector changed");
static_assert(std::is_same<decltype(std::declval<DaSession&>().browse(
    std::declval<tstring>(), std::declval<tstring>(),
    static_cast<DaAddressSpaceElementBrowseOptions*>(nullptr),
    std::declval<BrowseElements&>(), static_cast<ExecutionOptions*>(nullptr))), long>::value,
    "browse result type changed");
static_assert(std::is_same<decltype(std::declval<DaSession&>().read(
    0UL, std::declval<Strings>(), std::declval<Strings>(),
    std::declval<Values&>(), std::declval<Results&>(),
    static_cast<ExecutionOptions*>(nullptr))), long>::value,
    "session read signature changed");
static_assert(std::is_same<decltype(std::declval<DaSubscription&>().read(
    0UL, std::declval<Items>(), std::declval<Values&>(),
    std::declval<Results&>(), static_cast<ExecutionOptions*>(nullptr))), long>::value,
    "subscription read signature changed");
static_assert(std::is_same<decltype(std::declval<DaSubscription&>().write(
    std::declval<Items>(), std::declval<Values>(), std::declval<Results&>(),
    static_cast<ExecutionOptions*>(nullptr))), long>::value,
    "subscription write signature changed");
static_assert(std::is_same<decltype(std::declval<ValueQT&>().getQuality()), EnumQuality>::value,
    "quality type changed");
static_assert(std::is_same<decltype(std::declval<ValueQT&>().getTimeStamp()), DateTime>::value,
    "timestamp type changed");
static_assert(std::is_same<decltype(OTCValueData::m_quality), unsigned short>::value,
    "raw DA quality width changed");
static_assert(std::is_same<decltype(OTCValueData::m_pValue), OTVariant*>::value,
    "typed native value pointer changed");
static_assert(std::is_same<decltype(OTCCallbackFunctions::m_OTCOnDataChange), OTCOnDataChange>::value,
    "value callback signature changed");
static_assert(std::is_same<decltype(OTCCallbackFunctions::m_OTCOnReadComplete), OTCOnReadComplete>::value,
    "read completion callback signature changed");
static_assert(std::is_same<decltype(OTCCallbackFunctions::m_OTCOnWriteComplete), OTCOnWriteComplete>::value,
    "write completion callback signature changed");
#ifdef _WIN32
static_assert(sizeof(long) == 4, "Windows HRESULT needs 32 bits");
static_assert(sizeof(void*) == 4, "probe must use the x86 compiler");
#endif
