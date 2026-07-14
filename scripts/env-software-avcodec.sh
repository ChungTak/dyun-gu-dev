#!/usr/bin/env bash
# Environment for building dyun with avcodec-profile-software on Ubuntu
# when libclang is installed without a full clang package (bindgen needs
# both libclang.so and C builtin headers).
#
# Usage:
#   source scripts/env-software-avcodec.sh
#   cargo test -p dg-media --features avcodec-profile-software
#
# Note: FFmpeg 8.x currently hits UP2-FFMPEG-01 in upstream avcodec-codec-ffmpeg
# (*const vs *mut AVCodec). See dev-docs/002_fix_avcodec_profile_plan2/UP2-FFMPEG-01.md

set -euo pipefail

export LIBYUV_TARGET="${LIBYUV_TARGET:-ubuntu-24.04_x86_64}"
export RUSTUP_TOOLCHAIN="${RUSTUP_TOOLCHAIN:-stable}"

# Prefer llvm-21 layout used by Ubuntu 25.x packages.
_LLVM_LIB=""
for candidate in /usr/lib/llvm-21/lib /usr/lib/llvm-20/lib /usr/lib/llvm-19/lib; do
  if [[ -e "${candidate}/libclang-21.so.1" || -e "${candidate}/libclang.so" || -e "${candidate}/libclang.so.1" ]]; then
    _LLVM_LIB="${candidate}"
    break
  fi
done

if [[ -n "${_LLVM_LIB}" ]]; then
  _SHIM="${TMPDIR:-/tmp}/dyun-libclang-shim"
  mkdir -p "${_SHIM}"
  if [[ -e "${_LLVM_LIB}/libclang.so" ]]; then
    export LIBCLANG_PATH="${_LLVM_LIB}"
  elif [[ -e "${_LLVM_LIB}/libclang-21.so.1" ]]; then
    ln -sfn "${_LLVM_LIB}/libclang-21.so.1" "${_SHIM}/libclang.so"
    export LIBCLANG_PATH="${_SHIM}"
  elif [[ -e "${_LLVM_LIB}/libclang.so.1" ]]; then
    ln -sfn "${_LLVM_LIB}/libclang.so.1" "${_SHIM}/libclang.so"
    export LIBCLANG_PATH="${_SHIM}"
  fi
fi

# GCC builtin headers for bindgen when clang resource dir is missing.
_GCC_INC=""
for candidate in /usr/lib/gcc/x86_64-linux-gnu/15/include \
                 /usr/lib/gcc/x86_64-linux-gnu/14/include \
                 /usr/lib/gcc/x86_64-linux-gnu/13/include; do
  if [[ -d "${candidate}" ]]; then
    _GCC_INC="${candidate}"
    break
  fi
done

if [[ -n "${_GCC_INC}" ]]; then
  export BINDGEN_EXTRA_CLANG_ARGS="${BINDGEN_EXTRA_CLANG_ARGS:-} -isystem ${_GCC_INC} -isystem /usr/include"
fi

echo "LIBYUV_TARGET=${LIBYUV_TARGET}"
echo "LIBCLANG_PATH=${LIBCLANG_PATH:-<unset>}"
echo "BINDGEN_EXTRA_CLANG_ARGS=${BINDGEN_EXTRA_CLANG_ARGS:-<unset>}"
if pkg-config --exists libavcodec 2>/dev/null; then
  echo "libavcodec=$(pkg-config --modversion libavcodec)"
else
  echo "libavcodec=<not found via pkg-config>"
fi
