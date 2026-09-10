/* Real C ABI host: every returned buffer is released through its library. */
#include "libtext.h"
#include <pthread.h>
#include <stdio.h>
#include <string.h>

/* Checks a string return whose inputs and output addresses cross the C register boundary. */
static int check_combined(const char *encoding) {
    char *out = NULL;
    size_t length = 0;
    const char left[] = "Caf\xc3\xa9";
    const char right[] = "\0right";
    const char prefix[] = "CAF\xc3\x89\0right:";
    int32_t status = mb_combine(left, sizeof(left) - 1, 7, 1.5, 1,
        right, sizeof(right) - 1, 11, 13, 17, &out, &length);
    int valid = status == ELEPHC_STATUS_OK && elephc_last_status() == ELEPHC_STATUS_OK
        && length == sizeof(prefix) - 1 + strlen(encoding)
        && memcmp(out, prefix, sizeof(prefix) - 1) == 0
        && memcmp(out + sizeof(prefix) - 1, encoding, strlen(encoding)) == 0;
    elephc_free(out);
    return valid;
}

/* Checks the configured request value without retaining any library-owned result buffer. */
static int check_encoding(const char *encoding) {
    char *out = NULL;
    size_t length = 0;
    int32_t status = mb_read(&out, &length);
    int valid = status == ELEPHC_STATUS_OK && length == strlen(encoding)
        && memcmp(out, encoding, length) == 0;
    elephc_free(out);
    return valid;
}

/* Exercises a second thread with serialized native calls and its own initialized stack guard. */
static void *worker(void *unused) {
    (void)unused;
    if (elephc_init() != ELEPHC_STATUS_OK || !check_encoding("8bit")) return (void *)(uintptr_t)1;
    if (!mb_switch("ASCII", 5) || !check_encoding("ASCII")) return (void *)(uintptr_t)2;
    return NULL;
}

/* Runs each first-entry form in a new process, then checks idempotence after a PHP setting mutation. */
int main(int argc, char **argv) {
    const char text[] = "Caf\xc3\xa9";
    if (argc != 2) return 1;
    if (strcmp(argv[1], "init") == 0 && elephc_init() != ELEPHC_STATUS_OK) return 2;
    if (strcmp(argv[1], "string") == 0 && !check_combined("8bit")) return 3;
    if (mb_size(text, sizeof(text) - 1) != 5 || elephc_last_status() != ELEPHC_STATUS_OK) return 4;
    if (!check_encoding("8bit")) return 5;
    if (mb_weight(text, sizeof(text) - 1) != 5.5) return 13;
    if (!mb_switch("UTF-8", 5)) return 6;
    for (int i = 0; i < 8; i++) {
        if (elephc_init() != ELEPHC_STATUS_OK) return 7;
        if (mb_size(text, sizeof(text) - 1) != 4) return 8;
        if (!check_encoding("UTF-8") || !check_combined("UTF-8")) return 9;
    }
    pthread_t thread;
    void *result = NULL;
    if (pthread_create(&thread, NULL, worker, NULL) != 0) return 10;
    if (pthread_join(thread, &result) != 0 || result != NULL) return 11;
    if (elephc_init() != ELEPHC_STATUS_OK || !check_encoding("UTF-8")) return 12;
    elephc_shutdown();
    puts("configured:5:4:preserved");
    return 0;
}
