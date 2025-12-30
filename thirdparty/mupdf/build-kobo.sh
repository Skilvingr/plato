#! /bin/sh

#[ -e thirdparty/README ] && rm -rf thirdparty/*
#[ -e .gitattributes ] && rm -rf .git*

# rm -rf \
#     thirdparty/brotli \
#     thirdparty/curl \
#     thirdparty/freeglut \
#     thirdparty/freetype \
#     thirdparty/harfbuzz \
#     thirdparty/leptonica \
#     thirdparty/libjpeg \
#     thirdparty/tesseract \
#     thirdparty/zlib \

BUILD_KIND=${1:-release}

TRIPLE=arm-kobov4-linux-gnueabihf
XCFLAGS="-O2 -fPIC -mcpu=cortex-a9 -mfpu=neon -DTOFU_CJK_LANG -DTOFU_CJK_EXT -DFZ_ENABLE_ICC=0 -DFZ_ENABLE_JS=0 -DFZ_ENABLE_SPOT_RENDERING=0 -DFZ_ENABLE_ODT_OUTPUT=0 -DFZ_ENABLE_DOCX_OUTPUT=0 -DFZ_ENABLE_OCR_OUTPUT=0 -DNOBUILTINFONT -DFZ_PLOTTERS_CMYK=0 -DHAVE_LIBAES=1 -DHAVE_WEBP=1"
make \
    -j1 \
    AR=$TRIPLE-ar \
    AS=$TRIPLE-as \
    CC=$TRIPLE-gcc \
    CXX=$TRIPLE-g++ \
    LD=$TRIPLE-ld \
    XCFLAGS="$XCFLAGS" \
    USE_ARGUMENT_FILE=no \
    barcode=no \
    brotli=no \
    mujs=no \
    tesseract=no \
    HAVE_PTHREAD=yes \
    HAVE_LIBCRYPTO=no \
    HAVE_X11=no \
    HAVE_GLFW=no \
    USE_SYSTEM_LIBS=yes \
    SYS_PTHREAD_CFLAGS=\
    SYS_PTHREAD_LIBS=-lpthread \
    SYS_FREETYPE_CFLAGS=-I../freetype2/include \
    SYS_FREETYPE_LIBS=-L../freetype2/objs/.libs -lfreetype \
    SYS_GUMBO_CFLAGS=-I../gumbo/src \
    SYS_GUMBO_LIBS=-L../gumbo/.libs -lgumbo \
    SYS_HARFBUZZ_CFLAGS=-I../harfbuzz/src \
    SYS_HARFBUZZ_LIBS=-L../harfbuzz/src/.libs -lharfbuzz \
    SYS_OPENJPEG_CFLAGS=-I../openjpeg/src/lib/openjp2 \
    SYS_OPENJPEG_LIBS=-L../openjpeg/build/bin -lopenjpeg \
    SYS_JBIG2DEC_CFLAGS=-I../jbig2dec \
    SYS_JBIG2DEC_LIBS=-L../jbig2dec/.libs -ljbig2dec \
    SYS_LIBJPEG_CFLAGS=-I../libjpeg \
    SYS_LIBJPEG_LIBS=-L../libjpeg/.libs -ljpeg \
    SYS_ZLIB_CFLAGS=-I../zlib \
    SYS_ZLIB_LIBS=-L../zlib -lz \
    shared=yes verbose=no OS=linux build="$BUILD_KIND" libs

echo "Linking..."
arm-kobov4-linux-gnueabihf-gcc -Wl,--gc-sections -o build/"shared-$BUILD_KIND"/libmupdf.so $(find build/shared-"$BUILD_KIND" -name '*.o' | grep -Ev '(SourceHanSerif-Regular|DroidSansFallbackFull|NotoSerifTangut|color-lcms)') \
    -lm \
    -L../freetype2/objs/.libs -lfreetype \
    -L../harfbuzz/src/.libs -lharfbuzz \
    -L../gumbo/.libs -lgumbo \
    -L../jbig2dec/.libs -ljbig2dec \
    -L../libjpeg/.libs -ljpeg \
    -L../openjpeg/build/bin -lopenjp2 \
    -L../zlib -lz \
    -shared -Wl,-soname -Wl,libmupdf.so -Wl,--no-undefined
