#! /bin/sh

[ -d build ] && rm -Rf build

TRIPLE=arm-kobov4-linux-gnueabihf
export CFLAGS="-Wall -O2 -mcpu=cortex-a9 -mfpu=neon -std=c99"
export CXXFLAGS="$CFLAGS"

cmake -B build -S . -DCMAKE_BUILD_TYPE=None -DBUILD_CODEC=off -DBUILD_STATIC_LIBS=off -DCMAKE_SYSTEM_NAME=Linux -DCMAKE_C_COMPILER=${TRIPLE}-gcc -DCMAKE_AR=${TRIPLE}-ar .. && cmake --build build

cp build/src/lib/openjp2/opj_config.h src/lib/openjp2
