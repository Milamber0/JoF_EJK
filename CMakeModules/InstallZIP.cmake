#=============================================================================
# Copyright 2007-2009 Kitware, Inc.
# Copyright 2015 OpenJK contributors
#
# Redistribution and use in source and binary forms, with or without
# modification, are permitted provided that the following conditions
# are met:
#
# * Redistributions of source code must retain the above copyright
#   notice, this list of conditions and the following disclaimer.
#
# * Redistributions in binary form must reproduce the above copyright
#   notice, this list of conditions and the following disclaimer in the
#   documentation and/or other materials provided with the distribution.
#
# * Neither the names of Kitware, Inc., the Insight Software Consortium,
#   nor the names of their contributors may be used to endorse or promote
#   products derived from this software without specific prior written
#   permission.
#
# THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
# "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
# LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR
# A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT
# HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
# SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT
# LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
# DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
# THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
# (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
# OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
#=============================================================================

include(CMakeParseArguments)

unset(ZIP_EXECUTABLE CACHE)
if(WIN32)
  if(NOT ZIP_EXECUTABLE)
    find_program(ZIP_EXECUTABLE wzzip PATHS "$ENV{ProgramFiles}/WinZip")
    if(ZIP_EXECUTABLE)
      set(ZIP_COMMAND "${ZIP_EXECUTABLE}" -P "<ARCHIVE>" <FILES>)
    endif()
  endif()
  
  if(NOT ZIP_EXECUTABLE)
    find_program(ZIP_EXECUTABLE wzzip PATHS "$ENV{ProgramW6432}/WinZip")
    if(ZIP_EXECUTABLE)
      set(ZIP_COMMAND "${ZIP_EXECUTABLE}" -P "<ARCHIVE>" <FILES>)
    endif()
  endif()
  
  if(NOT ZIP_EXECUTABLE)
    find_program(ZIP_EXECUTABLE 7z PATHS "$ENV{ProgramFiles}/7-Zip")
    if(ZIP_EXECUTABLE)
      set(ZIP_COMMAND "${ZIP_EXECUTABLE}" a -tzip "<ARCHIVE>" <FILES>)
    endif()
  endif()
  
  if(NOT ZIP_EXECUTABLE)
    find_program(ZIP_EXECUTABLE 7z PATHS "$ENV{ProgramW6432}/7-Zip")
    if(ZIP_EXECUTABLE)
      set(ZIP_COMMAND "${ZIP_EXECUTABLE}" a -tzip "<ARCHIVE>" <FILES>)
    endif()
  endif()
  
  if(NOT ZIP_EXECUTABLE)
    find_program(ZIP_EXECUTABLE winrar PATHS "$ENV{ProgramFiles}/WinRAR")
    if(ZIP_EXECUTABLE)
      set(ZIP_COMMAND "${ZIP_EXECUTABLE}" a "<ARCHIVE>" <FILES>)
    endif()
  endif()
  
  if(NOT ZIP_EXECUTABLE)
    find_program(ZIP_EXECUTABLE winrar PATHS "$ENV{ProgramW6432}/WinRAR")
    if(ZIP_EXECUTABLE)
      set(ZIP_COMMAND "${ZIP_EXECUTABLE}" a "<ARCHIVE>" <FILES>)
    endif()
  endif()
endif()

if(NOT ZIP_EXECUTABLE)
  if(WIN32)
    find_package(Cygwin)
    find_program(ZIP_EXECUTABLE 7za PATHS "${CYGWIN_INSTALL_PATH}/bin")
  else()
    find_program(ZIP_EXECUTABLE 7za)
  endif()
  
  if(ZIP_EXECUTABLE)
    set(ZIP_COMMAND "${ZIP_EXECUTABLE}" a -tzip "<ARCHIVE>" <FILES>)
  endif()
endif()

if(NOT ZIP_EXECUTABLE)
  set(ZIP_COMMAND "${CMAKE_COMMAND}" -E tar "cf" "<ARCHIVE>" --format=zip <FILES>)
endif()

function(add_zip_command output)
  set(OneValueArgs WORKING_DIRECTORY)
  set(MultiValueArgs FILES DEPENDS)
  cmake_parse_arguments(ARGS "" "${OneValueArgs}" "${MultiValueArgs}" ${ARGN})

  # Every zip tool stores entry names relative to the directory it runs in, so
  # an archive of content that lives outside the build tree has to be packed
  # from inside that content's root.  Packing from the build directory with
  # absolute FILES instead produces entries like
  # "../../assets/japro/ui/jamp/setup.menu", which the engine cannot resolve --
  # the pk3 mounts fine but every lookup inside it misses.
  if(ARGS_WORKING_DIRECTORY)
    set(WorkingDirectory "${ARGS_WORKING_DIRECTORY}")
  else()
    set(WorkingDirectory "${CMAKE_CURRENT_BINARY_DIR}")
  endif()

  # Keep the archive itself addressed absolutely, so it still lands in the build
  # directory no matter where we pack from.
  if(IS_ABSOLUTE "${output}")
    set(ArchivePath "${output}")
  else()
    set(ArchivePath "${CMAKE_CURRENT_BINARY_DIR}/${output}")
  endif()

  set(ZipCommand ${ZIP_COMMAND})
  string(REPLACE <ARCHIVE> "${ArchivePath}" ZipCommand "${ZipCommand}")
  string(REPLACE <FILES> "${ARGS_FILES}" ZipCommand "${ZipCommand}")

  # zip, 7z and winrar all ADD to an existing archive rather than replacing it,
  # so a stale archive would keep serving entries that no longer exist.
  add_custom_command(OUTPUT ${output}
    COMMAND ${CMAKE_COMMAND} -E remove -f ${ArchivePath}
    COMMAND ${ZipCommand}
    WORKING_DIRECTORY ${WorkingDirectory}
    DEPENDS ${ARGS_DEPENDS})
endfunction(add_zip_command)
