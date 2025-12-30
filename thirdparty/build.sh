#! /usr/bin/env bash

set -e

declare -a packages=(zlib bzip2 libpng libjpeg openjpeg jbig2dec freetype2 harfbuzz gumbo djvulibre mupdf)

apply_patches () {
    for p in $(cat ../../patches/$1/order); do
        echo "Applying patch ../../patches/${1}/${p}"
        patch -p 1 < ../../patches/$1/$p
    done
}

for name in "${@:-${packages[@]}}" ; do
	cd "$name"
	echo "Building ${name}."
	[ -e kobo.patch ] && patch -p 1 < kobo.patch
	./build-kobo.sh
	cd ..
done
