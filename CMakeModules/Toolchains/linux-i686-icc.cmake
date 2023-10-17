set(CMAKE_SYSTEM_NAME Linux)
set(CMAKE_SYSTEM_PROCESSOR i686)

set(BuildMPRend2 OFF)

set(CMAKE_C_COMPILER "/opt/intel/compilers_and_libraries_2019/linux/bin/ia32/icc")
set(CMAKE_CXX_COMPILER "/opt/intel/compilers_and_libraries_2019/linux/bin/ia32/icpc")

#include(CMakeForceCompiler)
#cmake_force_c_compiler(/opt/intel/compilers_and_libraries_2019/linux/bin/ia32/icc Intel)
#cmake_force_cxx_compiler(/opt/intel/compilers_and_libraries_2019/linux/bin/ia32/icpc Intel)
#cmake_force_c_compiler(${COMPILER_PREFIX}icc Intel)
#cmake_force_cxx_compiler(${COMPILER_PREFIX}icpc Intel)

#set(CMAKE_C_COMPILER_ARG1 -m32)
#set(CMAKE_CXX_COMPILER_ARG1 -m32)
