cmake_minimum_required(VERSION 3.16)
project(pm_only_chat C)

function(extract path first last)
  set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS "${CMAKE_CURRENT_SOURCE_DIR}/../../${path}")
  file(READ "${CMAKE_CURRENT_SOURCE_DIR}/../../${path}" source)
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
extract(codemp/cgame/cg_servercmds.c "static void CG_RemoveChatEscapeChar(" "#define MAX_STRINGED_SV_STRING")
# Exercise the real admission, console/log output and metadata assignment;
# text wrapping and emoji layout are outside this filter regression.
extract(codemp/cgame/cg_draw.c "void CG_ChatBox_AddString(" "\n\tif (strlen(chatStr)")
file(APPEND "${CMAKE_CURRENT_BINARY_DIR}/actual.h" "
  Q_strncpyz(chat->string, chatStr, sizeof(chat->string));
  chat->time = cg.time + cg_chatBox.integer;
  chat->lines = 1;
  cg.chatItemActive = (cg.chatItemActive + 1) % cg_chatBoxLines.integer;
}
")
extract(codemp/cgame/cg_servercmds.c "static void CG_Chat_f(" "static void CG_RemapShader_f(")
extract(codemp/cgame/cg_consolecmds.c "static void CG_PMOnly_f(" "void CG_Say_f(")
extract(codemp/cgame/cg_draw.c "void CG_ChatBox_ArrayInsert(" "static void CG_DrawForceSenseOverlay(")

add_executable(pm_only_chat check.c)
target_include_directories(pm_only_chat PRIVATE "${CMAKE_CURRENT_BINARY_DIR}")
enable_testing()
add_test(NAME pm_only_chat COMMAND pm_only_chat)
