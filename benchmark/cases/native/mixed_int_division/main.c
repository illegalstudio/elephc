#include <stdio.h>

int main(int argc, char **argv) {
    (void) argv;
    long long a = argc + 5;
    long long b = argc + 2;
    long long sum = 0;
    long long limit = 4999999 + argc;
    for (long long i = 0; i < limit; i++) {
        sum += a / b;
    }
    printf("%lld\n", sum);
    return 0;
}
