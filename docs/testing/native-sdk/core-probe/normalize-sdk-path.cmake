# Normalise separators before CMake interpolates paths into VerifyGlobs.cmake.
function(normalize_sdk_path input output)
  string(REPLACE "\\" "/" normalized "${input}")
  if(WIN32)
    file(TO_CMAKE_PATH "${normalized}" normalized)
  endif()
  set(${output} "${normalized}" PARENT_SCOPE)
endfunction()
