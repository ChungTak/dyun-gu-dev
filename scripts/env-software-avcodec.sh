#!/usr/bin/env bash
# Environment for building dyun with avcodec-profile-software on Ubuntu
# when libclang is installed without a full clang package (bindgen needs
# both libclang.so and C builtin headers).
#
# Usage:
#   source scripts/env-software-avcodec.sh
#   cargo test -p dg-media --features avcodec-profile-software
#
# Diagnostic output is printed so CI/runner logs can record the actual
# libclang, libyuv and FFmpeg environment. This script does not modify files
# in the repository; it only sets environment variables for the current shell.

set -euo pipefail

export LIBYUV_TARGET="${LIBYUV_TARGET:-ubuntu-24.04_x86_64}"
# rust-toolchain.toml pins 1.94.1; do not override it with stable here.
# User can still set RUSTUP_TOOLCHAIN before sourcing if needed.

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
if command -v rustc >/dev/null 2>&1; then
  echo "rustc=$(rustc --version)"
else
  echo "rustc=<not found>"
fi
if command -v ffmpeg >/dev/null 2>&1; then
  echo "ffmpeg=$(ffmpeg -version 2>/dev/null | head -n 1)"
else
  echo "ffmpeg=<not found>"
fi
for _lib in libavcodec libavformat libavutil libswscale; do
  if pkg-config --exists "${_lib}" 2>/dev/null; then
    echo "${_lib}=$(pkg-config --modversion "${_lib}")"
  else
    echo "${_lib}=<not found via pkg-config>"
  fi
done
