cmake_minimum_required(VERSION 3.16)
project(sjk_chat C CXX)
if(NOT WIN32)
  message(FATAL_ERROR "SJK chat is Windows-only for now (codemp/client/cl_sjk.cpp)")
endif()
get_filename_component(repo "${CMAKE_CURRENT_SOURCE_DIR}/../.." ABSOLUTE)

# Beside this test's client.h, cl_sjk.cpp builds against it instead of the engine's.
configure_file("${repo}/codemp/client/cl_sjk.cpp" "cl_sjk.cpp" COPYONLY)
configure_file(client.h client.h COPYONLY)

# The engine's own charset conversion, cut out of the SDL input code it lives in.
set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS "${repo}/shared/sdl/sdl_input.cpp")
file(READ "${repo}/shared/sdl/sdl_input.cpp" input)
string(FIND "${input}" "uint8_t ConvertUTF32ToExpectedCharset" start)
string(FIND "${input}" "void SNDDMA_Activate( qboolean activate );" end)
if(start LESS 0 OR end LESS start)
  message(FATAL_ERROR "Cannot find ConvertUTF32ToExpectedCharset in shared/sdl/sdl_input.cpp")
endif()
math(EXPR length "${end} - ${start}")
string(SUBSTRING "${input}" ${start} ${length} charset)
file(WRITE "${CMAKE_CURRENT_BINARY_DIR}/charset.cpp" "#include <cstdint>\n${charset}\n")

add_executable(sjk_chat_check check.cpp "${CMAKE_CURRENT_BINARY_DIR}/charset.cpp"
  "${repo}/lib/cJSON/cJSON.c"
  "${repo}/lib/monocypher/monocypher.c"
  "${repo}/lib/monocypher/monocypher-ed25519.c")
target_include_directories(sjk_chat_check PRIVATE "${CMAKE_CURRENT_BINARY_DIR}" "${repo}/lib/cJSON/include" "${repo}/lib/monocypher")
target_compile_definitions(sjk_chat_check PRIVATE _CRT_SECURE_NO_WARNINGS)
target_link_libraries(sjk_chat_check PRIVATE winhttp bcrypt shell32)

enable_testing()
add_test(NAME sjk_chat COMMAND sjk_chat_check)
