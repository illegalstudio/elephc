/* Execute the emitted leaf with source slices ending at an inaccessible guard page. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/resource.h>
#include <unistd.h>

/* Adapt the runtime's three-register result without adding a heap or string terminator. */
static int probe(const unsigned char *p, size_t len, int64_t *value, size_t *consumed) {
    uint64_t ok, payload;
    __asm__ volatile("call __rt_json_integer_token"
        : "=a"(ok), "=D"(payload), "+d"(len) : "a"(p)
        : "rcx", "rsi", "r8", "r9", "r10", "r11", "cc", "memory");
    memcpy(value, &payload, sizeof(payload));
    *consumed = len;
    return ok != 0;
}

/* A length-zero slice deliberately points at the protected page. */
static void check(unsigned char *end, const char *text, int expected, int64_t wanted) {
    size_t len = strlen(text), consumed = 0;
    unsigned char *start = end - len;
    memcpy(start, text, len);
    int64_t value = 0;
    int actual = probe(start, len, &value, &consumed);
    if (actual != expected || (expected && (value != wanted || consumed != len - 1))) {
        fprintf(stderr, "token '%s': success=%d value=%lld consumed=%zu\n",
            text, actual, (long long)value, consumed);
        exit(1);
    }
}

/* Deterministic random signed payloads complement the hand-picked grammar boundaries. */
int main(void) {
    struct rlimit no_core = {0, 0};
    setrlimit(RLIMIT_CORE, &no_core);
    size_t page = (size_t)sysconf(_SC_PAGESIZE);
    unsigned char *memory = mmap(NULL, page * 2, PROT_READ | PROT_WRITE,
        MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (memory == MAP_FAILED || mprotect(memory + page, page, PROT_NONE)) return 2;
    unsigned char *end = memory + page;
    check(end, "0]", 1, 0);
    check(end, "-0]", 1, 0);
    check(end, "9223372036854775807]", 1, INT64_MAX);
    check(end, "-9223372036854775808]", 1, INT64_MIN);
    check(end, "-9223372036854775808 \t\r\n]", 1, INT64_MIN);
    const char *invalid[] = {"", "-", "0", "1", "-]", "]", "null]", "true]", " 1]",
        "01]", "-01]", "+1]", "1.0]", "1e2]", "1E+2]", "1\v]", "1\f]",
        "9223372036854775808]", "-9223372036854775809]", "999999999999999999999]"};
    for (size_t i = 0; i < sizeof(invalid) / sizeof(invalid[0]); ++i)
        check(end, invalid[i], 0, 0);
    uint64_t state = UINT64_C(0x193760fb87910a7d);
    for (size_t i = 0; i < 5000; ++i) {
        state = state * UINT64_C(6364136223846793005) + UINT64_C(1442695040888963407);
        int64_t value;
        memcpy(&value, &state, sizeof(value));
        char text[64];
        snprintf(text, sizeof(text), "%lld]", (long long)value);
        check(end, text, 1, value);
    }
    munmap(memory, page * 2);
    puts("integer token bounds and payloads: ok");
    return 0;
}
