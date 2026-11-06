/* ABI and ownership model for the emitted append helper, with deterministic allocation failures. */
#include <inttypes.h>
#include <setjmp.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* Report model failures without creating a core dump on the developer's machine. */
static void fail_check(const char *expression, int line) {
    fprintf(stderr, "append model check at line %d: %s\n", line, expression);
    exit(1);
}
#define CHECK(expression) ((expression) ? (void)0 : fail_check(#expression, __LINE__))

typedef struct { uint32_t size, refs; uint64_t kind; } Header;
typedef struct { unsigned char *ptr; uint64_t len; } Pair;
_Static_assert(sizeof(Header) == 16, "runtime header layout");
static Header *blocks[64];
static size_t live, allocs, frees, fresh_calls, last_fresh_len;
static uint64_t bytes_at_entry, last_requested;
static int fail_next, boundary_active;
static jmp_buf boundary;
uint64_t copied_bytes __asm__("append_test_copy_bytes");
Pair append_under_test(const void *, uint64_t, const void *, uint64_t) __asm__("append_under_test");

/* Find registered payloads without probing literal or foreign memory. */
static Header *find(const void *ptr) {
    for (size_t i = 0; i < live; i++) if ((void *)(blocks[i] + 1) == ptr) return blocks[i];
    return NULL;
}
#if defined(__aarch64__)
#define KIND_SYMBOL "__rt_heap_kind"
#define ALLOC_SYMBOL "__rt_heap_alloc"
#define DEC_SYMBOL "__rt_decref_any"
#else
#define KIND_SYMBOL "append_test_kind"
#define ALLOC_SYMBOL "append_test_alloc"
#define DEC_SYMBOL "append_test_decref"
#endif
uint64_t test_kind(const void *) __asm__(KIND_SYMBOL);
void *test_alloc(uint64_t) __asm__(ALLOC_SYMBOL);
void test_decref(void *) __asm__(DEC_SYMBOL);
void test_fresh(const void *, uint64_t) __asm__("append_test_fresh");
__attribute__((noreturn)) void test_overflow(void) __asm__("append_test_overflow");

uint64_t test_kind(const void *ptr) {
    Header *header = find(ptr);
    return header && header->refs ? header->kind & 255 : 0;
}
void *test_alloc(uint64_t size) {
    last_requested = size;
    if (fail_next) { fail_next = 0; CHECK(boundary_active); longjmp(boundary, 1); }
    size = (size < 8 ? 8 : size + 7) & ~(uint64_t)7;
    CHECK(size <= UINT32_MAX && live < 64);
    Header *header = calloc(1, sizeof(Header) + (size_t)size);
    CHECK(header);
    header->size = (uint32_t)size;
    header->refs = 1;
    blocks[live++] = header;
    allocs++;
    return header + 1;
}
void test_decref(void *ptr) {
    Header *header = find(ptr);
    CHECK(header && header->refs && (header->kind & 255) == 1);
    if (--header->refs) return;
    size_t i = 0;
    while (blocks[i] != header) i++;
    blocks[i] = blocks[--live];
    free(header);
    frees++;
}
void test_fresh(const void *ptr, uint64_t len) {
    CHECK(copied_bytes == bytes_at_entry);
    Header *header = find(ptr);
    CHECK(header && header->refs && header->size >= len && (header->kind & 255) == 1);
    last_fresh_len = (size_t)len;
    fresh_calls++;
}
void test_overflow(void) {
    CHECK(boundary_active);
    longjmp(boundary, 2);
}

/* Fresh metadata must be published before the first byte of every changed result. */
static Pair append(const void *left, uint64_t length, const void *right, uint64_t tail) {
    bytes_at_entry = copied_bytes;
    return append_under_test(left, length, right, tail);
}

static Pair owned(const void *bytes, size_t len, size_t capacity) {
    Pair result = {test_alloc(capacity), len};
    Header *header = find(result.ptr);
    header->kind = 1;
    memcpy(result.ptr, bytes, len);
    return result;
}
static void equal(Pair value, const void *bytes, size_t len) {
    CHECK(value.len == len && memcmp(value.ptr, bytes, len) == 0);
}
static void balanced(void) { CHECK(live == 0 && allocs == frees); }

static void ownership_cases(void) {
    Pair old = owned("abc", 3, 16);
    size_t before = allocs;
    copied_bytes = 0;
    Pair next = append(old.ptr, old.len, "d", 1);
    equal(next, "abcd", 4);
    CHECK(next.ptr == old.ptr && allocs == before && copied_bytes == 1 && last_fresh_len == 4);
    size_t fresh_before = fresh_calls;
    next = append(next.ptr, next.len, "", 0);
    CHECK(next.ptr == old.ptr && fresh_calls == fresh_before && allocs == before);
    test_decref(next.ptr);
    balanced();

    old = owned("abc", 3, 16);
    find(old.ptr)->refs = 2;
    copied_bytes = 0;
    next = append(old.ptr, old.len, "d", 1);
    equal(old, "abc", 3);
    equal(next, "abcd", 4);
    CHECK(next.ptr != old.ptr && find(old.ptr)->refs == 1 && copied_bytes == 4);
    test_decref(old.ptr);
    test_decref(next.ptr);
    balanced();

    old = owned("abcde", 5, 16);
    copied_bytes = 0;
    next = append(old.ptr, old.len, old.ptr + 1, 3);
    equal(next, "abcdebcd", 8);
    CHECK(copied_bytes == 8 && next.ptr != old.ptr);
    test_decref(next.ptr);
    balanced();

    old = owned("abc", 3, 16);
    next = append(old.ptr, old.len, old.ptr, old.len);
    equal(next, "abcabc", 6);
    test_decref(next.ptr);
    balanced();

    next = append("raw", 3, "!", 1);
    equal(next, "raw!", 4);
    test_decref(next.ptr);
    balanced();

    const unsigned char bytes[] = {'a', 0, 'b'};
    old = owned(bytes, 3, 8);
    next = append(old.ptr, old.len, bytes + 1, 2);
    const unsigned char expected[] = {'a', 0, 'b', 0, 'b'};
    equal(next, expected, 5);
    test_decref(next.ptr);
    balanced();
}

/* Retained snapshots must not make spare capacity double on every shared copy. */
static void shared_capacity_cases(void) {
    Pair history[20];
    Pair value = owned("a", 1, 8);
    unsigned char expected[21];
    memset(expected, 'x', sizeof(expected));
    expected[0] = 'a';
    for (size_t i = 0; i < 20; i++) {
        find(value.ptr)->refs++;
        history[i] = value;
        value = append(value.ptr, value.len, "x", 1);
        CHECK(find(value.ptr)->size <= 2 * value.len + 16);
    }
    equal(value, expected, 21);
    test_decref(value.ptr);
    for (size_t i = 0; i < 20; i++) {
        equal(history[i], expected, i + 1);
        test_decref(history[i].ptr);
    }
    balanced();
}

static void boundary_cases(void) {
    Pair old = owned("stay", 4, 8);
    size_t before = allocs;
    copied_bytes = 0;
    fail_next = 1;
    boundary_active = 1;
    if (setjmp(boundary) == 0) {
        (void)append(old.ptr, old.len, "+++++", 5);
        CHECK(!"allocation failure must unwind");
    }
    boundary_active = 0;
    equal(old, "stay", 4);
    CHECK(find(old.ptr)->refs == 1 && copied_bytes == 0 && allocs == before);
    test_decref(old.ptr);
    balanced();

    old = owned("a", 1, 8);
    boundary_active = 1;
    if (setjmp(boundary) == 0) {
        (void)append(old.ptr, UINT64_MAX, "x", 1);
        CHECK(!"length wrap must unwind");
    }
    boundary_active = 0;
    equal(old, "a", 1);
    CHECK(find(old.ptr)->refs == 1);
    test_decref(old.ptr);
    balanced();

    boundary_active = 1;
    if (setjmp(boundary) == 0) {
        (void)append("", 0, "x", UINT32_MAX);
        CHECK(!"header size overflow must unwind");
    }
    boundary_active = 0;
    balanced();

    old = owned("a", 1, 8);
    find(old.ptr)->size = UINT32_C(0x80000000);
    copied_bytes = 0;
    fail_next = 1;
    boundary_active = 1;
    if (setjmp(boundary) == 0) {
        (void)append(old.ptr, UINT64_C(0x80000000), (const void *)(uintptr_t)1, 1);
        CHECK(!"capacity-boundary allocation must unwind");
    }
    boundary_active = 0;
    CHECK(last_requested == UINT64_C(0x80000001) && copied_bytes == 0);
    CHECK(find(old.ptr)->refs == 1);
    test_decref(old.ptr);
    balanced();

    old = owned("a", 1, 8);
    find(old.ptr)->size = UINT32_MAX - 7;
    find(old.ptr)->refs = 2;
    Pair next = append(old.ptr, old.len, "b", 1);
    equal(next, "ab", 2);
    CHECK(find(next.ptr)->size == 16 && find(old.ptr)->refs == 1);
    test_decref(old.ptr);
    test_decref(next.ptr);
    balanced();
}

static void scaling(size_t n) {
    allocs = frees = fresh_calls = 0;
    copied_bytes = 0;
    Pair value = owned("", 0, 8);
    for (size_t i = 0; i < n; i++) value = append(value.ptr, value.len, "x", 1);
    CHECK(value.len == n && copied_bytes < 3 * n && allocs <= 20);
    for (size_t i = 0; i < n; i++) CHECK(value.ptr[i] == 'x');
    test_decref(value.ptr);
    balanced();
    printf("n=%zu copied=%" PRIu64 " allocs=%zu frees=%zu\n", n, copied_bytes, allocs, frees);
}
int main(void) {
    ownership_cases();
    shared_capacity_cases();
    boundary_cases();
    scaling(5000);
    scaling(10000);
    scaling(20000);
    return 0;
}
