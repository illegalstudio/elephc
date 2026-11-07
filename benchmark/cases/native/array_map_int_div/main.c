#include <stdio.h>

int main(int argc, char **argv) {
    (void) argv;
    long long sum = 0;
    long long limit = 4999 + argc;
    for (long long i = 0; i < limit; i++) {
        for (int j = 2; j <= 200; j += 2) {
            sum += j / 2;
        }
    }
    printf("%lld\n", sum);
    return 0;
}
