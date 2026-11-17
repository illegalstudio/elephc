#include <stdio.h>

int main(int argc, char **argv) {
    (void) argv;
    int values[100];
    for (int i = 0; i < 100; i++) {
        values[i] = i + 1;
    }
    double sum = 0.0;
    long long limit = 4999 + argc;
    for (long long i = 0; i < limit; i++) {
        for (int j = 0; j < 100; j++) {
            sum += values[j] * 1.5 + 2.5;
        }
    }
    printf("%.0f\n", sum);
    return 0;
}
