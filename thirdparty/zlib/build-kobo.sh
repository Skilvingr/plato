#! /bin/sh

export CHOST=arm-kobov4-linux-gnueabihf
export CFLAGS="-O2 -mcpu=cortex-a9 -mfpu=neon"
export CXXFLAGS="$CFLAGS"

./configure && make
