/*
 * Drawing for NetSurf: everything it asks to have drawn lands in one RGBA
 * buffer, which the program around NetSurf reads and puts on the screen.
 *
 * Text is drawn from a TrueType face with stb_truetype, because a recovery
 * has fonts but no font server. The face is named by SKYWARD_FONT; without
 * one, glyphs are drawn as outlines of their cells so that layout still
 * reads.
 */

#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <math.h>
#include <fcntl.h>
#include <sys/mman.h>
#include <unistd.h>

#include "utils/errors.h"
#include "utils/log.h"
#include "netsurf/plotters.h"
#include "netsurf/plot_style.h"
#include "netsurf/bitmap.h"
#include "desktop/gui_internal.h"

#define STB_TRUETYPE_IMPLEMENTATION
#include "stb_truetype.h"

bool skyward_surface_open(int width, int height);

struct skyward_surface {
	int width;
	int height;
	int stride;          /* bytes per row */
	uint8_t *pixels;     /* RGBA, one byte each */
	int clip_x0, clip_y0, clip_x1, clip_y1;
};

static struct skyward_surface surface;
static bool surface_ready;

/* The buffer is handed to the program around NetSurf through one file:
 * a small header, then the pixels, then a counter that changes whenever
 * the pixels do. */
#define FRAME_MAGIC 0x534b5946u /* "SKYF" */
struct frame_header {
	uint32_t magic;
	uint32_t width;
	uint32_t height;
	uint32_t stride;
	uint64_t seq;
};

static struct frame_header *frame;
static uint8_t *frame_pixels;
static size_t frame_size;
static const char *frame_path;
static uint64_t frame_seq;

static void frame_publish(void)
{
	if (!surface_ready || surface.pixels == NULL) {
		return;
	}
	if (frame == NULL) {
		frame_path = getenv("SKYWARD_FRAME");
		if (frame_path == NULL) {
			frame_path = "/tmp/skyward-frame";
		}
		frame_size = sizeof(struct frame_header) +
				(size_t)surface.width * surface.height * 4;
		int fd = open(frame_path, O_RDWR | O_CREAT, 0666);
		if (fd < 0) {
			return;
		}
		if (ftruncate(fd, (off_t)frame_size) != 0) {
			close(fd);
			return;
		}
		void *at = mmap(NULL, frame_size, PROT_READ | PROT_WRITE,
				MAP_SHARED, fd, 0);
		close(fd);
		if (at == MAP_FAILED) {
			return;
		}
		frame = at;
		frame_pixels = (uint8_t *)at + sizeof(struct frame_header);
		frame->magic = FRAME_MAGIC;
		frame->width = (uint32_t)surface.width;
		frame->height = (uint32_t)surface.height;
		frame->stride = (uint32_t)surface.stride;
	}

	size_t bytes = (size_t)surface.stride * surface.height;
	memcpy(frame_pixels, surface.pixels, bytes);
	__atomic_store_n(&frame->seq, ++frame_seq, __ATOMIC_RELEASE);
}

void skyward_frame_publish(void);

static stbtt_fontinfo face;
static unsigned char *face_data;
static float face_scale;
static int face_ascent;
static int face_line;

static void blend(int x, int y, uint32_t colour, uint8_t alpha)
{
	if (x < surface.clip_x0 || x >= surface.clip_x1 ||
			y < surface.clip_y0 || y >= surface.clip_y1) {
		return;
	}
	uint8_t *p = surface.pixels + y * surface.stride + x * 4;
	unsigned r = (colour >> 16) & 0xff;
	unsigned g = (colour >> 8) & 0xff;
	unsigned b = colour & 0xff;
	unsigned a = alpha;
	p[0] = (uint8_t)((r * a + p[0] * (255 - a)) / 255);
	p[1] = (uint8_t)((g * a + p[1] * (255 - a)) / 255);
	p[2] = (uint8_t)((b * a + p[2] * (255 - a)) / 255);
	p[3] = 255;
}

static void fill_span(int y, int x0, int x1, uint32_t colour)
{
	if (x0 < surface.clip_x0) {
		x0 = surface.clip_x0;
	}
	if (x1 > surface.clip_x1) {
		x1 = surface.clip_x1;
	}
	for (int x = x0; x < x1; x++) {
		blend(x, y, colour, 255);
	}
}

nserror skyward_plot_clip(const struct redraw_context *ctx,
		const struct rect *clip)
{
	{
		FILE *log = fopen("/tmp/skyplot.log", "a");
		if (log != NULL) {
			fprintf(log, "clip %d,%d %d,%d ready=%d\n", clip->x0, clip->y0,
					clip->x1, clip->y1, surface_ready);
			fclose(log);
		}
	}
	if (!surface_ready && clip->x1 > 0 && clip->y1 > 0) {
		skyward_surface_open(clip->x1, clip->y1);
	}
	surface.clip_x0 = clip->x0 < 0 ? 0 : clip->x0;
	surface.clip_y0 = clip->y0 < 0 ? 0 : clip->y0;
	surface.clip_x1 = clip->x1 > surface.width ? surface.width : clip->x1;
	surface.clip_y1 = clip->y1 > surface.height ? surface.height : clip->y1;
	if (surface.clip_x1 < surface.clip_x0) {
		surface.clip_x1 = surface.clip_x0;
	}
	if (surface.clip_y1 < surface.clip_y0) {
		surface.clip_y1 = surface.clip_y0;
	}
	return NSERROR_OK;
}

nserror skyward_plot_rectangle(const struct redraw_context *ctx,
		const plot_style_t *style, const struct rect *r)
{
	{
		FILE *log = fopen("/tmp/skyplot.log", "a");
		if (log != NULL) {
			fprintf(log, "rect %d,%d %d,%d fill=%u\n", (int)r->x0,
					(int)r->y0, (int)r->x1, (int)r->y1,
					(unsigned)style->fill_colour);
			fclose(log);
		}
	}
	int y0 = r->y0 < (float)surface.clip_y0 ? surface.clip_y0 : (int)r->y0;
	int y1 = r->y1 > (float)surface.clip_y1 ? surface.clip_y1 : (int)r->y1;
	if (style->fill_type != PLOT_OP_TYPE_NONE &&
			style->fill_type != PLOT_OP_TYPE_DOT) {
		for (int y = y0; y < y1; y++) {
			fill_span(y, (int)r->x0, (int)r->x1, style->fill_colour);
		}
	}
	if (style->stroke_type != PLOT_OP_TYPE_NONE) {
		for (int x = (int)r->x0; x < (int)r->x1; x++) {
			blend(x, (int)r->y0, style->stroke_colour, 255);
			blend(x, (int)r->y1 - 1, style->stroke_colour, 255);
		}
		for (int y = (int)r->y0; y < (int)r->y1; y++) {
			blend((int)r->x0, y, style->stroke_colour, 255);
			blend((int)r->x1 - 1, y, style->stroke_colour, 255);
		}
	}
	return NSERROR_OK;
}

static void draw_line(int x0, int y0, int x1, int y1, uint32_t colour)
{
	int dx = abs(x1 - x0);
	int dy = -abs(y1 - y0);
	int sx = x0 < x1 ? 1 : -1;
	int sy = y0 < y1 ? 1 : -1;
	int err = dx + dy;
	for (;;) {
		blend(x0, y0, colour, 255);
		if (x0 == x1 && y0 == y1) {
			break;
		}
		int e2 = 2 * err;
		if (e2 >= dy) {
			err += dy;
			x0 += sx;
		}
		if (e2 <= dx) {
			err += dx;
			y0 += sy;
		}
	}
}

nserror skyward_plot_line(const struct redraw_context *ctx,
		const plot_style_t *style, const struct rect *line)
{
	draw_line((int)line->x0, (int)line->y0, (int)line->x1, (int)line->y1,
			style->stroke_colour);
	return NSERROR_OK;
}

nserror skyward_plot_polygon(const struct redraw_context *ctx,
		const plot_style_t *style, const int *p, unsigned int n)
{
	for (unsigned int i = 0; i + 3 < n * 2; i += 2) {
		draw_line(p[i], p[i + 1], p[i + 2], p[i + 3], style->stroke_colour);
	}
	if (style->fill_type != PLOT_OP_TYPE_NONE && n > 2) {
		/* Scanline fill: even-odd, over the polygon's own span. */
		int ymin = p[1], ymax = p[1];
		for (unsigned int i = 0; i < n; i++) {
			if (p[i * 2 + 1] < ymin) ymin = p[i * 2 + 1];
			if (p[i * 2 + 1] > ymax) ymax = p[i * 2 + 1];
		}
		for (int y = ymin; y <= ymax; y++) {
			int xs[64];
			int count = 0;
			for (unsigned int i = 0; i < n && count < 62; i++) {
				unsigned int j = (i + 1) % n;
				int y0 = p[i * 2 + 1], y1 = p[j * 2 + 1];
				if ((y0 <= y && y1 > y) || (y1 <= y && y0 > y)) {
					int x0 = p[i * 2], x1 = p[j * 2];
					xs[count++] = x0 + (y - y0) * (x1 - x0) / (y1 - y0);
				}
			}
			for (int i = 0; i + 1 < count; i += 2) {
				if (xs[i] > xs[i + 1]) {
					int t = xs[i];
					xs[i] = xs[i + 1];
					xs[i + 1] = t;
				}
				fill_span(y, xs[i], xs[i + 1], style->fill_colour);
			}
		}
	}
	return NSERROR_OK;
}

nserror skyward_plot_disc(const struct redraw_context *ctx,
		const plot_style_t *style, int x, int y, int radius)
{
	int r2 = radius * radius;
	for (int dy = -radius; dy <= radius; dy++) {
		int span = 0;
		while ((span + 1) * (span + 1) + dy * dy <= r2) {
			span++;
		}
		if (style->fill_type != PLOT_OP_TYPE_NONE) {
			fill_span(y + dy, x - span, x + span + 1, style->fill_colour);
		} else {
			blend(x - span, y + dy, style->stroke_colour, 255);
			blend(x + span, y + dy, style->stroke_colour, 255);
		}
	}
	return NSERROR_OK;
}

nserror skyward_plot_arc(const struct redraw_context *ctx,
		const plot_style_t *style, int x, int y, int radius,
		int angle1, int angle2)
{
	/* Enough points for the arc to read as one at this size. */
	for (int a = angle1; a < angle2; a++) {
		double rad = a * 3.14159265 / 180.0;
		blend(x + (int)(radius * cos(rad)), y + (int)(radius * sin(rad)),
				style->stroke_colour, 255);
	}
	return NSERROR_OK;
}

nserror skyward_plot_path(const struct redraw_context *ctx,
		const plot_style_t *style, const float *p, unsigned int n,
		const float transform[6])
{
	int px = 0, py = 0;
	for (unsigned int i = 0; i < n; i++) {
		int x = (int)p[i * 2];
		int y = (int)p[i * 2 + 1];
		if (i > 0) {
			draw_line(px, py, x, y, style->stroke_colour);
		}
		px = x;
		py = y;
	}
	return NSERROR_OK;
}

nserror skyward_plot_bitmap(const struct redraw_context *ctx,
		struct bitmap *bitmap, int x, int y, int width, int height,
		colour bg, bitmap_flags_t flags)
{
	/* Bitmaps are the frontend's, and reaching into one needs its table;
	 * that is a step of its own. */
	(void)ctx; (void)bitmap; (void)x; (void)y; (void)width; (void)height;
	(void)bg; (void)flags;
	return NSERROR_OK;
}

static void draw_glyph(const plot_font_style_t *fstyle, uint32_t codepoint,
		int x, int y, int size)
{
	if (!stbtt_IsGlyphEmpty(&face, stbtt_FindGlyphIndex(&face, (int)codepoint))) {
		/* Scale for this size, in pixels, not the face's own unit scale. */
		float scale = stbtt_ScaleForPixelHeight(&face, (float)size);
		int advance = 0, left = 0;
		stbtt_GetCodepointHMetrics(&face, (int)codepoint, &advance, &left);
		int x0, y0, x1, y1;
		stbtt_GetCodepointBitmapBox(&face, (int)codepoint, scale, scale,
				&x0, &y0, &x1, &y1);
		int w = x1 - x0, h = y1 - y0;
		unsigned char *bitmap = malloc((size_t)(w * h));
		if (bitmap == NULL) {
			return;
		}
		stbtt_MakeCodepointBitmap(&face, bitmap, w, h, w, scale, scale,
				(int)codepoint);
		for (int row = 0; row < h; row++) {
			for (int col = 0; col < w; col++) {
				unsigned char cover = bitmap[row * w + col];
				if (cover > 8) {
					blend(x + x0 + col, y + y0 + row,
							fstyle->foreground, cover);
				}
			}
		}
		free(bitmap);
	}
}

static int texts_drawn;

nserror skyward_plot_text(const struct redraw_context *ctx,
		const plot_font_style_t *fstyle, int x, int y,
		const char *text, size_t length)
{
	if (texts_drawn < 4) {
		FILE *log = fopen("/tmp/skyplot.log", "a");
		if (log != NULL) {
			fprintf(log, "text len=%zu size=%d face=%p x=%d y=%d\n",
					length, (int)fstyle->size, (void *)face_data, x, y);
			fclose(log);
		}
	}
	texts_drawn++;
	if (text == NULL) {
		return NSERROR_OK;
	}
	int size = fstyle->size * 3 / (4 * PLOT_STYLE_SCALE);
	if (size < 6) {
		size = 6;
	}
	if (size > 128) {
		size = 128;
	}

	int px = x;
	size_t i = 0;
	while (i < length) {
		uint32_t cp = 0;
		unsigned char c = (unsigned char)text[i];
		size_t used = 1;
		if (c < 0x80) {
			cp = c;
		} else if ((c & 0xe0) == 0xc0 && i + 1 < length) {
			cp = (uint32_t)(c & 0x1f) << 6 |
					((unsigned char)text[i + 1] & 0x3f);
			used = 2;
		} else if ((c & 0xf0) == 0xe0 && i + 2 < length) {
			cp = (uint32_t)(c & 0x0f) << 12 |
					((uint32_t)((unsigned char)text[i + 1] & 0x3f) << 6) |
					((unsigned char)text[i + 2] & 0x3f);
			used = 3;
		} else if ((c & 0xf8) == 0xf0 && i + 3 < length) {
			cp = (uint32_t)(c & 0x07) << 18 |
					((uint32_t)((unsigned char)text[i + 1] & 0x3f) << 12) |
					((uint32_t)((unsigned char)text[i + 2] & 0x3f) << 6) |
					((unsigned char)text[i + 3] & 0x3f);
			used = 4;
		}

		draw_glyph(fstyle, cp, px, y, size);

		if (face_data != NULL) {
			float scale = stbtt_ScaleForPixelHeight(&face, (float)size);
			int advance = 0, bearing = 0;
			stbtt_GetCodepointHMetrics(&face, (int)cp, &advance, &bearing);
			px += (int)(advance * scale);
		} else {
			px += size;
		}
		i += used;
	}
	return NSERROR_OK;
}

nserror skyward_plot_group_start(const struct redraw_context *ctx,
		const char *name)
{
	return NSERROR_OK;
}

nserror skyward_plot_group_end(const struct redraw_context *ctx)
{
	return NSERROR_OK;
}

nserror skyward_plot_flush(const struct redraw_context *ctx)
{
	frame_publish();
	return NSERROR_OK;
}

void skyward_frame_publish(void)
{
	frame_publish();
}

static const struct plotter_table plotters = {
	.clip = skyward_plot_clip,
	.arc = skyward_plot_arc,
	.disc = skyward_plot_disc,
	.line = skyward_plot_line,
	.rectangle = skyward_plot_rectangle,
	.polygon = skyward_plot_polygon,
	.path = skyward_plot_path,
	.bitmap = skyward_plot_bitmap,
	.text = skyward_plot_text,
	.group_start = skyward_plot_group_start,
	.group_end = skyward_plot_group_end,
	.flush = skyward_plot_flush,
	.option_knockout = false,
};

const struct plotter_table *skyward_plotters = &plotters;

bool skyward_surface_open(int width, int height)
{
	if (surface_ready) {
		return true;
	}
	surface.width = width;
	surface.height = height;
	surface.stride = width * 4;
	surface.pixels = calloc((size_t)(surface.stride * height), 1);
	if (surface.pixels == NULL) {
		return false;
	}
	surface.clip_x0 = 0;
	surface.clip_y0 = 0;
	surface.clip_x1 = width;
	surface.clip_y1 = height;
	surface_ready = true;

	const char *path = getenv("SKYWARD_FONT");
	if (path != NULL) {
		FILE *f = fopen(path, "rb");
		if (f != NULL) {
			fseek(f, 0, SEEK_END);
			long size = ftell(f);
			fseek(f, 0, SEEK_SET);
			face_data = malloc((size_t)size);
			if (face_data != NULL &&
					fread(face_data, 1, (size_t)size, f) == (size_t)size &&
					stbtt_InitFont(&face, face_data,
						stbtt_GetFontOffsetForIndex(face_data, 0))) {
				int ascent = 0, descent = 0, line = 0;
				stbtt_GetFontVMetrics(&face, &ascent, &descent, &line);
				face_ascent = ascent;
				face_line = line;
				face_scale = stbtt_ScaleForPixelHeight(&face, 32.0f);
				if (face_scale <= 0) {
					face_scale = 1.0f;
				}
			}
			fclose(f);
		}
	}
	(void)face_ascent;
	(void)face_line;
	return true;
}

struct skyward_surface *skyward_surface_get(void)
{
	return &surface;
}

/* The name the rest of the frontend asks for. */
const struct plotter_table *monkey_plotters = &plotters;
