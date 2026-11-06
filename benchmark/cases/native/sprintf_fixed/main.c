#include <stdio.h>

int main(int argc, char **argv) {
    (void) argv;
    char buffer[64];
    long long n = 0;
    long long limit = 499999 + argc;
    for (long long i = 0; i < limit; i++) {
        int length = snprintf(buffer, sizeof(buffer), "%lld:%s:%.2f", i, "x", 1.5);
        n += length + buffer[0];
    }
    printf("%lld\n", n);
    return 0;
}
