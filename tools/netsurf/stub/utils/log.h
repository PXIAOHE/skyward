/*
 * The logging header libnsfb asks for, standing in for libnslog.
 *
 * A recovery has nowhere to send a browser's own logging, and the library
 * only ever writes through these macros, so they go nowhere.
 */

#ifndef SKYWARD_STUB_UTILS_LOG_H
#define SKYWARD_STUB_UTILS_LOG_H

#define NSLOG(category, level, ...) do { } while (0)
#define NSLOG_SET_FILTER(filter) do { } while (0)

#endif
