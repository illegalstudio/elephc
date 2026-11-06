#include <stdio.h>
#include <stdlib.h>

int main(int argc, char **argv) {
    (void) argv;
    static const char *texts[8] = {
        "123456789012345678", "  -12345678901234567", "42abc", "0x1A",
        "1_000", "007", "  -99  ", "999999999999999999"
    };
    long long result = 0;
    long long limit = 2999999 + argc;
    for (long long index = 0; index < limit; index++) {
        result = ((result << 5) ^ strtoll(texts[index & 7], NULL, 10)) & 0xFFFFFFFFLL;
    }
    printf("%lld\n", result);
    return 0;
}
