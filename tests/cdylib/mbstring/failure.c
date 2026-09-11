/* Invalid configuration arguments or a missing provider remain recoverable library failures. */
#include "libtext.h"
#include <stdio.h>
#include <string.h>

/* Checks status and stable error storage after init and ordinary export entry fail. */
static int failed(void) {
    const char *error = elephc_last_error();
    return elephc_last_status() == ELEPHC_STATUS_RUNTIME_FAILURE && error != NULL
        && strcmp(error, "elephc runtime boundary failed") == 0;
}

/* Exercises failure before any PHP body, preserving null output ownership and the host process. */
int main(void) {
    const char text[] = "abc";
    for (int i = 0; i < 8; i++) {
        if (elephc_init() != ELEPHC_STATUS_RUNTIME_FAILURE || !failed()) return 1;
        if (mb_size(text, 3) != 0 || !failed()) return 2;
        if (mb_weight(text, 3) != 0.0 || !failed()) return 5;
        char *out = (char *)text;
        size_t length = 99;
        if (mb_read(&out, &length) != ELEPHC_STATUS_RUNTIME_FAILURE || !failed()) return 3;
        if (out != NULL || length != 0) return 4;
    }
    elephc_shutdown();
    puts("configuration failure:host alive");
    return 0;
}
