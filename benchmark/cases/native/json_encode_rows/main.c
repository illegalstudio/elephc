#include <stdio.h>

/* Writes one unsigned decimal integer and returns the advanced cursor. */
static char *append_uint(char *out, unsigned value) {
    char digits[10];
    int count = 0;
    do {
        digits[count++] = (char)('0' + value % 10);
        value /= 10;
    } while (value != 0);
    while (count > 0) {
        *out++ = digits[--count];
    }
    return out;
}

int main(int argc, char **argv) {
    (void) argv;
    char row[8192];
    long long total = 0;
    long long limit = 29999 + argc;
    for (long long round = 0; round < limit; round++) {
        char *cursor = row;
        *cursor++ = '[';
        for (unsigned id = 0; id < 100; id++) {
            *cursor++ = '{';
            *cursor++ = '"'; *cursor++ = 'i'; *cursor++ = 'd'; *cursor++ = '"'; *cursor++ = ':';
            cursor = append_uint(cursor, id);
            *cursor++ = ','; *cursor++ = '"'; *cursor++ = 'n'; *cursor++ = 'a'; *cursor++ = 'm'; *cursor++ = 'e'; *cursor++ = '"'; *cursor++ = ':'; *cursor++ = '"';
            *cursor++ = 'i'; *cursor++ = 't'; *cursor++ = 'e'; *cursor++ = 'm';
            cursor = append_uint(cursor, id);
            *cursor++ = '"'; *cursor++ = ','; *cursor++ = '"'; *cursor++ = 'a'; *cursor++ = 'c'; *cursor++ = 't'; *cursor++ = 'i'; *cursor++ = 'v'; *cursor++ = 'e'; *cursor++ = '"'; *cursor++ = ':';
            *cursor++ = 't'; *cursor++ = 'r'; *cursor++ = 'u'; *cursor++ = 'e'; *cursor++ = '}';
            if (id != 99) {
                *cursor++ = ',';
            }
        }
        *cursor++ = ']';
        total += (long long)(cursor - row);
    }
    printf("%lld\n", total);
    return 0;
}
