cmake_minimum_required(VERSION 3.20)
project(softing_core_probe LANGUAGES C CXX)
if(NOT WIN32 OR NOT CMAKE_SIZEOF_VOID_P EQUAL 4)
  message(FATAL_ERROR "This SDK probe requires the Windows x86 toolchain")
endif()
if(NOT DEFINED SOFTING_SDK_ROOT)
  message(FATAL_ERROR "Set SOFTING_SDK_ROOT to the pinned SDK checkout")
endif()
find_package(Python3 REQUIRED COMPONENTS Interpreter)
execute_process(COMMAND "${CMAKE_COMMAND}" -E env "SOFTING_SDK_ROOT=${SOFTING_SDK_ROOT}"
  "${Python3_EXECUTABLE}" "${CMAKE_CURRENT_SOURCE_DIR}/../verify-softing-sdk.py"
  RESULT_VARIABLE pin_result)
if(NOT pin_result EQUAL 0)
  message(FATAL_ERROR "Softing SDK pin/header verification failed")
endif()
set(core "${SOFTING_SDK_ROOT}/development/core")
file(GLOB core_sources CONFIGURE_DEPENDS
  "${core}/src/SOCmn/*.cpp" "${core}/src/SOClt/*.cpp" "${core}/src/SODaC/*.cpp"
  "${core}/src/opc/*_i.c")
add_executable(softing_core_probe main.cpp ${core_sources})
target_include_directories(softing_core_probe PRIVATE
  "${CMAKE_CURRENT_SOURCE_DIR}" "${core}/src/include" "${core}/src/opc"
  "${core}/src/common" "${core}/common")
target_compile_definitions(softing_core_probe PRIVATE
  _WINDOWS WIN32 _WIN32_DCOM _CRT_SECURE_NO_WARNINGS _CRT_NON_CONFORMING_SWPRINTFS)
target_compile_options(softing_core_probe PRIVATE /W3 /Zc:wchar_t- /EHsc)
target_link_libraries(softing_core_probe PRIVATE ole32 oleaut32 uuid advapi32 ws2_32 mpr rpcrt4)
