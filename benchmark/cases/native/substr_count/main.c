#include <stdio.h>
#include <string.h>

int main(int argc, char **argv) {
    (void) argv;
    char haystack[2101];
    for (int i = 0; i < 700; i++) {
        memcpy(haystack + i * 3, "abc", 3);
    }
    haystack[2100] = '\0';

    long long total = 0;
    long long limit = 19999 + argc;
    for (long long i = 0; i < limit; i++) {
        const char *cursor = haystack;
        long long count = 0;
        while ((cursor = strstr(cursor, "abc")) != NULL) {
            count++;
            cursor += 3;
        }
        total += count;
    }
    printf("%lld\n", total);
    return 0;
}
