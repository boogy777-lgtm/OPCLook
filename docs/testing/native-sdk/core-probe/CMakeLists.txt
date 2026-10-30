cmake_minimum_required(VERSION 3.20)
project(softing_core_probe LANGUAGES C CXX)
if(NOT WIN32 OR NOT CMAKE_SIZEOF_VOID_P EQUAL 4)
  message(FATAL_ERROR "This SDK probe requires the Windows x86 toolchain")
endif()
if(NOT DEFINED SOFTING_SDK_ROOT)
  message(FATAL_ERROR "Set SOFTING_SDK_ROOT to the pinned SDK checkout")
endif()
include("${CMAKE_CURRENT_SOURCE_DIR}/normalize-sdk-path.cmake")
normalize_sdk_path("${SOFTING_SDK_ROOT}" SOFTING_SDK_ROOT_NORMALIZED)
find_package(Python3 REQUIRED COMPONENTS Interpreter)
execute_process(COMMAND "${CMAKE_COMMAND}" -E env "SOFTING_SDK_ROOT=${SOFTING_SDK_ROOT_NORMALIZED}"
  "${Python3_EXECUTABLE}" "${CMAKE_CURRENT_SOURCE_DIR}/../verify-softing-sdk.py"
  RESULT_VARIABLE pin_result)
if(NOT pin_result EQUAL 0)
  message(FATAL_ERROR "Softing SDK pin/header verification failed")
endif()
set(core "${SOFTING_SDK_ROOT_NORMALIZED}/development/core")
file(GLOB core_sources CONFIGURE_DEPENDS
  "${core}/src/SOCmn/*.cpp" "${core}/src/SOClt/*.cpp" "${core}/src/SODaC/*.cpp"
  "${core}/src/opc/*_i.c")
# SDK embeds Expat sources into one C++ TU. Build the same pinned C sources
# as separate C translation units so winconfig.h controls its own Windows
# includes and its private types do not collide with the SDK headers.
include("${CMAKE_CURRENT_SOURCE_DIR}/filter-embedded-expat.cmake")
filter_embedded_expat(core_sources)
set(expat "${core}/src/SOCmn/expat")
add_library(softing_expat STATIC "${expat}/xmlrole.c" "${expat}/xmltok.c" "${expat}/xmlparse.c")
target_include_directories(softing_expat PRIVATE "${core}/src/include" "${expat}")
target_compile_definitions(softing_expat PRIVATE XML_STATIC COMPILED_FROM_DSP)
add_executable(softing_expat_smoke expat-smoke.c)
target_include_directories(softing_expat_smoke PRIVATE "${core}/src/include")
target_compile_definitions(softing_expat_smoke PRIVATE XML_STATIC)
target_link_libraries(softing_expat_smoke PRIVATE softing_expat)
add_executable(softing_core_probe main.cpp ../../../../native/softing-da/adapter.cpp ../../../../native/softing-da/runtime.cpp ${core_sources})
target_include_directories(softing_core_probe PRIVATE
  "${CMAKE_CURRENT_SOURCE_DIR}" "${CMAKE_CURRENT_SOURCE_DIR}/../../../../native/softing-da" "${core}/src/include" "${core}/src/opc"
  "${core}/src/common" "${core}/common")
target_compile_definitions(softing_core_probe PRIVATE
  _WINDOWS WIN32 _WIN32_DCOM _CRT_SECURE_NO_WARNINGS _CRT_NON_CONFORMING_SWPRINTFS)
target_compile_options(softing_core_probe PRIVATE /W3 /Zc:wchar_t- /EHsc)
target_link_libraries(softing_core_probe PRIVATE softing_expat ole32 oleaut32 uuid advapi32 ws2_32 mpr rpcrt4)
