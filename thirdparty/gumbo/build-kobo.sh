#! /bin/sh

export TRIPLE=arm-kobov4-linux-gnueabihf

[ -x configure ] || ./autogen.sh
./configure --host="$TRIPLE" && make
