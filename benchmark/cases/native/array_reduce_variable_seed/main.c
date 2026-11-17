#include <stdio.h>

int main(int argc, char **argv) {
    (void) argv;
    int values[100];
    for (int i = 0; i < 100; i++) {
        values[i] = i + 1;
    }
    long long seed = 10;
    long long sum = 0;
    long long limit = 4999 + argc;
    for (long long i = 0; i < limit; i++) {
        long long inner = seed;
        for (int j = 0; j < 100; j++) {
            inner += values[j];
        }
        sum += inner;
    }
    printf("%lld\n", sum);
    return 0;
}
