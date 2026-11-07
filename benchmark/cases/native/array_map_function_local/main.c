#include <stdio.h>

static double map_local(int argc) {
    int values[100];
    for (int i = 0; i < 100; i++) {
        values[i] = i + 1;
    }
    double sum = 0.0;
    long long limit = 4999 + argc;
    for (long long i = 0; i < limit; i++) {
        for (int j = 0; j < 100; j++) {
            sum += values[j] + 1.5;
        }
    }
    return sum;
}

int main(int argc, char **argv) {
    (void) argv;
    printf("%.0f\n", map_local(argc));
    return 0;
}
