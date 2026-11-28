#include <stdio.h>

int main(int argc, char **argv) {
    (void) argv;
    long long n = 0;
    long long limit = 4999999 + argc;
    for (long long i = 0; i < limit; i++) {
        n += 42;
    }
    printf("%lld\n", n);
    return 0;
}
