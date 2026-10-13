#include <stdio.h>

int main(void) {
    long long sum = 0;
    for (long long i = 1; i <= 200000; ++i) {
        sum += i;
    }
    printf("%lld\n", sum);
    return 0;
}
