#!/usr/bin/env bash
# Inside the Android container: build Tasks for Android, unsigned.
#   uuob/docker/android.sh uuob/docker/apk.sh [aarch64|x86_64]   (signs it too)
# 1. the core (core/, with UUOB's SDK) as a library for that kind of phone,
# 2. its Kotlin bindings (UniFFI), written from that library,
# 3. the app (Gradle): android/app/build/outputs/apk/release/app-release-unsigned.apk.
set -euo pipefail
arch=${1:-aarch64}
cd "$(dirname "$0")/.."
case $arch in
  aarch64) triple=aarch64-linux-android abi=arm64-v8a ;;
  x86_64) triple=x86_64-linux-android abi=x86_64 ;;
  *) echo "aarch64 or x86_64"; exit 1 ;;
esac
tools=$NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin
var=$(echo "$triple" | tr 'a-z-' 'A-Z_')
export "CARGO_TARGET_${var}_LINKER=$tools/${triple}26-clang" "CC_${triple//-/_}=$tools/${triple}26-clang" "AR_${triple//-/_}=$tools/llvm-ar"

cargo build --release --lib -p tasks-core --target "$triple"
lib=target/$triple/release/libtasks_core.so
rm -rf android/app/src/main/jniLibs && mkdir -p "android/app/src/main/jniLibs/$abi"
"$tools/llvm-strip" -o "android/app/src/main/jniLibs/$abi/libtasks_core.so" "$lib"

rm -rf android/app/src/main/java/app/uuob/client android/app/src/main/java/app/uuob/tasks/core
cargo run -q -p tasks-core --features bindgen --bin uniffi-bindgen -- \
  generate --library "$lib" --language kotlin --out-dir android/app/src/main/java --no-format

cd android && ./gradlew --no-daemon -q assembleRelease
