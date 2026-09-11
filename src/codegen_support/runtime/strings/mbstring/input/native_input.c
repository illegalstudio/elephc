/* Independent native-value layouts and eval metadata stubs for the mbstring input ABI. */
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#define HIDDEN __attribute__((visibility("hidden")))
struct cell { uint64_t tag, lo, hi; };
struct input { uint64_t kind, value; const unsigned char *bytes; uint64_t len, flags; };
struct result { uint64_t kind; int64_t value; unsigned char *bytes; uint64_t len; unsigned char *diagnostics; uint64_t diagnostics_len; };
struct eval_result { uint32_t kind; struct cell *value, *error; };
struct name { const unsigned char *bytes; uint64_t len; };
typedef void (*prepare_fn)(uint32_t, uint32_t, const struct input *, uint32_t, struct result *);
typedef void (*release_fn)(struct result *);
extern int describe(void *, const struct cell *, struct input *, void **) __asm__("__rt_mbstring_input");
HIDDEN uint64_t class_count __asm__("_class_name_count") = 3;
HIDDEN struct name class_names[] __asm__("_class_name_entries") = {{(const unsigned char *)"Plain", 5}, {(const unsigned char *)"Text\0\xff", 6}, {0, 0}};
HIDDEN uint64_t tostring_count __asm__("_class_tostring_count") = 2;
HIDDEN uintptr_t tostring_methods[] __asm__("_class_tostring_ptrs") = {0, 1};
HIDDEN const unsigned char closure_name[] __asm__("_sprintf_closure_class_name") = "Closure";
HIDDEN const unsigned char tostring_name[] __asm__("_mbstring_tostring_name") = "__toString";
HIDDEN void *fixture_catalog __asm__("_mbstring_catalog_array") = 0;
static int context_marker, live, name_calls, method_calls, failure;
static const unsigned char dynamic_name[] = "Dynamic\0\xff";

/* Allocates a fake native metadata cell, keeping ownership counts independent of the emitter. */
static struct cell *allocate(uint64_t tag) {
    struct cell *value = calloc(1, sizeof(*value));
    if (!value) { abort(); }
    value->tag = tag;
    if (tag == 1) { value->lo = (uintptr_t)dynamic_name; value->hi = sizeof(dynamic_name) - 1; }
    live++;
    return value;
}
/* Releases only metadata allocated by this fixture, never borrowed source cells or class tables. */
void mb_test_release(struct cell *value) {
    if (value) { live--; free(value); }
}

/* Resolves the original nested object descriptor without running any user conversion. */
static int64_t identity(const struct cell *value) {
    while (value->tag == 7) { value = (const struct cell *)(uintptr_t)value->lo; }
    return *(const int64_t *)(uintptr_t)value->lo;
}

/* Models optional ownership lookup, including native class-table holes backed by eval metadata. */
void *__elephc_eval_object_context(void *context, const struct cell *value) {
    int64_t id = identity(value);
    return id < 0 || id == 2 ? context : NULL;
}

/* Models existing eval class introspection, including owned malformed and exceptional responses. */
int __elephc_eval_object_class_name(void *context, const struct cell *value, uint64_t lookup, struct eval_result *out) {
    name_calls++;
    if (context != &context_marker || lookup || !out || out->value || out->error) { failure = __LINE__; return 1; }
    int64_t id = identity(value);
    if (id == -3) { out->value = allocate(1); out->error = allocate(8); return 1; }
    if (id == -4) { out->value = allocate(0); return 0; }
    if (id == -5) { return 0; }
    out->value = allocate(1);
    return 0;
}

/* Verifies borrowed method-name framing and observes capability without invoking the method. */
int __elephc_eval_member_exists(void *context, const struct cell *value, const struct cell *member, uint64_t lookup) {
    method_calls++;
    if (context != &context_marker || lookup || live != 1 || member->tag != 1 || member->hi != 10 || memcmp((const void *)(uintptr_t)member->lo, "__toString", 10)) { failure = __LINE__; return 0; }
    return identity(value) != -2;
}

/* Checks a complete descriptor, preservation of its borrowed source, and real shared preparation. */
static int check(const struct cell *source, uint64_t kind, uint64_t payload, const unsigned char *bytes, uint64_t len,
                 uint64_t flags, int dynamic, uint32_t op, prepare_fn prepare, release_fn release) {
    struct cell original = source ? *source : (struct cell){0};
    struct input input;
    memset(&input, 0xa5, sizeof(input));
    void *owner = (void *)(uintptr_t)42;
    int before_names = name_calls;
    int before_methods = method_calls;
    if (describe(&context_marker, source, &input, &owner)) { return __LINE__; }
    if (input.kind != kind || input.value != payload || input.len != len || input.flags != flags) { return __LINE__; }
    if (len ? (!input.bytes || memcmp(input.bytes, bytes, len)) : input.bytes != bytes) { return __LINE__; }
    if ((owner != NULL) != dynamic || live != dynamic) { return __LINE__; }
    if (source && memcmp(source, &original, sizeof(original))) { return __LINE__; }
    if (name_calls - before_names != dynamic || method_calls - before_methods != dynamic) { return __LINE__; }
    struct result output = {0};
    prepare(op, 0, &input, 1, &output);
    if (kind == 1) {
        if (output.kind != 256 || output.bytes || output.len) { return __LINE__; }
    } else if (kind == 6) {
        const char prefix[] = "mb_strlen(): Argument #1 ($string) must be of type string, ";
        const char suffix[] = " given";
        if (output.kind != 4 || output.len != sizeof(prefix) - 1 + len + sizeof(suffix) - 1) { return __LINE__; }
        if (memcmp(output.bytes, prefix, sizeof(prefix) - 1) || memcmp(output.bytes + sizeof(prefix) - 1, bytes, len)) { return __LINE__; }
        if (memcmp(output.bytes + sizeof(prefix) - 1 + len, suffix, sizeof(suffix) - 1)) { return __LINE__; }
    } else if (output.kind != 4) { return __LINE__; }
    release(&output);
    if (kind == 6) {
        prepare(op, 0, &input, 0, &output);
        if (output.kind != (flags ? 259 : 4)) { return __LINE__; }
        release(&output);
    }
    mb_test_release(owner);
    return failure;
}

/* Exercises concrete kinds, nested Mixed, binary class metadata, synthetic classes, and failures. */
int mb_test_input(uint32_t op, prepare_fn prepare, release_fn release) {
    struct cell sources[] = {{0, UINT64_C(0x7ffffffffffffffe), 0}, {0, UINT64_C(0x8000000000000000), 0},
        {2, UINT64_C(0x7ff8000000000042), 0}, {3, 1, 0}, {4, 1, 0}, {5, 1, 0}, {8, 123, 456}, {9, 42, 0}};
    for (unsigned i = 0; i < sizeof(sources) / sizeof(*sources); i++) {
        int result = check(&sources[i], sources[i].tag, sources[i].tag == 8 ? 0 : sources[i].lo, NULL, 0, 0, 0, op, prepare, release);
        if (result) { return result; }
    }
    fixture_catalog = (void *)(uintptr_t)1;
    struct cell catalog = {4, 1, 0}, different_array = {4, 2, 0};
    int result = check(&catalog, 4, 1, NULL, 0, 1, 0, op, prepare, release);
    if (result) { return result; }
    result = check(&different_array, 4, 2, NULL, 0, 0, 0, op, prepare, release);
    if (result) { return result; }
    fixture_catalog = NULL;
    result = check(NULL, 8, 0, NULL, 0, 0, 0, op, prepare, release);
    if (result) { return result; }
    const unsigned char binary[] = "a\0\xff";
    struct cell string = {1, (uintptr_t)binary, 3};
    struct cell nested = {7, (uintptr_t)&string, 0};
    result = check(&nested, 1, 0, binary, 3, 0, 0, op, prepare, release);
    if (result) { return result; }
    for (uint64_t tag = 4; tag <= 6; tag++) {
        for (int sentinel = 0; sentinel < 2; sentinel++) {
            struct cell empty = {tag, sentinel ? UINT64_C(0x7ffffffffffffffe) : 0, 19};
            result = check(&empty, 8, 0, NULL, 0, 0, 0, op, prepare, release);
            if (result) { return result; }
        }
    }
    struct cell callable = {10, 77, 0};
    result = check(&callable, 6, 77, closure_name, 7, 0, 0, op, prepare, release);
    if (result) { return result; }
    for (int64_t id = -2; id <= 2; id++) {
        struct cell object = {6, (uintptr_t)&id, 0};
        struct cell wrapped = {7, (uintptr_t)&object, 0};
        int dynamic = id < 0 || id == 2;
        const unsigned char *name = dynamic ? dynamic_name : class_names[id].bytes;
        uint64_t len = dynamic ? sizeof(dynamic_name) - 1 : class_names[id].len;
        result = check(&wrapped, 6, (uintptr_t)&id, name, len, id == 1 || id == -1 || id == 2, dynamic, op, prepare, release);
        if (result) { return result; }
    }
    for (int64_t id = -5; id <= -3; id++) {
        struct cell object = {6, (uintptr_t)&id, 0};
        struct input input;
        void *owner = NULL;
        if (describe(&context_marker, &object, &input, &owner) != 1 || owner || live) { return __LINE__; }
        unsigned char zero[sizeof(input)] = {0};
        if (memcmp(&input, zero, sizeof(input))) { return __LINE__; }
    }
    struct cell unknown = {99, 0, 0};
    struct input input;
    void *owner = NULL;
    if (describe(NULL, &unknown, &input, &owner) != 1 || owner || live) { return __LINE__; }
    int64_t dynamic_id = -1;
    struct cell dynamic = {6, (uintptr_t)&dynamic_id, 0};
    if (describe(NULL, &dynamic, &input, &owner) != 1 || owner || live) { return __LINE__; }
    if (describe(NULL, NULL, NULL, &owner) != 1 || describe(NULL, NULL, &input, NULL) != 1) { return __LINE__; }
    return failure;
}
