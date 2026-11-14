#include <stdio.h>

int main(int argc, char **argv) {
    (void) argv;
    static const int lengths[4] = {1, 2, 3, 4};
    long long n = 0;
    long long limit = 999999 + argc;
    for (long long i = 0; i < limit; i++) {
        n += lengths[i & 3];
    }
    printf("%lld\n", n);
    return 0;
}
