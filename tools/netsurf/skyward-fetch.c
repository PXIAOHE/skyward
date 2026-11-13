/*
 * A fetcher that asks the program around NetSurf for the bytes.
 *
 * A recovery has no business carrying a second TLS stack, a second DNS
 * resolver and a cookie jar of its own: the environment it lives in already
 * speaks HTTPS and already knows the network's name servers. So this fetcher
 * hands the whole URL to the program that started it, over a unix socket,
 * and receives the HTTP response exactly as the server wrote it — status
 * line, headers and body — to parse and pass on.
 *
 * Wire format, both directions text plus raw bytes:
 *
 *   -> GET <url>\n
 *   <- HTTP/1.1 200 OK\r\n ... headers ... \r\n\r\n ... body ...
 *
 * A POST carries its body after the request line:
 *
 *   -> POST <url> <length>\n<length bytes>
 *
 * The socket is named by SKYWARD_FETCH_SOCKET, or /tmp/skyward-fetch.
 */

#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <errno.h>
#include <fcntl.h>
#include <sys/socket.h>
#include <sys/un.h>
#include <time.h>

#include "utils/errors.h"
#include "utils/log.h"
#include "utils/nsurl.h"
#include "utils/utils.h"
#include "content/fetch.h"
#include "content/fetchers.h"
#include "content/fetchers/skyward.h"

#define DEFAULT_SOCKET "/tmp/skyward-fetch"
#define READ_CHUNK 16384

enum parse_state {
	STATE_STATUS,
	STATE_HEADERS,
	STATE_BODY,
	STATE_DONE,
};

struct skyward_fetch {
	struct skyward_fetch *next;
	struct fetch *fetch_handle;
	char *url;
	char *post;
	size_t post_len;
	int fd;
	bool started;
	enum parse_state state;
	char *buf;
	size_t used;
	long http_code;
	long content_length;   /* -1 when the server did not say */
	size_t body_seen;
	bool chunked;
	size_t chunk_left;
	bool chunk_done;
	char *location;
	bool finished;
};

static struct skyward_fetch *active;

static void skyward_clear(struct skyward_fetch *f)
{
	free(f->url);
	free(f->post);
	free(f->buf);
	free(f->location);
	if (f->fd >= 0) {
		close(f->fd);
	}
	free(f);
}

static void skyward_send(struct skyward_fetch *f, fetch_msg_type type,
		const char *text, const uint8_t *data, size_t len)
{
	fetch_msg msg;
	msg.type = type;
	if (type == FETCH_DATA || type == FETCH_HEADER) {
		msg.data.header_or_data.buf = data;
		msg.data.header_or_data.len = len;
	} else {
		msg.data.error = text;
	}
	fetch_send_callback(&msg, f->fetch_handle);
}

static void skyward_error(struct skyward_fetch *f, const char *why)
{
	if (f->finished) {
		return;
	}
	f->finished = true;
	skyward_send(f, FETCH_ERROR, why, NULL, 0);
}

/* Take one header line out of the buffer, leaving the rest. Returns false
 * when no complete line is buffered yet. */
static bool take_line(struct skyward_fetch *f, char **line, size_t *len)
{
	char *end = memchr(f->buf, '\n', f->used);
	if (end == NULL) {
		return false;
	}
	*line = f->buf;
	*len = (size_t)(end - f->buf) + 1;
	return true;
}

static void drop_line(struct skyward_fetch *f, size_t len)
{
	memmove(f->buf, f->buf + len, f->used - len);
	f->used -= len;
}

static bool name_is(const char *line, size_t len, const char *name)
{
	size_t n = strlen(name);
	if (len < n + 1) {
		return false;
	}
	for (size_t i = 0; i < n; i++) {
		char a = line[i];
		if (a >= 'A' && a <= 'Z') {
			a = (char)(a - 'A' + 'a');
		}
		if (a != name[i]) {
			return false;
		}
	}
	return line[n] == ':';
}

static char *value_of(const char *line, size_t len, size_t skip)
{
	size_t start = skip;
	while (start < len && (line[start] == ' ' || line[start] == '\t')) {
		start++;
	}
	size_t end = len;
	while (end > start && (line[end - 1] == '\r' || line[end - 1] == '\n' ||
			line[end - 1] == ' ' || line[end - 1] == '\t')) {
		end--;
	}
	char *out = malloc(end - start + 1);
	if (out == NULL) {
		return NULL;
	}
	memcpy(out, line + start, end - start);
	out[end - start] = '\0';
	return out;
}

static void parse_status(struct skyward_fetch *f, const char *line, size_t len)
{
	long code = 0;
	const char *p = line;
	while (p < line + len && (*p < '0' || *p > '9')) {
		p++;
	}
	while (p < line + len && *p >= '0' && *p <= '9') {
		code = code * 10 + (*p - '0');
		p++;
	}
	f->http_code = code;
	fetch_set_http_code(f->fetch_handle, code);
	f->state = STATE_HEADERS;
}

static void parse_header(struct skyward_fetch *f, const char *line, size_t len)
{
	if (len <= 2) {
		/* The blank line that ends the headers. */
		if (f->http_code == 301 || f->http_code == 302 ||
				f->http_code == 303 || f->http_code == 307 ||
				f->http_code == 308) {
			if (f->location != NULL) {
				f->finished = true;
				skyward_send(f, FETCH_REDIRECT, f->location, NULL, 0);
				return;
			}
		}
		f->state = STATE_BODY;
		return;
	}

	if (name_is(line, len, "content-length")) {
		char *value = value_of(line, len, 15);
		if (value != NULL) {
			f->content_length = strtol(value, NULL, 10);
			free(value);
		}
	} else if (name_is(line, len, "transfer-encoding")) {
		char *value = value_of(line, len, 18);
		if (value != NULL) {
			for (char *c = value; *c != '\0'; c++) {
				if (*c >= 'A' && *c <= 'Z') {
					*c = (char)(*c - 'A' + 'a');
				}
			}
			if (strstr(value, "chunked") != NULL) {
				f->chunked = true;
			}
			free(value);
		}
	} else if (name_is(line, len, "location")) {
		free(f->location);
		f->location = value_of(line, len, 9);
	}

	skyward_send(f, FETCH_HEADER, NULL, (const uint8_t *)line, len);
}

static void parse_body(struct skyward_fetch *f)
{
	if (!f->chunked) {
		size_t take = f->used;
		if (f->content_length >= 0) {
			size_t left = (size_t)f->content_length - f->body_seen;
			if (take > left) {
				take = left;
			}
		}
		if (take > 0) {
			skyward_send(f, FETCH_DATA, NULL, (const uint8_t *)f->buf, take);
			f->body_seen += take;
			drop_line(f, take);
		}
		if (f->content_length >= 0 &&
				f->body_seen >= (size_t)f->content_length) {
			f->state = STATE_DONE;
		}
		return;
	}

	/* Chunked: a hex size line, that many bytes, then a blank line. */
	while (f->used > 0) {
		if (f->chunk_left == 0) {
			char *line;
			size_t len;
			if (!take_line(f, &line, &len)) {
				return;
			}
			char *end = NULL;
			unsigned long size = strtoul(line, &end, 16);
			drop_line(f, len);
			if (size == 0) {
				/* Trailer: swallow until a blank line. */
				f->state = STATE_DONE;
				return;
			}
			f->chunk_left = (size_t)size;
			continue;
		}

		size_t take = f->used < f->chunk_left ? f->used : f->chunk_left;
		skyward_send(f, FETCH_DATA, NULL, (const uint8_t *)f->buf, take);
		f->body_seen += take;
		f->chunk_left -= take;
		drop_line(f, take);

		if (f->chunk_left == 0) {
			/* The CRLF the chunk is followed by. */
			if (f->used >= 2 && f->buf[0] == '\r' && f->buf[1] == '\n') {
				drop_line(f, 2);
			} else if (f->used >= 1 && f->buf[0] == '\n') {
				drop_line(f, 1);
			}
		}
	}
}

static void parse(struct skyward_fetch *f)
{
	while (f->state != STATE_DONE) {
		if (f->state == STATE_STATUS) {
			char *line;
			size_t len;
			if (!take_line(f, &line, &len)) {
				return;
			}
			parse_status(f, line, len);
			drop_line(f, len);
		} else if (f->state == STATE_HEADERS) {
			char *line;
			size_t len;
			if (!take_line(f, &line, &len)) {
				return;
			}
			parse_header(f, line, len);
			drop_line(f, len);
			if (f->finished) {
				return;
			}
		} else {
			size_t before = f->used;
			parse_body(f);
			if (f->used == before && f->state == STATE_BODY) {
				return; /* nothing more to take without more bytes */
			}
		}
	}
}

bool fetch_skyward_initialise(lwc_string *scheme)
{
	return true;
}

bool fetch_skyward_acceptable(const struct nsurl *url)
{
	/* Everything that speaks HTTP is asked of the program around us; the
	 * other schemes have fetchers of their own. */
	const char *text = nsurl_access(url);
	return strncmp(text, "http://", 7) == 0 ||
			strncmp(text, "https://", 8) == 0;
}

static int connect_to(const char *path)
{
	int fd = socket(AF_UNIX, SOCK_STREAM, 0);
	if (fd < 0) {
		return -1;
	}
	struct sockaddr_un addr;
	memset(&addr, 0, sizeof(addr));
	addr.sun_family = AF_UNIX;
	strncpy(addr.sun_path, path, sizeof(addr.sun_path) - 1);
	if (connect(fd, (struct sockaddr *)&addr, sizeof(addr)) < 0) {
		close(fd);
		return -1;
	}
	return fd;
}

void *fetch_skyward_setup(struct fetch *parent_fetch, struct nsurl *url,
		bool only_2xx, bool downgrade_tls, const char *post_urlenc,
		const struct fetch_multipart_data *post_multipart,
		const char **headers)
{
	if (post_multipart != NULL) {
		return NULL; /* not carried over this socket */
	}

	struct skyward_fetch *f = calloc(1, sizeof(*f));
	if (f == NULL) {
		return NULL;
	}
	f->fetch_handle = parent_fetch;
	f->fd = -1;
	f->state = STATE_STATUS;
	f->content_length = -1;
	f->url = strdup(nsurl_access(url));

	if (post_urlenc != NULL) {
		f->post = strdup(post_urlenc);
		f->post_len = strlen(post_urlenc);
	}

	f->buf = malloc(READ_CHUNK);
	if (f->url == NULL || f->buf == NULL) {
		skyward_clear(f);
		return NULL;
	}
	return f;
}

bool fetch_skyward_start(void *vfetch)
{
	struct skyward_fetch *f = vfetch;
	const char *path = getenv("SKYWARD_FETCH_SOCKET");
	if (path == NULL) {
		path = DEFAULT_SOCKET;
	}

	f->fd = connect_to(path);
	if (f->fd < 0) {
		skyward_error(f, "the program that holds the network is not there");
		return false;
	}

	char *request;
	size_t request_len;
	if (f->post != NULL) {
		size_t need = strlen(f->url) + 32;
		request = malloc(need);
		if (request == NULL) {
			skyward_error(f, "out of memory");
			return false;
		}
		request_len = (size_t)snprintf(request, need, "POST %s %zu\n",
				f->url, f->post_len);
	} else {
		request = malloc(strlen(f->url) + 8);
		if (request == NULL) {
			skyward_error(f, "out of memory");
			return false;
		}
		request_len = (size_t)snprintf(request, strlen(f->url) + 8,
				"GET %s\n", f->url);
	}

	bool ok = write(f->fd, request, request_len) == (ssize_t)request_len;
	if (ok && f->post != NULL) {
		ok = write(f->fd, f->post, f->post_len) == (ssize_t)f->post_len;
	}
	free(request);

	if (!ok) {
		skyward_error(f, "the request could not be handed over");
		return false;
	}

	int flags = fcntl(f->fd, F_GETFL, 0);
	fcntl(f->fd, F_SETFL, flags | O_NONBLOCK);

	f->started = true;
	f->next = active;
	active = f;
	return true;
}

void fetch_skyward_abort(void *vfetch)
{
	struct skyward_fetch *f = vfetch;
	f->finished = true;
}

void fetch_skyward_free(void *vfetch)
{
	struct skyward_fetch *f = vfetch;
	struct skyward_fetch **link = &active;
	while (*link != NULL) {
		if (*link == f) {
			*link = f->next;
			break;
		}
		link = &(*link)->next;
	}
	skyward_clear(f);
}

void fetch_skyward_poll(lwc_string *scheme)
{
	struct skyward_fetch *f = active;
	while (f != NULL) {
		struct skyward_fetch *next = f->next;
		if (!f->started || f->finished) {
			f = next;
			continue;
		}

		ssize_t got;
		while ((got = read(f->fd, f->buf + f->used,
				READ_CHUNK - f->used)) > 0) {
			f->used += (size_t)got;
			parse(f);
			if (f->finished || f->state == STATE_DONE) {
				break;
			}
			if (f->used == READ_CHUNK) {
				break;
			}
		}

		if (!f->finished) {
			if (got == 0 || (got < 0 && errno != EAGAIN &&
					errno != EWOULDBLOCK && errno != EINTR)) {
				parse(f); /* whatever is left */
				if (f->used > 0 && f->state == STATE_BODY) {
					parse_body(f);
				}
				f->finished = true;
				skyward_send(f, FETCH_FINISHED, NULL, NULL, 0);
			} else if (f->state == STATE_DONE) {
				f->finished = true;
				skyward_send(f, FETCH_FINISHED, NULL, NULL, 0);
			}
		}

		f = next;
	}
}

int fetch_skyward_fdset(lwc_string *scheme, fd_set *read_set,
		fd_set *write_set, fd_set *error_set)
{
	int count = 0;
	for (struct skyward_fetch *f = active; f != NULL; f = f->next) {
		if (f->started && !f->finished && f->fd >= 0) {
			FD_SET(f->fd, read_set);
			FD_SET(f->fd, error_set);
			count++;
		}
	}
	return count;
}

void fetch_skyward_finalise(lwc_string *scheme)
{
}

static struct fetcher_operation_table skyward_ops = {
	.initialise = fetch_skyward_initialise,
	.acceptable = fetch_skyward_acceptable,
	.setup = fetch_skyward_setup,
	.start = fetch_skyward_start,
	.abort = fetch_skyward_abort,
	.free = fetch_skyward_free,
	.poll = fetch_skyward_poll,
	.fdset = fetch_skyward_fdset,
	.finalise = fetch_skyward_finalise,
};

nserror fetch_skyward_register(void)
{
	lwc_string *scheme;
	lwc_error error;

	error = lwc_intern_string("http", 4, &scheme);
	if (error == lwc_error_ok) {
		error = fetcher_add(scheme, &skyward_ops) == NSERROR_OK ?
				lwc_error_ok : lwc_error_oom;
	}
	error = lwc_intern_string("https", 5, &scheme);
	if (error == lwc_error_ok) {
		fetcher_add(scheme, &skyward_ops);
	}
	return NSERROR_OK;
}

/*
 * NetSurf reads the date a server puts on a response through a function it
 * took from curl. With curl gone, the parsing it needs is here: the HTTP
 * date forms, as RFC 9110 spells them.
 */
#ifndef __GLIBC__
time_t timegm(struct tm *tm);
#endif

time_t curl_getdate(const char *text, const time_t *unused)
{
	static const char *const months[] = {
		"Jan", "Feb", "Mar", "Apr", "May", "Jun",
		"Jul", "Aug", "Sep", "Oct", "Nov", "Dec"
	};
	struct tm tm;
	memset(&tm, 0, sizeof(tm));

	const char *p = text;
	if (p == NULL) {
		return -1;
	}

	/* A leading weekday name, with or without a comma, is not needed. */
	while (*p != '\0' && ((*p >= 'A' && *p <= 'Z') ||
			(*p >= 'a' && *p <= 'z'))) {
		p++;
	}
	if (*p == ',') {
		p++;
	}
	while (*p == ' ') {
		p++;
	}

	/* day */
	char *end = NULL;
	long day = strtol(p, &end, 10);
	if (end == p) {
		return -1;
	}
	p = end;
	while (*p == ' ' || *p == '-') {
		p++;
	}

	/* month */
	int month = -1;
	for (int i = 0; i < 12; i++) {
		if (strncmp(p, months[i], 3) == 0) {
			month = i;
			break;
		}
	}
	if (month < 0) {
		return -1;
	}
	p += 3;
	while (*p == ' ' || *p == '-') {
		p++;
	}

	/* year: two digits mean this century, as the old format uses */
	long year = strtol(p, &end, 10);
	if (end == p) {
		return -1;
	}
	if (year < 100) {
		year += 2000;
	}
	p = end;
	while (*p == ' ') {
		p++;
	}

	long hour = 0;
	long minute = 0;
	long second = 0;
	if (*p != '\0') {
		hour = strtol(p, &end, 10);
		if (end == p || *end != ':') {
			return -1;
		}
		p = end + 1;
		minute = strtol(p, &end, 10);
		if (end == p || *end != ':') {
			return -1;
		}
		p = end + 1;
		second = strtol(p, &end, 10);
	}

	tm.tm_mday = (int)day;
	tm.tm_mon = month;
	tm.tm_year = (int)year - 1900;
	tm.tm_hour = (int)hour;
	tm.tm_min = (int)minute;
	tm.tm_sec = (int)second;
	tm.tm_isdst = 0;

	(void)unused;
	return timegm(&tm);
}
