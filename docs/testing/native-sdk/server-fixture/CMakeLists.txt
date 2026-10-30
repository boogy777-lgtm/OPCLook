cmake_minimum_required(VERSION 3.20)
project(opclook_da_interop_fixture LANGUAGES C CXX)
if(NOT WIN32 OR NOT CMAKE_SIZEOF_VOID_P EQUAL 4)
  message(FATAL_ERROR "This interop fixture requires Windows x86")
endif()
if(NOT DEFINED SOFTING_SDK_ROOT)
  message(FATAL_ERROR "Set SOFTING_SDK_ROOT to the pinned SDK checkout")
endif()
include("${CMAKE_CURRENT_SOURCE_DIR}/../core-probe/normalize-sdk-path.cmake")
normalize_sdk_path("${SOFTING_SDK_ROOT}" sdk)
find_package(Python3 REQUIRED COMPONENTS Interpreter)
execute_process(COMMAND "${CMAKE_COMMAND}" -E env "SOFTING_SDK_ROOT=${sdk}"
  "${Python3_EXECUTABLE}" "${CMAKE_CURRENT_SOURCE_DIR}/../verify-softing-sdk.py" --core-clean
  RESULT_VARIABLE pin_result)
if(NOT pin_result EQUAL 0)
  message(FATAL_ERROR "SDK pin/source verification failed")
endif()
set(core "${sdk}/development/core")
file(GLOB common_sources CONFIGURE_DEPENDS "${core}/src/SOCmn/*.cpp")
if(DEFINED OPCF_MIT_GENERATED)
  include("${CMAKE_CURRENT_SOURCE_DIR}/../core-probe/opcf-mit-inputs.cmake")
  opcf_mit_inputs(opc_sources opc_include)
else()
  file(GLOB opc_sources CONFIGURE_DEPENDS "${core}/src/opc/*_i.c")
  set(opc_include "${core}/src/opc")
endif()
list(APPEND common_sources ${opc_sources})
include("${CMAKE_CURRENT_SOURCE_DIR}/../core-probe/filter-embedded-expat.cmake")
filter_embedded_expat(common_sources)
set(expat "${core}/src/SOCmn/expat")
add_library(fixture_expat STATIC "${expat}/xmlrole.c" "${expat}/xmltok.c" "${expat}/xmlparse.c")
target_include_directories(fixture_expat PRIVATE "${core}/src/include" "${expat}")
target_compile_definitions(fixture_expat PRIVATE XML_STATIC COMPILED_FROM_DSP)
file(GLOB server_sources CONFIGURE_DEPENDS "${core}/src/SOSrv/*.cpp" "${core}/src/SODaS/*.cpp")
file(GLOB client_sources CONFIGURE_DEPENDS "${core}/src/SOClt/*.cpp" "${core}/src/SODaC/*.cpp")
add_executable(opcl_da_server server.cpp ${common_sources} ${server_sources}
  "${core}/src/demo/SOServer/SOSAllocHelpers.cpp" "${core}/src/include/SOProxy_i.c")
target_compile_definitions(opcl_da_server PRIVATE SODMS_DA UNICODE _UNICODE)
set_source_files_properties("${core}/src/demo/SOServer/SOSAllocHelpers.cpp" PROPERTIES
  COMPILE_OPTIONS "/FI${CMAKE_CURRENT_SOURCE_DIR}/stdafx.h")
add_executable(opcl_da_sdk_client client-smoke.cpp ${common_sources} ${client_sources})
target_compile_definitions(opcl_da_sdk_client PRIVATE OPCL_FIXTURE_CLIENT)
foreach(target opcl_da_server opcl_da_sdk_client)
  target_include_directories(${target} PRIVATE "${CMAKE_CURRENT_SOURCE_DIR}"
    "${core}/src/include" "${opc_include}" "${core}/src/common" "${core}/common"
    "${core}/src/demo/SOServer" "${core}/src/SOSrv" "${core}/src/SODaS")
  target_compile_definitions(${target} PRIVATE _WINDOWS WIN32 _WIN32_DCOM
    _CRT_SECURE_NO_WARNINGS _CRT_NON_CONFORMING_SWPRINTFS)
  target_compile_options(${target} PRIVATE $<$<COMPILE_LANGUAGE:CXX>:/W3>
    $<$<COMPILE_LANGUAGE:CXX>:/Zc:wchar_t-> $<$<COMPILE_LANGUAGE:CXX>:/EHsc>)
  target_link_libraries(${target} PRIVATE fixture_expat ole32 oleaut32 uuid advapi32 ws2_32 mpr rpcrt4)
endforeach()

# The Windows image need not have the OPC Foundation Core Components installed.
# Generate only the RPC proxy/stub from the pinned original interface IDL.
if(DEFINED OPCF_MIT_GENERATED)
  add_library(opcl_opcproxy SHARED proxy-list-mit.c proxy.def
    "${OPCF_MIT_GENERATED}/opccomn_p.c" "${OPCF_MIT_GENERATED}/opccomn_i.c"
    "${OPCF_MIT_GENERATED}/opcda_p.c" "${OPCF_MIT_GENERATED}/opcda_i.c")
  target_include_directories(opcl_opcproxy PRIVATE "${OPCF_MIT_GENERATED}")
  foreach(target opcl_da_server opcl_da_sdk_client opcl_opcproxy)
    add_dependencies(${target} opcf_mit_guard)
    target_compile_options(${target} PRIVATE /showIncludes)
    target_link_options(${target} PRIVATE /VERBOSE:LIB)
  endforeach()
else()
find_program(MIDL_EXECUTABLE midl REQUIRED)
set(generated "${CMAKE_CURRENT_BINARY_DIR}/rpc")
file(MAKE_DIRECTORY "${generated}")
foreach(idl OPCCOMN OPCDA)
  add_custom_command(OUTPUT "${generated}/${idl}_p.c" "${generated}/${idl}_i.c" "${generated}/${idl}.h"
    COMMAND "${MIDL_EXECUTABLE}" /nologo /env win32 /robust /Oicf
      /I "${core}/src/opc" /out "${generated}" /h "${idl}.h"
      /iid "${idl}_i.c" /proxy "${idl}_p.c" /dlldata "${idl}_dlldata.c"
      "${core}/src/opc/${idl}.IDL"
    DEPENDS "${core}/src/opc/${idl}.IDL" VERBATIM)
endforeach()
add_library(opcl_opcproxy SHARED proxy-list.c proxy.def
  "${generated}/OPCCOMN_p.c" "${generated}/OPCCOMN_i.c"
  "${generated}/OPCDA_p.c" "${generated}/OPCDA_i.c")
target_include_directories(opcl_opcproxy PRIVATE "${generated}")
endif()
target_compile_definitions(opcl_opcproxy PRIVATE REGISTER_PROXY_DLL _WIN32_WINNT=0x0601)
target_link_libraries(opcl_opcproxy PRIVATE rpcrt4 oleaut32 uuid)
