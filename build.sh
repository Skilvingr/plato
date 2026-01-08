#! /bin/sh

set -e

method=${1:-"fast"}

#[ -e libs -a $# -eq 0 ] && method=skip

case "$method" in
	fast)
    	[ -e libs ] && rm -f libs/* || mkdir libs

    	cp thirdparty/zlib/libz.so libs
    	cp thirdparty/bzip2/libbz2.so libs

    	cp thirdparty/libpng/.libs/libpng16.so libs
    	cp thirdparty/libjpeg/.libs/libjpeg.so libs
    	cp thirdparty/openjpeg/build/bin/libopenjp2.so libs
    	cp thirdparty/jbig2dec/.libs/libjbig2dec.so libs

    	cp thirdparty/freetype2/objs/.libs/libfreetype.so libs
    	cp thirdparty/harfbuzz/src/.libs/libharfbuzz.so libs

    	cp thirdparty/gumbo/.libs/libgumbo.so libs
    	cp thirdparty/djvulibre/libdjvu/.libs/libdjvulibre.so libs
    	cp thirdparty/mupdf/build/shared-release/libmupdf.so libs

        # TODO: fix path
        cp ~/x-tools/arm-kobov4-linux-gnueabihf/arm-kobov4-linux-gnueabihf/sysroot/lib/libstdc++.so.6.0.33 libs/libstdc++.so.6
        chmod 755 libs/libstdc++.so.6
		;;

	slow)
		shift
		cd thirdparty
		./download.sh "$@"
		./build.sh "$@"
		cd ..

		[ -e libs ] || mkdir libs

		cp thirdparty/zlib/libz.so libs
		cp thirdparty/bzip2/libbz2.so libs

		cp thirdparty/libpng/.libs/libpng16.so libs
		cp thirdparty/libjpeg/.libs/libjpeg.so libs
		cp thirdparty/openjpeg/build/bin/libopenjp2.so libs
		cp thirdparty/jbig2dec/.libs/libjbig2dec.so libs

		cp thirdparty/freetype2/objs/.libs/libfreetype.so libs
		cp thirdparty/harfbuzz/src/.libs/libharfbuzz.so libs

		cp thirdparty/gumbo/.libs/libgumbo.so libs
		cp thirdparty/djvulibre/libdjvu/.libs/libdjvulibre.so libs
		cp thirdparty/mupdf/build/shared-release/libmupdf.so libs

		# TODO: fix path
		cp ~/x-tools/arm-kobov4-linux-gnueabihf/arm-kobov4-linux-gnueabihf/sysroot/lib/libstdc++.so.6.0.33 libs/libstdc++.so.6
		chmod 755 libs/libstdc++.so.6
		;;

	skip)
		;;
	*)
		printf "Unknown build method: %s.\n" "$method" 1>&2
		exit 1
		;;
esac

cd mupdf_wrapper
./build-kobo.sh
cd ..

cargo build --release --target=arm-unknown-linux-gnueabihf -p kaesar
