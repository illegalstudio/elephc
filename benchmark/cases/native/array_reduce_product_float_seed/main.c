#include <stdio.h>

int main(int argc, char **argv) {
    (void) argv;
    int values[10];
    for (int i = 0; i < 10; i++) {
        values[i] = i + 1;
    }
    double sum = 0.0;
    long long limit = 4999 + argc;
    for (long long i = 0; i < limit; i++) {
        double inner = 1.5;
        for (int j = 0; j < 10; j++) {
            inner *= values[j];
        }
        sum += inner;
    }
    printf("%.0f\n", sum);
    return 0;
}
