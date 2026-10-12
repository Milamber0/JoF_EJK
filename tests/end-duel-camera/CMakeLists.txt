cmake_minimum_required(VERSION 3.16)
project(end_duel_camera C)

get_filename_component(repo "${CMAKE_CURRENT_SOURCE_DIR}/../.." ABSOLUTE)
configure_file("${repo}/codemp/versionfile/jofversion.h.in"
  "${CMAKE_CURRENT_BINARY_DIR}/jofversion.h" @ONLY)

# Compile the real plugin command, event dispatch and camera predicates.
function(extract path first last)
  set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS "${repo}/${path}")
  file(READ "${repo}/${path}" source)
  string(FIND "${source}" "${first}" start)
  string(FIND "${source}" "${last}" end)
  if(start LESS 0 OR end LESS start)
    message(FATAL_ERROR "Cannot extract ${first}")
  endif()
  math(EXPR length "${end}-${start}")
  string(SUBSTRING "${source}" ${start} ${length} part)
  file(APPEND "${CMAKE_CURRENT_BINARY_DIR}/actual.h" "${part}\n")
endfunction()

file(WRITE "${CMAKE_CURRENT_BINARY_DIR}/actual.h" "")
extract(codemp/cgame/cg_consolecmds.c "typedef struct bitInfo_S {" "static bitInfo_T strafeTweaks[] = {")
extract(codemp/cgame/cg_consolecmds.c "static qboolean japroPluginDisables[] = {" "//Playermodel visuals")
file(APPEND "${CMAKE_CURRENT_BINARY_DIR}/actual.h"
  "static void DispatchDuelEvent(entityState_t *es) { switch (EV_PRIVATE_DUEL) {\n")
extract(codemp/cgame/cg_event.c "\tcase EV_PRIVATE_DUEL:" "\tcase EV_JUMP:")
file(APPEND "${CMAKE_CURRENT_BINARY_DIR}/actual.h" "} }\n")
extract(codemp/cgame/cg_view.c "#define END_DUEL_CAMERA_DURATION" "static void CG_OffsetThirdPersonView( void )\n")

add_executable(end_duel_camera check.c)
target_compile_definitions(end_duel_camera PRIVATE _CGAME)
target_include_directories(end_duel_camera PRIVATE
  "${CMAKE_CURRENT_BINARY_DIR}" "${repo}/codemp" "${repo}/shared")
enable_testing()
add_test(NAME end_duel_camera COMMAND end_duel_camera)
