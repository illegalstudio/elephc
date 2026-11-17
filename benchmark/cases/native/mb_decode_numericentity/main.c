#include <stdint.h>
#include <stdio.h>
int main(int argc, char **argv) {
    (void) argv;
    static const unsigned char source[] = "caf&#233; &#29483; &#128512; <&>";
    long long limit = 499999 + argc, n = 0;
    unsigned char out[64];
    for (long long i = 0; i < limit; i++) {
        const unsigned char *cur = source; size_t w = 0;
        while (*cur) {
            if (cur[0] != '&' || cur[1] != '#') { out[w++] = *cur++; continue; }
            cur += 2; uint32_t code = 0;
            while (*cur >= '0' && *cur <= '9') { code = code * 10 + (uint32_t)(*cur - '0'); cur++; }
            if (*cur == ';') cur++;
            if (code < 0x800) { out[w++] = (unsigned char)(0xC0 | (code >> 6)); out[w++] = (unsigned char)(0x80 | (code & 0x3F)); }
            else if (code < 0x10000) { out[w++] = (unsigned char)(0xE0 | (code >> 12)); out[w++] = (unsigned char)(0x80 | ((code >> 6) & 0x3F)); out[w++] = (unsigned char)(0x80 | (code & 0x3F)); }
            else { out[w++] = (unsigned char)(0xF0 | (code >> 18)); out[w++] = (unsigned char)(0x80 | ((code >> 12) & 0x3F)); out[w++] = (unsigned char)(0x80 | ((code >> 6) & 0x3F)); out[w++] = (unsigned char)(0x80 | (code & 0x3F)); }
        }
        n += (long long)w + out[0];
    }
    printf("%lld\n", n); return 0;
}
