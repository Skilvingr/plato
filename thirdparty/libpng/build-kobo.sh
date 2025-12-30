#! /bin/sh

TRIPLE=arm-kobov4-linux-gnueabihf
ZLIB_DIR=../zlib
export CFLAGS="-O2 -mcpu=cortex-a9 -mfpu=neon"
export CXXFLAGS="$CFLAGS"
export CPPFLAGS="-I${ZLIB_DIR}"
export LDFLAGS="-L${ZLIB_DIR}"

./configure --host=${TRIPLE} && make
