#!/usr/bin/env bash
# Build rd-freedom_x86_64.dll (the Freedom renderer, built on SJK, for JoF EJK) on Linux with MinGW-w64.
#
#   scripts/build-rd-sjk.sh [SJK source folder] [build folder]
#
# Needs: cargo/rustc (std for x86_64-pc-windows-gnu, or the Rust library source for
# -Zbuild-std), mingw-w64, cmake, ninja. The SJK folder defaults to lib/sjk (SJK with
# its `host` module). The DLL lands in the build folder.
set -euo pipefail

here="$(cd "$(dirname "$0")/.." && pwd)"
sjk="${1:-$here/lib/sjk}"
build="${2:-$here/build-rd-sjk}"
target=x86_64-pc-windows-gnu

cargo_args=(rustc -p sjk-viewer --lib --release --target "$target" --crate-type=staticlib)
if ! rustup target list --installed 2>/dev/null | grep -qx "$target"; then
	# No prebuilt std for the target: build it from the toolchain's library source.
	export RUSTC_BOOTSTRAP=1
	cargo_args+=(-Zbuild-std=std,panic_unwind)
fi
export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc
(cd "$sjk" && cargo "${cargo_args[@]}")
library="$sjk/target/$target/release/libsjk_host.a"

# Import libraries of the windows/winapi crates the Rust library links.
registry="$(ls -d "${CARGO_HOME:-$HOME/.cargo}"/registry/src/*/ | head -1)"
windows_lib="$(ls -d "$registry"windows_x86_64_gnu-*/lib | sort -V | tail -1)"
winapi_lib="$(ls -d "$registry"winapi-x86_64-pc-windows-gnu-*/lib | sort -V | tail -1)"
windows_name="$(basename "$(ls "$windows_lib"/libwindows.*.a | head -1)" .a)"
windows_name="${windows_name#lib}"

cmake -S "$here" -B "$build" -G Ninja \
	-DCMAKE_TOOLCHAIN_FILE="$here/cmake/Toolchains/mingw-w64-x86_64.cmake" \
	-DCMAKE_BUILD_TYPE=Release \
	-DBuildMPEngine=OFF -DBuildMPRdVanilla=OFF -DBuildMPDed=OFF -DBuildMPGame=OFF \
	-DBuildMPCGame=OFF -DBuildMPUI=OFF -DBuildMPRend2=OFF -DBuildDiscordRichPresence=OFF \
	-DBuildMPRdSjk=ON \
	-DSJK_HOST_LIBRARY="$library" \
	"-DSJK_HOST_LINK_DIRECTORIES=$windows_lib;$winapi_lib" \
	"-DSJK_HOST_EXTRA_LIBRARIES=$windows_name;winapi_advapi32;winapi_cfgmgr32;winapi_kernel32;winapi_user32"
ninja -C "$build" rd-freedom_x86_64
x86_64-w64-mingw32-strip --strip-debug "$build/rd-freedom_x86_64.dll"
ls -la "$build/rd-freedom_x86_64.dll"
