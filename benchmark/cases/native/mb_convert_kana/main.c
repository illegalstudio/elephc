#include <stdint.h>
#include <stdio.h>
int main(int argc, char **argv) {
    (void) argv;
    static const uint32_t full_width[9] = {0xff21,0xff22,0xff23,0xff11,0xff12,0xff13,0xff41,0xff42,0xff43};
    long long limit = 999999 + argc, n = 0;
    unsigned char converted[9];
    for (long long i = 0; i < limit; i++) { for (int j = 0; j < 9; j++) converted[j] = (unsigned char)(full_width[j] - 0xfee0); n += 9 + converted[0]; }
    printf("%lld\n", n); return 0;
}
