#! /bin/sh

TRIPLE=arm-kobov4-linux-gnueabihf
export CFLAGS="-O2 -mcpu=cortex-a9 -mfpu=neon"

./configure --host=${TRIPLE} && make

[ -L include ] || ln -s . include
[ -L lib ] || ln -s .libs lib
