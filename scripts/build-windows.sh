#!/usr/bin/env bash
# Baut die Windows-exe auf dem Mac (llvm-mingw, gnullvm-Targets).
# Aufruf: scripts/build-windows.sh [arm64|x64]   (Standard: arm64)
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$HOME/toolchains/llvm-mingw/bin:$PATH"
case "${1:-arm64}" in
  arm64) T=aarch64-pc-windows-gnullvm; P=aarch64-w64-mingw32 ;;
  x64)   T=x86_64-pc-windows-gnullvm;  P=x86_64-w64-mingw32 ;;
  *) echo "arm64 oder x64"; exit 1 ;;
esac
T_ENV=${T//-/_}
export "CC_${T_ENV}=${P}-clang" "CXX_${T_ENV}=${P}-clang++" "AR_${T_ENV}=llvm-ar"
cargo build --release --target "$T" -p zmk-keymap-viewer
mkdir -p dist
cp "target/$T/release/zmk-keymap-viewer.exe" "dist/zmk-keymap-viewer-${1:-arm64}.exe"
ls -la "dist/zmk-keymap-viewer-${1:-arm64}.exe"
