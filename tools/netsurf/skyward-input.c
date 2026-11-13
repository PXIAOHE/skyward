/*
 * A finger on the page, from the program around NetSurf.
 *
 * The frontend reads its input from the surface, and the memory surface has
 * none: nothing arrives, nothing is clicked. This listens on a socket instead
 * and hands what it reads to the same place the surface's events go, so a
 * touch from the recovery reaches the page as a press and a release.
 *
 * Lines, one command each:
 *
 *   d <x> <y>    a finger down
 *   u <x> <y>    the finger lifted
 *   m <x> <y>    the finger moved
 *   w <up|down>  the wheel
 */

#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <time.h>
#include <sys/socket.h>
#include <sys/un.h>

#include <libnsfb.h>
#include <libnsfb_event.h>

#include "fbtk.h"

/* The same dispatch the loop does for the surface's own events: a movement
 * warps the pointer, a mouse button goes to whatever is under it, and a key
 * goes to the widget that has the focus. */
static void skyward_send_event(fbtk_widget_t *fbtk, enum nsfb_event_type_e type,
		int x, int y, enum nsfb_key_code_e key)
{
	nsfb_event_t event;
	memset(&event, 0, sizeof(event));
	event.type = type;

	if (type == NSFB_EVENT_MOVE_ABSOLUTE) {
		event.value.vector.x = x;
		event.value.vector.y = y;
		fbtk_warp_pointer(fbtk, x, y, false);
		return;
	}

	event.value.keycode = key;
	if (key >= NSFB_KEY_MOUSE_1 && key <= NSFB_KEY_MOUSE_5) {
		fbtk_click(fbtk, &event);
	} else {
		fbtk_input(fbtk, &event);
	}
}

static void skyward_run_command(fbtk_widget_t *fbtk, char *line)
{
	char *what = strtok(line, " \t");
	if (what == NULL) {
		return;
	}
	if (strcmp(what, "d") == 0 || strcmp(what, "u") == 0 ||
			strcmp(what, "m") == 0) {
		char *xs = strtok(NULL, " \t");
		char *ys = strtok(NULL, " \t");
		if (xs == NULL || ys == NULL) {
			return;
		}
		int x = atoi(xs);
		int y = atoi(ys);
		skyward_send_event(fbtk, NSFB_EVENT_MOVE_ABSOLUTE, x, y, 0);
		if (strcmp(what, "d") == 0) {
			skyward_send_event(fbtk, NSFB_EVENT_KEY_DOWN, x, y,
					NSFB_KEY_MOUSE_1);
		} else if (strcmp(what, "u") == 0) {
			skyward_send_event(fbtk, NSFB_EVENT_KEY_UP, x, y,
					NSFB_KEY_MOUSE_1);
		}
	} else if (strcmp(what, "w") == 0) {
		char *way = strtok(NULL, " \t");
		enum nsfb_key_code_e key = NSFB_KEY_MOUSE_4;
		if (way != NULL && strcmp(way, "down") == 0) {
			key = NSFB_KEY_MOUSE_5;
		}
		skyward_send_event(fbtk, NSFB_EVENT_KEY_DOWN, 0, 0, key);
		skyward_send_event(fbtk, NSFB_EVENT_KEY_UP, 0, 0, key);
	}
}

/* Take whatever has arrived, as whole lines, and let it through. */
void skyward_input_poll(fbtk_widget_t *fbtk)
{
	static int listening = -2;
	static int connected = -1;
	static char buffered[1024];
	static size_t used;

	if (listening == -2) {
		const char *path = getenv("SKYWARD_INPUT");
		if (path == NULL) {
			path = "/tmp/skyward-input";
		}
		listening = socket(AF_UNIX, SOCK_STREAM, 0);
		if (listening >= 0) {
			struct sockaddr_un addr;
			memset(&addr, 0, sizeof(addr));
			addr.sun_family = AF_UNIX;
			strncpy(addr.sun_path, path, sizeof(addr.sun_path) - 1);
			unlink(path);
			if (bind(listening, (struct sockaddr *)&addr, sizeof(addr)) != 0 ||
					listen(listening, 2) != 0) {
				close(listening);
				listening = -1;
			} else {
				fcntl(listening, F_SETFL, O_NONBLOCK);
			}
		}
	}
	if (listening < 0) {
		return;
	}

	if (connected < 0) {
		connected = accept(listening, NULL, NULL);
		if (connected < 0) {
			return;
		}
		fcntl(connected, F_SETFL, O_NONBLOCK);
	}

	ssize_t got = read(connected, buffered + used, sizeof(buffered) - used - 1);
	if (got > 0) {
		used += (size_t)got;
		buffered[used] = '\0';
		char *start = buffered;
		char *end;
		while ((end = strchr(start, '\n')) != NULL) {
			*end = '\0';
			skyward_run_command(fbtk, start);
			start = end + 1;
		}
		size_t left = (size_t)(buffered + used - start);
		memmove(buffered, start, left);
		used = left;
	} else if (got == 0) {
		close(connected);
		connected = -1;
		used = 0;
	}
}

/* The loop has nothing to wait on with a memory surface, so it would spin;
 * a short sleep leaves the processor to the recovery beside it. */
void skyward_idle(void)
{
	struct timespec pause = { 0, 20 * 1000 * 1000 };
	nanosleep(&pause, NULL);
}
