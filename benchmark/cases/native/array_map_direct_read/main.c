#include <stdio.h>

int main(int argc, char **argv) {
    (void) argv;
    int values[100];
    for (int i = 0; i < 100; i++) {
        values[i] = i + 1;
    }
    long long n = 0;
    long long limit = 4999 + argc;
    for (long long i = 0; i < limit; i++) {
        n += values[10] * 3;
    }
    printf("%lld\n", n);
    return 0;
}
