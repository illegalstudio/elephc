/* Real-header build probe support, used only by the compiler's SDK discovery. */
#include <php.h>
#include <stddef.h>
#include <stdio.h>

#if defined(__APPLE__) && defined(__aarch64__)
# define ELEPHC_HOST_TARGET "macos-aarch64"
#elif defined(__linux__) && defined(__aarch64__)
# define ELEPHC_HOST_TARGET "linux-aarch64"
#elif defined(__linux__) && defined(__x86_64__)
# define ELEPHC_HOST_TARGET "linux-x86_64"
#else
# define ELEPHC_HOST_TARGET "unsupported"
#endif

/* Emit JSON safely even when a host adds quotes or control bytes to its build ID. */
static void json_string(const char *text)
{
    putchar('"');
    for (const unsigned char *p = (const unsigned char *)text; *p; ++p) {
        if (*p == '"' || *p == '\\') {
            putchar('\\');
            putchar(*p);
        } else if (*p < 32) {
            printf("\\u%04x", (unsigned)*p);
        } else {
            putchar(*p);
        }
    }
    putchar('"');
}
