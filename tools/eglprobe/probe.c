/*
 * Says whether a phone's GPU can be used from wherever this runs: whether the
 * EGL library opens at all, what it calls itself, whether a context can be
 * made on it, and whether GLES answers through it.
 *
 * Nothing is linked at build time: every library is opened by name, so the
 * program runs where the headers it was built from have no libraries of their
 * own, as a recovery does.
 */
#include <dlfcn.h>
#include <fcntl.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

/* The handful of EGL's names this needs, none of them from a header. */
typedef void *EGLDisplay;
typedef void *EGLConfig;
typedef void *EGLContext;
typedef void *EGLSurface;
typedef int EGLint;
typedef unsigned int EGLBoolean;
typedef unsigned int EGLenum;

#define EGL_NO_DISPLAY ((EGLDisplay)0)
#define EGL_NO_CONTEXT ((EGLContext)0)
#define EGL_NO_SURFACE ((EGLSurface)0)
#define EGL_DEFAULT_DISPLAY ((void *)0)
#define EGL_OPENGL_ES_API 0x30A0
#define EGL_SURFACE_TYPE 0x3033
#define EGL_PBUFFER_BIT 0x0001
#define EGL_RENDERABLE_TYPE 0x3040
#define EGL_OPENGL_ES2_BIT 0x0004
#define EGL_OPENGL_ES3_BIT 0x0040
#define EGL_NONE 0x3038
#define EGL_WIDTH 0x3057
#define EGL_HEIGHT 0x3056
#define EGL_CONTEXT_CLIENT_VERSION 0x3098
#define EGL_VENDOR 0x3053
#define EGL_VERSION 0x3054
#define EGL_EXTENSIONS 0x3055
#define EGL_CLIENT_APIS 0x308D
#define EGL_RENDERER 0x305F

static void say(const char *what, const char *value) {
    printf("  %-18s %s\n", what, value ? value : "(none)");
}

/* Whether each of `names` is among the extension string. */
static void have(const char *extensions, const char *const *names, size_t count) {
    for (size_t i = 0; i < count; i++) {
        int found = extensions && strstr(extensions, names[i]) != NULL;
        printf("  %-18s %s\n", names[i], found ? "yes" : "no");
    }
}

int main(void) {
    printf("== the libraries ==\n");
    const char *egl_paths[] = {
        "libEGL.so", "/system/lib64/libEGL.so", "/vendor/lib64/libEGL.so", NULL,
    };
    void *egl = NULL;
    const char *egl_used = NULL;
    for (int i = 0; egl_paths[i] && !egl; i++) {
        egl = dlopen(egl_paths[i], RTLD_NOW);
        if (egl) {
            egl_used = egl_paths[i];
        } else {
            printf("  %-18s %s\n", egl_paths[i], dlerror());
        }
    }
    if (!egl) {
        printf("\nlibEGL cannot be opened at all: no GPU this way.\n");
        return 1;
    }
    say("libEGL from", egl_used);

    void *gles = dlopen("libGLESv2.so", RTLD_NOW);
    if (!gles) {
        gles = dlopen("/system/lib64/libGLESv2.so", RTLD_NOW);
    }
    printf("  %-18s %s\n", "libGLESv2", gles ? "opens" : "does not open");

    printf("\n== the display ==\n");
    EGLDisplay (*get_display)(void *) = (EGLDisplay(*)(void *))dlsym(egl, "eglGetDisplay");
    EGLBoolean (*initialize)(EGLDisplay, EGLint *, EGLint *) = (EGLBoolean(*)(EGLDisplay, EGLint *, EGLint *))dlsym(egl, "eglInitialize");
    const char *(*query_string)(EGLDisplay, EGLint) = (const char *(*)(EGLDisplay, EGLint))dlsym(egl, "eglQueryString");
    EGLint (*get_error)(void) = (EGLint(*)(void))dlsym(egl, "eglGetError");
    if (!get_display || !initialize || !query_string) {
        printf("  the library has no eglGetDisplay/eglInitialize: not EGL.\n");
        return 1;
    }

    EGLDisplay display = get_display(EGL_DEFAULT_DISPLAY);
    if (display == EGL_NO_DISPLAY) {
        printf("  eglGetDisplay(EGL_DEFAULT_DISPLAY)  no display (0x%x)\n", get_error ? get_error() : 0);
        printf("\nNo EGL display: the GPU is not reachable this way.\n");
        return 1;
    }
    EGLint major = 0, minor = 0;
    if (!initialize(display, &major, &minor)) {
        printf("  eglInitialize                      fails (0x%x)\n", get_error ? get_error() : 0);
        printf("\nNo EGL: the driver is there but does not come up.\n");
        return 1;
    }
    printf("  %-18s %d.%d\n", "EGL", major, minor);
    say("vendor", query_string(display, EGL_VENDOR));
    say("version", query_string(display, EGL_VERSION));
    say("client apis", query_string(display, EGL_CLIENT_APIS));
    say("renderer", query_string(display, EGL_RENDERER) ? query_string(display, EGL_RENDERER) : "");

    const char *extensions = query_string(display, EGL_EXTENSIONS);
    printf("\n== the extensions a browser backend leans on ==\n");
    const char *names[] = {
        "EGL_KHR_surfaceless_context",
        "EGL_KHR_image_base",
        "EGL_KHR_fence_sync",
        "EGL_EXT_image_dma_buf_import",
        "EGL_EXT_image_dma_buf_import_modifiers",
        "EGL_ANDROID_image_native_buffer",
        "EGL_KHR_platform_gbm",
        "EGL_EXT_platform_base",
        "EGL_MESA_platform_gbm",
    };
    have(extensions, names, sizeof(names) / sizeof(names[0]));

    printf("\n== a context ==\n");
    EGLBoolean (*bind_api)(EGLenum) = (EGLBoolean(*)(EGLenum))dlsym(egl, "eglBindAPI");
    EGLContext (*create_context)(EGLDisplay, EGLConfig, EGLContext, const EGLint *) =
        (EGLContext(*)(EGLDisplay, EGLConfig, EGLContext, const EGLint *))dlsym(egl, "eglCreateContext");
    EGLBoolean (*make_current)(EGLDisplay, EGLSurface, EGLSurface, EGLContext) =
        (EGLBoolean(*)(EGLDisplay, EGLSurface, EGLSurface, EGLContext))dlsym(egl, "eglMakeCurrent");

    if (!bind_api || !create_context || !make_current) {
        printf("  the library cannot make a context.\n");
        return 0;
    }
    bind_api(EGL_OPENGL_ES_API);
    EGLint attributes[] = {EGL_CONTEXT_CLIENT_VERSION, 3, EGL_NONE};
    EGLContext context = create_context(display, NULL, EGL_NO_CONTEXT, attributes);
    if (context == EGL_NO_CONTEXT) {
        printf("  EGL ES3 context    fails (0x%x)\n", get_error ? get_error() : 0);
        attributes[1] = 2;
        context = create_context(display, NULL, EGL_NO_CONTEXT, attributes);
        printf("  EGL ES2 context    %s\n", context == EGL_NO_CONTEXT ? "fails too" : "made");
        if (context == EGL_NO_CONTEXT) {
            return 0;
        }
    } else {
        printf("  EGL ES3 context    made\n");
    }

    /* A context with no window: what a browser rendering offscreen needs. */
    if (make_current(display, EGL_NO_SURFACE, EGL_NO_SURFACE, context)) {
        printf("  surfaceless        works\n");
    } else {
        printf("  surfaceless        fails (0x%x)\n", get_error ? get_error() : 0);
    }

    if (gles) {
        printf("\n== what GLES answers ==\n");
        const unsigned char *(*gl_get_string)(unsigned int) =
            (const unsigned char *(*)(unsigned int))dlsym(gles, "glGetString");
        if (gl_get_string) {
            say("GL_VENDOR", (const char *)gl_get_string(0x1F00));
            say("GL_RENDERER", (const char *)gl_get_string(0x1F01));
            say("GL_VERSION", (const char *)gl_get_string(0x1F02));
        }
    }

    printf("\n== the pieces a browser backend needs ==\n");
    const char *files[] = {
        "/dev/dri/card0",
        "/dev/dri/renderD128",
        "/dev/kgsl-3d0",
        "/dev/dma_heap/system",
        "/dev/dma_heap/qcom,system",
    };
    for (size_t i = 0; i < sizeof(files) / sizeof(files[0]); i++) {
        int fd = open(files[i], O_RDWR);
        if (fd < 0) {
            fd = open(files[i], O_RDONLY);
        }
        printf("  %-26s %s\n", files[i], fd >= 0 ? "opens" : "no");
        if (fd >= 0) {
            close(fd);
        }
    }
    const char *gbm[] = {"libgbm.so", "/vendor/lib64/libgbm.so", "/system/lib64/libgbm.so"};
    for (size_t i = 0; i < sizeof(gbm) / sizeof(gbm[0]); i++) {
        void *h = dlopen(gbm[i], RTLD_NOW);
        printf("  %-26s %s\n", gbm[i], h ? "opens" : "no");
        if (h) {
            dlclose(h);
        }
    }

    printf("\nDone.\n");
    return 0;
}
