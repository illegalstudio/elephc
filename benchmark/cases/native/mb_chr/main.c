#include <stdint.h>
#include <stdio.h>
static int utf8_encode(uint32_t cp, unsigned char *out) {
    if (cp <= 0x7f) { out[0] = (unsigned char) cp; return 1; }
    if (cp <= 0x7ff) { out[0] = (unsigned char)(0xc0 | (cp >> 6)); out[1] = (unsigned char)(0x80 | (cp & 0x3f)); return 2; }
    if (cp <= 0xffff) { out[0] = (unsigned char)(0xe0 | (cp >> 12)); out[1] = (unsigned char)(0x80 | ((cp >> 6) & 0x3f)); out[2] = (unsigned char)(0x80 | (cp & 0x3f)); return 3; }
    out[0] = (unsigned char)(0xf0 | (cp >> 18)); out[1] = (unsigned char)(0x80 | ((cp >> 12) & 0x3f));
    out[2] = (unsigned char)(0x80 | ((cp >> 6) & 0x3f)); out[3] = (unsigned char)(0x80 | (cp & 0x3f)); return 4;
}
int main(int argc, char **argv) {
    (void) argv;
    static const uint32_t codepoints[4] = {0x41, 0xa9, 0x20ac, 0x1f600};
    long long limit = 999999 + argc, n = 0;
    unsigned char encoded[4];
    for (long long i = 0; i < limit; i++) { int len = utf8_encode(codepoints[i & 3], encoded); n += len + encoded[0]; }
    printf("%lld\n", n); return 0;
}
