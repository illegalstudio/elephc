#include <stdint.h>
#include <stdio.h>
static int utf8_decode(const unsigned char *p, uint32_t *code) {
    unsigned char lead = p[0];
    if (lead < 0x80) { *code = lead; return 1; }
    if (lead < 0xE0) { *code = ((uint32_t)(lead & 0x1F) << 6) | (p[1] & 0x3F); return 2; }
    if (lead < 0xF0) { *code = ((uint32_t)(lead & 0x0F) << 12) | ((uint32_t)(p[1] & 0x3F) << 6) | (p[2] & 0x3F); return 3; }
    *code = ((uint32_t)(lead & 0x07) << 18) | ((uint32_t)(p[1] & 0x3F) << 12) | ((uint32_t)(p[2] & 0x3F) << 6) | (p[3] & 0x3F); return 4;
}
int main(int argc, char **argv) {
    (void) argv;
    static const unsigned char source[] = "caf\xC3\xA9 \xE7\x8C\xAB \xF0\x9F\x98\x80 <&>";
    long long limit = 499999 + argc, n = 0;
    unsigned char out[64];
    for (long long i = 0; i < limit; i++) {
        const unsigned char *cur = source; size_t w = 0;
        while (*cur) { uint32_t c; cur += utf8_decode(cur, &c);
            if (c >= 0x80) { out[w++]='&'; out[w++]='#'; unsigned char d[10]; size_t k=0; do { d[k++]=(unsigned char)('0'+c%10); c/=10; } while(c); while(k) out[w++]=d[--k]; out[w++]=';'; }
            else out[w++] = (unsigned char)c; }
        n += (long long)w + out[0];
    }
    printf("%lld\n", n); return 0;
}
