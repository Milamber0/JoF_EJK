cmake_minimum_required(VERSION 3.16)
project(rancor_spectator C)

set(repo "${CMAKE_CURRENT_SOURCE_DIR}/../..")
# Compile the actual rancor AI and timers; extract the spectator entry points
# so engine services can be mocked without linking the entire game module.
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
extract(codemp/game/g_active.c "void G_HeldByMonster(" "typedef enum tauntTypes_e")
extract(codemp/game/g_cmds.c "void StopFollowing(" "void Cmd_Team_f(")
extract(codemp/game/g_active.c "void SpectatorThink(" "qboolean ClientInactivityTimer(")
extract(codemp/game/g_active.c "void SpectatorClientEndFrame(" "extern qboolean G_IsMindTricked(")
configure_file("${repo}/codemp/versionfile/jofversion.h.in" jofversion.h @ONLY)

add_executable(rancor_spectator check.c
  "${repo}/codemp/game/NPC_AI_Rancor.c"
  "${repo}/codemp/game/g_timer.c"
  "${repo}/shared/qcommon/q_math.c")
target_include_directories(rancor_spectator PRIVATE
  "${CMAKE_CURRENT_BINARY_DIR}" "${repo}/codemp" "${repo}/shared")
if(MSVC)
  target_compile_definitions(rancor_spectator PRIVATE _CRT_SECURE_NO_WARNINGS)
else()
  target_link_libraries(rancor_spectator PRIVATE m)
endif()
enable_testing()
foreach(case follow escape attachment attacks delayed)
  add_test(NAME rancor_spectator_${case} COMMAND rancor_spectator ${case})
endforeach()
