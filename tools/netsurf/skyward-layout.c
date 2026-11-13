/*
 * Text measurement for NetSurf, from the same face the page is drawn with.
 *
 * NetSurf lays a page out before drawing it, and asks how wide a run of text
 * will be. The frontend NetSurf brings for embedding answers that with a
 * rough guess, which is enough for its tests and not enough to draw with:
 * measure with the real face instead, so the boxes the layout makes match
 * the glyphs that are later put in them.
 */

#include <stdbool.h>
#include <stdint.h>
#include <string.h>

#include "utils/errors.h"
#include "netsurf/layout.h"
#include "netsurf/plot_style.h"

/* From the drawing side: the width of a run, in pixels, at a size in pixels. */
int skyward_text_width(const char *text, size_t length, int px);

static int px_of(const plot_font_style_t *fstyle)
{
	int px = fstyle->size * 3 / (4 * PLOT_STYLE_SCALE);
	if (px < 6) {
		px = 6;
	}
	return px;
}

static size_t next_char(const char *text, size_t length, size_t at)
{
	unsigned char c = (unsigned char)text[at];
	if (c < 0x80) {
		return at + 1;
	}
	if ((c & 0xe0) == 0xc0 && at + 1 < length) {
		return at + 2;
	}
	if ((c & 0xf0) == 0xe0 && at + 2 < length) {
		return at + 3;
	}
	if ((c & 0xf8) == 0xf0 && at + 3 < length) {
		return at + 4;
	}
	return at + 1;
}

static nserror nsfont_width(const plot_font_style_t *fstyle, const char *string,
		size_t length, int *width)
{
	*width = skyward_text_width(string, length, px_of(fstyle));
	return NSERROR_OK;
}

static nserror nsfont_position_in_string(const plot_font_style_t *fstyle,
		const char *string, size_t length, int x, size_t *char_offset,
		int *actual_x)
{
	int px = px_of(fstyle);
	int run = 0;
	size_t at = 0;
	while (at < length) {
		size_t to = next_char(string, length, at);
		int step = skyward_text_width(string + at, to - at, px);
		if (run + step > x) {
			break;
		}
		run += step;
		at = to;
	}
	*char_offset = at;
	*actual_x = run;
	return NSERROR_OK;
}

static nserror nsfont_split(const plot_font_style_t *fstyle, const char *string,
		size_t length, int x, size_t *char_offset, int *actual_x)
{
	int px = px_of(fstyle);
	int run = 0;
	size_t at = 0;
	size_t last_space = 0;
	int at_space = 0;

	while (at < length) {
		size_t to = next_char(string, length, at);
		int step = skyward_text_width(string + at, to - at, px);
		if (run + step > x) {
			break;
		}
		run += step;
		at = to;
		if (string[at - 1] == ' ') {
			last_space = at;
			at_space = run;
		}
	}

	if (at < length && last_space > 0) {
		*char_offset = last_space;
		*actual_x = at_space;
	} else {
		*char_offset = at;
		*actual_x = run;
	}
	return NSERROR_OK;
}

static struct gui_layout_table layout_table = {
	.width = nsfont_width,
	.position = nsfont_position_in_string,
	.split = nsfont_split,
};

struct gui_layout_table *monkey_layout_table = &layout_table;
