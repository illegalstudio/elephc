#include <stdio.h>
#include <stdlib.h>
#include <string.h>

int main(void) {
    size_t capacity = 15001;
    char *out = malloc(capacity);

    if (out == NULL) {
        return 1;
    }

    out[0] = '\0';
    for (int i = 0; i < 5000; ++i) {
        strcat(out, "abc");
    }

    printf("%zu\n", strlen(out));
    free(out);
    return 0;
}
