/* Independent native host for leased INI results and compiled PHP ownership checks. */
#include <stdint.h>
#include <stddef.h>
#include <stdlib.h>
#include <string.h>

struct argument { uint64_t kind; int64_t value; const unsigned char *bytes; uint64_t length; };
struct result { uint64_t kind; int64_t value; unsigned char *bytes; uint64_t length; void *diagnostics; uint64_t diagnostics_length; };
struct cell { uint64_t tag, lo, hi; };
typedef int32_t (*diagnostic_fn)(void *, uint32_t, const unsigned char *, uint64_t);
struct host { uint32_t version, size; void *context; diagnostic_fn diagnostic; };
extern int32_t elephc_mbstring_ini_v1(uint32_t, const struct argument *, uint64_t, const struct host *, struct result *);
extern int32_t elephc_mbstring_core_ini_v1(uint32_t, const struct argument *, uint64_t, const struct host *, struct result *);
extern void elephc_mbstring_release_v1(struct result *);
extern int32_t elephc_mbstring_ini_string_retain_v1(uint64_t);
extern int32_t elephc_mbstring_ini_string_release_v1(uint64_t);
extern uint64_t elephc_mbstring_native_string_lookup_v1(uint64_t, uint64_t);
extern struct cell *materialize(struct result *) __asm__("_ini_fixture_materialize");
extern struct cell *restore(const unsigned char *, uint64_t, uint64_t) __asm__("_ini_fixture_restore");
static uint64_t expected[128], expected_count;

/* Treat an unexpected diagnostic as a fixture failure without unwinding through the engine. */
static int32_t diagnostic(void *context, uint32_t level, const unsigned char *bytes, uint64_t length) {
    (void)context; (void)level; (void)bytes; (void)length;
    return 1;
}

/* Read unaligned wire words explicitly, retaining the contract's little-endian byte order. */
static uint64_t read_word(const unsigned char *bytes) {
    uint64_t value = 0;
    for (unsigned i = 0; i < 8; i++) value |= (uint64_t)bytes[i] << (i * 8);
    return value;
}

/* Write fixture framing without exporting C pointers into the graph format. */
static void word(unsigned char *bytes, size_t *at, uint64_t value) {
    for (unsigned i = 0; i < 8; i++) bytes[(*at)++] = (unsigned char)(value >> (i * 8));
}

/* Import one fresh source string whose lease remains owned by this wire result. */
static struct result fresh(const unsigned char *bytes, uint64_t length) {
    const struct host host = { 1, sizeof(struct host), NULL, diagnostic };
    const struct argument argument = { 2, 0, bytes, length };
    struct result result = {0};
    if (elephc_mbstring_ini_v1(5, &argument, 1, &host, &result) || result.kind != 13) exit(91);
    return result;
}

/* Rebuild an indexed root with distinct equal strings and a fresh empty string from borrowed framing. */
static struct cell *indexed(void) {
    struct result strings[] = { fresh((const unsigned char *)"same", 4), fresh((const unsigned char *)"same", 4),
        fresh((const unsigned char *)"", 0) };
    unsigned char bytes[256];
    size_t at = 0;
    word(bytes, &at, 1); word(bytes, &at, 2);
    word(bytes, &at, 0); word(bytes, &at, 3);
    for (uint64_t i = 0; i < 3; i++) {
        word(bytes, &at, 1); word(bytes, &at, i);
        word(bytes, &at, 2); word(bytes, &at, strings[i].length);
        memcpy(bytes + at, strings[i].bytes, strings[i].length); at += strings[i].length;
    }
    size_t prefix = at;
    for (uint64_t i = 0; i < 3; i++) {
        expected[i] = (uint64_t)strings[i].value;
        word(bytes, &at, 1); word(bytes, &at, i); word(bytes, &at, expected[i]);
    }
    expected_count = 3;
    struct cell *value = restore(bytes, at, prefix);
    for (unsigned i = 0; i < 3; i++) elephc_mbstring_release_v1(&strings[i]);
    if (!value) exit(92);
    return value;
}

/* Return a real engine result after the common materializer has consumed and cleared its wire owner. */
struct cell *fixture_result(int64_t mode) __asm__("_ini_fixture_result");
struct cell *fixture_result(int64_t mode) {
    expected_count = 0;
    if (mode == 6) return indexed();
    struct result result = {0};
    if (mode < 3) {
        static const unsigned char binary[] = { 'x', 0, 255 };
        result = mode == 0 ? fresh(binary, sizeof(binary)) : fresh((const unsigned char *)(mode == 1 ? "" : "a"), mode == 1 ? 0 : 1);
        expected[expected_count++] = (uint64_t)result.value;
    } else if (mode < 6) {
        const struct host host = { 1, sizeof(struct host), NULL, diagnostic };
        const struct argument argument = { 3, mode == 3 ? 0 : 1, NULL, 0 };
        int32_t status = mode < 5 ? elephc_mbstring_core_ini_v1(4, &argument, 1, &host, &result)
            : elephc_mbstring_ini_v1(4, &argument, 1, &host, &result);
        if (status || result.kind != 14) exit(93);
        for (uint64_t at = (uint64_t)result.value; at < result.length; at += 24) {
            if (expected_count == 128) exit(94);
            expected[expected_count++] = read_word(result.bytes + at + 16);
        }
    } else {
        result = fresh((const unsigned char *)"invalid identity", 16);
        elephc_mbstring_ini_string_release_v1((uint64_t)result.value);
        result.value = 0;
        if (materialize(&result) != NULL) exit(95);
        result = (struct result){ .kind = 8, .value = 1 };
    }
    struct cell *value = materialize(&result);
    const unsigned char *cleared = (const unsigned char *)&result;
    for (size_t i = 0; i < sizeof(result); i++) if (cleared[i]) exit(96);
    if (!value) exit(97);
    return value;
}

/* Compare native string ownership against the wire identity captured before result release. */
int64_t fixture_identity(const struct cell *value, int64_t index) __asm__("_ini_fixture_identity");
int64_t fixture_identity(const struct cell *value, int64_t index) {
    return value && value->tag == 1 && index >= 0 && (uint64_t)index < expected_count
        && elephc_mbstring_native_string_lookup_v1(value->lo, value->hi) == expected[index];
}

/* Final PHP release must retire registry leases even when the INI state still owns the raw string. */
int64_t fixture_retired(void) __asm__("_ini_fixture_retired");
int64_t fixture_retired(void) {
    for (uint64_t i = 0; i < expected_count; i++) {
        if (elephc_mbstring_ini_string_retain_v1(expected[i]) == 0) {
            elephc_mbstring_ini_string_release_v1(expected[i]);
            return 0;
        }
    }
    return 1;
}
