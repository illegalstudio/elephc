#include <stdio.h>
#include <stdlib.h>

int main(void) {
    int *data = malloc(sizeof(int) * 50000);
    long long sum = 0;

    if (data == NULL) {
        return 1;
    }

    for (int i = 0; i < 50000; ++i) {
        data[i] = i % 97;
    }

    for (int i = 0; i < 50000; ++i) {
        sum += data[i];
    }

    printf("%lld\n", sum);
    free(data);
    return 0;
}
