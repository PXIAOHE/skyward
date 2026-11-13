#!/bin/sh
# Builds NetSurf for the phone, against the libraries built by
# build-netsurf-libs.sh.
#
# NetSurf reaches HTTP through curl, which a recovery has no use for: this
# swaps in a fetcher that asks the program around it for the bytes, so the
# phone's own HTTPS stack and name servers are the only ones involved.
#
# Usage: build-netsurf.sh <cc> <ar> <libs dir> <out dir> [extra cflags...]
#   HOSTCC   compiler for the generators (default: cc)

set -eu

CC=${1:?compiler}
AR=${2:?archiver}
LIBS=${3:?library directory}
OUT=${4:?output directory}
shift 4
HOSTCC=${HOSTCC:-cc}

VERSION=3.11
BASE=https://download.netsurf-browser.org/netsurf/releases/source

HERE=$(cd "$(dirname "$0")" && pwd)

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

echo "fetching netsurf $VERSION"
curl -sSL -o "$WORK/netsurf.tar.gz" "$BASE/netsurf-$VERSION-src.tar.gz"
tar xzf "$WORK/netsurf.tar.gz" -C "$WORK"
SRC="$WORK/netsurf-$VERSION"

# The fetcher that asks the program around NetSurf for the bytes, in place of
# the one that brings its own TLS and resolver.
cp "$HERE/netsurf/skyward-fetch.c" "$SRC/content/fetchers/skyward.c"
cp "$HERE/netsurf/skyward-fetch.h" "$SRC/content/fetchers/skyward.h"

sed -i \
    -e 's|#include "content/fetchers/curl.h"|#include "content/fetchers/skyward.h"|' \
    -e 's|ret = fetch_curl_register();|ret = fetch_skyward_register();|' \
    "$SRC/content/fetch.c"

sed -i \
    -e 's|^S_FETCHERS_YES := data.c resource.c$|S_FETCHERS_YES := data.c resource.c skyward.c|' \
    -e 's|^S_FETCHERS_$(NETSURF_USE_CURL) += curl.c$||' \
    "$SRC/content/fetchers/Makefile"

INCS="-I$LIBS/include"

cat > "$SRC/Makefile.config" <<EOF
NETSURF_FRONTEND := monkey
NETSURF_USE_CURL := NO
NETSURF_USE_OPENSSL := NO
NETSURF_USE_JPEG := NO
NETSURF_USE_JPEGXL := NO
NETSURF_USE_PNG := NO
NETSURF_USE_GIF := YES
NETSURF_USE_BMP := YES
NETSURF_USE_WEBP := NO
NETSURF_USE_VIDEO := NO
NETSURF_USE_NSPSL := NO
NETSURF_USE_NSLOG := NO
NETSURF_USE_UTF8PROC := NO
NETSURF_USE_DUKTAPE := NO
NETSURF_USE_HARU_PDF := NO
NETSURF_USE_LIBICONV_PLUG := YES
NETSURF_STRIP_BINARY := NO
CC := $CC
AR := $AR
CFLAGS += $INCS -I$LIBS/include -Wno-error $*
LDFLAGS += -L$LIBS -lnsdeps
EOF

# pkg-config is how the makefiles look for the libraries; tell it where the
# ones just built are.
mkdir -p "$WORK/pc"
for lib in libwapcaplet libparserutils libhubbub libcss libdom libnsutils \
           libnsgif libnsbmp; do
    name=${lib#lib}
    cat > "$WORK/pc/$lib.pc" <<EOF
Name: $lib
Description: $lib
Version: 1.0
Libs: -L$LIBS -l$name
Cflags: -I$LIBS/include
EOF
done

# One archive for the whole stack: the libraries call each other in both
# directions, and a single archive takes the order out of the link line.
if [ ! -f "$LIBS/libnsdeps.a" ]; then
    $CC -nostdlib -no-pie -Wl,-r -o "$WORK/all.o" -Wl,--whole-archive \
        "$LIBS/libwapcaplet.a" "$LIBS/libparserutils.a" \
        "$LIBS/libhubbub.a" "$LIBS/libcss.a" "$LIBS/libdom.a" \
        "$LIBS/libnsutils.a" "$LIBS/libnsgif.a" "$LIBS/libnsbmp.a" \
        -Wl,--no-whole-archive
    $AR rcs "$LIBS/libnsdeps.a" "$WORK/all.o"
fi

echo "building netsurf $VERSION"
( cd "$SRC" && PKG_CONFIG_PATH="$WORK/pc" make TARGET=monkey -j"$(nproc)" )

mkdir -p "$OUT"
cp "$SRC/nsmonkey" "$OUT/"
echo "-> $OUT/nsmonkey ($(wc -c < "$OUT/nsmonkey") bytes)"
