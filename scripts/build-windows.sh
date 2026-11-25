#!/usr/bin/env bash
# Builds the Windows exe on a Mac (llvm-mingw, gnullvm targets).
# Usage: scripts/build-windows.sh [arm64|x64]   (default: arm64)
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$HOME/toolchains/llvm-mingw/bin:$PATH"
case "${1:-arm64}" in
  arm64) T=aarch64-pc-windows-gnullvm; P=aarch64-w64-mingw32 ;;
  x64)   T=x86_64-pc-windows-gnullvm;  P=x86_64-w64-mingw32 ;;
  *) echo "arm64 or x64"; exit 1 ;;
esac
T_ENV=${T//-/_}
export "CC_${T_ENV}=${P}-clang" "CXX_${T_ENV}=${P}-clang++" "AR_${T_ENV}=llvm-ar"
cargo build --release --target "$T" -p peekaboard
# The GNU targets load WebView2 through WebView2Loader.dll, which has to sit
# next to the exe: each architecture gets its own folder
OUT="dist/windows-${1:-arm64}"
mkdir -p "$OUT"
cp "target/$T/release/peekaboard.exe" "target/$T/release/WebView2Loader.dll" "$OUT/"
ls -la "$OUT"
