cmake_minimum_required(VERSION 3.16)
project(dialogue_sound_check C)

set(repo "${CMAKE_CURRENT_SOURCE_DIR}/../..")
configure_file("${repo}/codemp/versionfile/jofversion.h.in" jofversion.h @ONLY)
function(extract path first last output)
  set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS "${repo}/${path}")
  file(READ "${repo}/${path}" source)
  string(FIND "${source}" "${first}" start)
  if(last STREQUAL "EOF")
    string(LENGTH "${source}" end)
  else()
    string(FIND "${source}" "${last}" end)
  endif()
  if(start LESS 0 OR end LESS start)
    message(FATAL_ERROR "Cannot extract ${first}")
  endif()
  math(EXPR length "${end}-${start}")
  string(SUBSTRING "${source}" ${start} ${length} section)
  file(WRITE "${CMAKE_CURRENT_BINARY_DIR}/${output}" "${section}")
endfunction()

extract(codemp/game/g_dialogue.c "#define DLG_MAX_FILES" "EOF" server.h)
extract(codemp/cgame/cg_dialogue.c "#define CG_DLG_MAX_CHOICES" "qboolean CG_DialogueKeyEvent(" client.h)
extract(codemp/client/snd_dma.cpp "void S_SpatializeOrigin (" "// Start a sound effect" spatialize.h)
add_executable(dialogue_sound_check check.c
  "${repo}/codemp/qcommon/q_shared.c"
  "${repo}/shared/qcommon/q_color.c"
  "${repo}/shared/qcommon/q_math.c"
  "${repo}/shared/qcommon/q_string.c")
target_include_directories(dialogue_sound_check PRIVATE
  "${CMAKE_CURRENT_BINARY_DIR}" "${repo}/codemp" "${repo}/shared")
if(MSVC)
  target_compile_definitions(dialogue_sound_check PRIVATE _CRT_SECURE_NO_WARNINGS)
else()
  target_link_libraries(dialogue_sound_check PRIVATE m)
endif()
enable_testing()
add_test(NAME dialogue_sound COMMAND dialogue_sound_check)
