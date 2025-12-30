#! /bin/sh

TRIPLE=arm-kobov4-linux-gnueabihf
export CFLAGS='-O2 -mcpu=cortex-a9 -mfpu=neon'
export CXXFLAGS="$CFLAGS"
export AS=${TRIPLE}-as

./autogen.sh --host=${TRIPLE} && make
