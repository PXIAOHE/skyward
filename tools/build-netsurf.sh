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

# The build runs inside the extracted tree, so every path it is given has to
# hold from there.
LIBS=$(cd "$LIBS" && pwd)
mkdir -p "$OUT"
OUT=$(cd "$OUT" && pwd)

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

# Drawing lands in our own buffer instead of being logged, and the finished
# frame is handed over once the window has been redrawn.
cp "$HERE/netsurf/skyward-plot.c" "$SRC/frontends/monkey/plot.c"
# Text is measured with the same face it is drawn with.
cp "$HERE/netsurf/skyward-layout.c" "$SRC/frontends/monkey/layout.c"
python3 - "$SRC/frontends/monkey/browser.c" <<'PATCH'
import sys
path = sys.argv[1]
text = open(path).read()
if 'skyward_frame_publish' not in text:
    text = text.replace(
        '\tbrowser_window_redraw(gw->bw, gw->scrollx, gw->scrolly, &clip, &ctx);',
        '\t{ FILE *d = fopen("/tmp/skyplot.log","a"); if (d) { fprintf(d, "redraw has_content=%d x=%d y=%d w=%d h=%d\\n", (int)browser_window_has_content(gw->bw), gw->scrollx, gw->scrolly, (int)gw->width, (int)gw->height); fclose(d); } }\n'
        '\tbrowser_window_redraw(gw->bw, gw->scrollx, gw->scrolly, &clip, &ctx);\n'
        '\tskyward_frame_publish();')
    text = text.replace(
        '\tret->width = 800;\n\tret->height = 600;',
        '\tret->width = getenv("SKYWARD_WIDTH") ? atoi(getenv("SKYWARD_WIDTH")) : 800;\n'
        '\tret->height = getenv("SKYWARD_HEIGHT") ? atoi(getenv("SKYWARD_HEIGHT")) : 600;')
    text = text.replace('#include "monkey/browser.h"',
        '#include "monkey/browser.h"\n\nvoid skyward_frame_publish(void);')
    open(path, 'w').write(text)
PATCH

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
NETSURF_FRONTEND := framebuffer
NETSURF_FB_FRONTEND := ram
NETSURF_FB_FONTLIB := internal
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
CFLAGS += $INCS -I$LIBS/include -I$HERE/netsurf -Wno-error $*
LDFLAGS += -L$LIBS -lnsdeps -lm
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

# libnsfb draws and hands back a buffer: the surface the page is put on. Only
# what a recovery uses is built — the memory surface and the 32-bit plotting —
# with the backends that talk to a display server left out.
echo "fetching libnsfb"
curl -sSL -o "$WORK/nsfb.tar.gz" \
    "https://download.netsurf-browser.org/libs/releases/libnsfb-0.2.2-src.tar.gz"
tar xzf "$WORK/nsfb.tar.gz" -C "$WORK"
NSFB="$WORK/libnsfb-0.2.2"
mkdir -p "$WORK/nsfbobj"

nsfb_build() {
    src=$1
    o="$WORK/nsfbobj/$(echo "$src" | sed "s|$NSFB/||; s|/|_|g; s|\.c$|.o|")"
    $CC -O2 -fPIC -fcommon -DNDEBUG -std=c99 -D_POSIX_C_SOURCE=200809L \
        -D_DEFAULT_SOURCE -I"$NSFB/include" -I"$NSFB/src" -I"$SRC" \
        -I"$SRC/include" -I"$SRC/frontends" -I"$LIBS/include" \
        -I"$HERE/netsurf/stub" -c "$src" -o "$o"
}

for src in "$NSFB"/src/*.c \
           "$NSFB"/src/plot/api.c "$NSFB"/src/plot/util.c \
           "$NSFB"/src/plot/generic.c "$NSFB"/src/plot/32bpp-xrgb8888.c \
           "$NSFB"/src/plot/32bpp-xbgr8888.c "$NSFB"/src/plot/16bpp.c \
           "$NSFB"/src/plot/8bpp.c \
           "$NSFB"/src/surface/surface.c "$NSFB"/src/surface/ram.c; do
    nsfb_build "$src"
done
$AR rcs "$LIBS/libnsfb.a" "$WORK"/nsfbobj/*.o
echo "  -> $LIBS/libnsfb.a ($(wc -c < "$LIBS/libnsfb.a") bytes)"

# The frontend finds libnsfb through pkg-config.
mkdir -p "$LIBS/pkgconfig"
cat > "$LIBS/pkgconfig/libnsfb.pc" <<PC
Name: libnsfb
Description: libnsfb
Version: 0.2.2
Libs: -L$LIBS -lnsfb
Cflags: -I$NSFB/include
PC

# One archive for the whole stack: the libraries call each other in both
# directions, and a single archive takes the order out of the link line.
if [ ! -f "$LIBS/libnsdeps.a" ]; then
    $CC -nostdlib -no-pie -Wl,-r -o "$WORK/all.o" -Wl,--whole-archive \
        "$LIBS/libwapcaplet.a" "$LIBS/libparserutils.a" \
        "$LIBS/libhubbub.a" "$LIBS/libcss.a" "$LIBS/libdom.a" \
        "$LIBS/libnsutils.a" "$LIBS/libnsgif.a" "$LIBS/libnsbmp.a" \
        "$LIBS/libnsfb.a" \
        -Wl,--no-whole-archive
    $AR rcs "$LIBS/libnsdeps.a" "$WORK/all.o"
fi

echo "building netsurf $VERSION"
( cd "$SRC" && PKG_CONFIG_PATH="$WORK/pc:$LIBS/pkgconfig" make TARGET=framebuffer -j"$(nproc)" )

mkdir -p "$OUT"
cp "$SRC/nsfb" "$OUT/"
echo "-> $OUT/nsfb ($(wc -c < "$OUT/nsfb") bytes)"
