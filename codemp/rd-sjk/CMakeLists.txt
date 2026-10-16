#============================================================================
# rd-sjk: SJK's renderer for JoF EJK.
#
# The C++ half is the GL-free model, skin, shader-name and Ghoul2 code of
# rd-dedicated plus the engine-facing entry points (tr_sjk.cpp); the drawing is
# SJK's own renderer (github.com/Sol-Vulpes/SJK, GPL-2.0-only), built as a Rust
# static library from `lib/sjk` and linked in: `SJK_HOST_LIBRARY` names it
# (libsjk_host.a, from `scripts/build-rd-sjk.sh`).
#============================================================================

if(NOT InOpenJK)
	message(FATAL_ERROR "Use the top-level cmake script!")
endif(NOT InOpenJK)

if(NOT SJK_HOST_LIBRARY)
	message(FATAL_ERROR "BuildMPRdSjk needs SJK_HOST_LIBRARY (the path of libsjk_host.a)")
endif()

set(MPRdSjkIncludeDirectories
	${SharedDir}
	${MPDir}
	"${MPDir}/rd-sjk"
	"${GSLIncludeDirectory}"
	${OpenJKLibDir}
	"${CMAKE_CURRENT_BINARY_DIR}/../versionfile"
	)

set(MPRdSjkFiles
	"${MPDir}/rd-sjk/sjk_host.h"
	"${MPDir}/rd-sjk/G2_API.cpp"
	"${MPDir}/rd-sjk/G2_bolts.cpp"
	"${MPDir}/rd-sjk/G2_bones.cpp"
	"${MPDir}/rd-sjk/G2_misc.cpp"
	"${MPDir}/rd-sjk/G2_surfaces.cpp"
	"${MPDir}/rd-sjk/tr_backend.cpp"
	"${MPDir}/rd-sjk/tr_ghoul2.cpp"
	"${MPDir}/rd-sjk/tr_init.cpp"
	"${MPDir}/rd-sjk/tr_local.h"
	"${MPDir}/rd-sjk/tr_main.cpp"
	"${MPDir}/rd-sjk/tr_mesh.cpp"
	"${MPDir}/rd-sjk/tr_model.cpp"
	"${MPDir}/rd-sjk/tr_shader.cpp"
	"${MPDir}/rd-sjk/tr_skin.cpp"
	"${MPDir}/rd-sjk/tr_sjk.cpp"
	"${MPDir}/rd-sjk/sjk_crash.cpp"
	"${MPDir}/rd-sjk/tr_subs.cpp"
	"${MPDir}/ghoul2/g2_local.h"
	"${MPDir}/ghoul2/ghoul2_shared.h"
	"${MPDir}/ghoul2/G2_gore.cpp"
	"${MPDir}/ghoul2/G2_gore.h"
	"${MPDir}/rd-common/mdx_format.h"
	"${MPDir}/rd-common/tr_common.h"
	"${MPDir}/rd-common/tr_font.cpp"
	"${MPDir}/rd-common/tr_font.h"
	"${MPDir}/rd-common/tr_noise.cpp"
	"${MPDir}/rd-common/tr_public.h"
	"${MPDir}/rd-common/tr_types.h"
	"${MPDir}/qcommon/matcomp.cpp"
	"${MPDir}/qcommon/q_shared.cpp"
	${SharedCommonFiles}
	${SharedCommonSafeFiles}
	)
source_group("renderer" FILES ${MPRdSjkFiles})

add_library(${MPRdSjk} SHARED ${MPRdSjkFiles})

if(NOT MSVC)
	# remove "lib" prefix
	set_target_properties(${MPRdSjk} PROPERTIES PREFIX "")
endif()

install(TARGETS ${MPRdSjk}
	RUNTIME
	DESTINATION ${JKAInstallDir}
	COMPONENT ${JKAMPClientComponent})

set_target_properties(${MPRdSjk} PROPERTIES COMPILE_DEFINITIONS "${SharedDefines};RD_SJK")
set_property(TARGET ${MPRdSjk} APPEND PROPERTY COMPILE_OPTIONS ${OPENJK_VISIBILITY_FLAGS})
set_target_properties(${MPRdSjk} PROPERTIES INCLUDE_DIRECTORIES "${MPRdSjkIncludeDirectories}")
set_target_properties(${MPRdSjk} PROPERTIES PROJECT_LABEL "MP SJK Renderer")

# The Rust library and the Windows libraries it imports
# (`--print native-static-libs`). The import libraries of the `windows` and
# `winapi` crates come from their crate folders (SJK_HOST_LINK_DIRECTORIES).
target_link_directories(${MPRdSjk} PRIVATE ${SJK_HOST_LINK_DIRECTORIES})
target_link_libraries(${MPRdSjk}
	${SJK_HOST_LIBRARY}
	kernel32 user32 shell32 gdi32 advapi32 bcrypt opengl32 ntdll userenv ws2_32 dbghelp
	ole32 oleaut32 propsys uuid d3dcompiler dxgi
	${SJK_HOST_EXTRA_LIBRARIES})
if(MINGW)
	# gsl-lite includes <cstdint> inside its namespace; include it first.
	target_compile_options(${MPRdSjk} PRIVATE $<$<COMPILE_LANGUAGE:CXX>:-include cstdint>)
	# One self-contained DLL: no libstdc++/libgcc/winpthread DLLs beside it.
	target_link_options(${MPRdSjk} PRIVATE -static -static-libgcc -static-libstdc++)
endif()
