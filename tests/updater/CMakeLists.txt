cmake_minimum_required(VERSION 3.16)
project(updater C CXX)
if(NOT WIN32)
  message(FATAL_ERROR "The updater is Windows-only for now (codemp/client/cl_update.cpp)")
endif()
get_filename_component(repo "${CMAKE_CURRENT_SOURCE_DIR}/../.." ABSOLUTE)
find_package(Python3 REQUIRED COMPONENTS Interpreter)

# Beside this test's client.h and jofupdatekey.h, cl_update.cpp builds against
# them instead of the engine's.
configure_file("${repo}/codemp/client/cl_update.cpp" "cl_update.cpp" COPYONLY)
configure_file(client.h client.h COPYONLY)
configure_file(jofupdatekey.h jofupdatekey.h COPYONLY)

set(zlib "${repo}/lib/zlib")
add_executable(updater_check check.cpp
  "${repo}/lib/cJSON/cJSON.c"
  "${repo}/lib/monocypher/monocypher.c"
  "${repo}/lib/monocypher/monocypher-ed25519.c"
  "${zlib}/adler32.c" "${zlib}/crc32.c" "${zlib}/inffast.c" "${zlib}/inflate.c" "${zlib}/inftrees.c" "${zlib}/zutil.c")
target_include_directories(updater_check PRIVATE "${CMAKE_CURRENT_BINARY_DIR}" "${repo}/lib/cJSON/include"
  "${repo}/lib/monocypher" "${zlib}/include" "${zlib}")
target_compile_definitions(updater_check PRIVATE _CRT_SECURE_NO_WARNINGS)
target_link_libraries(updater_check PRIVATE winhttp)

enable_testing()
add_test(NAME updater COMMAND updater_check "${repo}" "${Python3_EXECUTABLE}" "${CMAKE_CURRENT_BINARY_DIR}/work")
