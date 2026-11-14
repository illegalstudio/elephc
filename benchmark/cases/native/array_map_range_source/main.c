#include <stdio.h>

int main(int argc, char **argv) {
    (void) argv;
    double sum = 0.0;
    long long limit = 4999 + argc;
    for (long long i = 0; i < limit; i++) {
        for (int j = 1; j <= 100; j++) {
            sum += j + 1.5;
        }
    }
    printf("%.0f\n", sum);
    return 0;
}
