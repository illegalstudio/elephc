/* Native array layout fixtures for the mbstring host reader. No runtime heap allocation is used. */
#include <stdint.h>
#include <stddef.h>
#include <string.h>

typedef struct { uint64_t tag, lo, hi; } Value;
typedef struct { uint64_t kind, length, capacity, width, data[24]; } Indexed;
typedef struct { uint64_t occupied, key_lo, key_hi, value_lo, value_hi, tag, prev, next; } Entry;
typedef struct { uint64_t count, capacity, value_type, head, tail; Entry *entries; uint64_t pins; int64_t next_index; } Hash;
extern uint64_t native_next(void *, const Value *, uint64_t *, Value *, Value *) __asm__("__rt_mbstring_array_next");
#define PTR(value) ((uint64_t)(uintptr_t)(value))
#define CHECK(condition) do { if (!(condition)) { return __LINE__; } } while (0)

_Static_assert(sizeof(Hash) == 64, "hash header ABI must remain 64 bytes");
_Static_assert(offsetof(Hash, entries) == 40, "hash entry-storage pointer must remain at byte 40");

static const unsigned char text[] = {'a', 0, 255};
static const unsigned char binary_key[] = {'k', 0};
static Indexed graph_indexed;
static Hash graph_hash;
static Entry graph_hash_entries[4];
static Value graph_values[6], wrapped, graph_root;

/* Keep every borrowed graph descriptor alive after the fixture initializer returns. */
static void initialize_graph(void) {
    graph_root = (Value){4, PTR(&graph_indexed.length), 0};
    graph_indexed = (Indexed){.kind = 0x8702, .length = 6, .capacity = 6, .width = 8};
    wrapped = (Value){0, UINT64_C(0x8000000000000000), 0};
    graph_values[0] = (Value){1, PTR(text), sizeof(text)};
    graph_values[1] = (Value){7, PTR(&wrapped), 0};
    graph_values[2] = (Value){5, PTR(&graph_hash), 0};
    graph_values[3] = graph_values[2];
    graph_values[4] = graph_root;
    graph_values[5] = (Value){6, 1234, 0};
    for (size_t i = 0; i < 6; ++i) { graph_indexed.data[i] = PTR(&graph_values[i]); }
    memset(graph_hash_entries, 0, sizeof(graph_hash_entries));
    graph_hash = (Hash){.count = 3, .capacity = 4, .value_type = 7, .head = 3, .tail = 2,
        .entries = graph_hash_entries, .pins = 0, .next_index = INT64_MIN};
    graph_hash_entries[3] = (Entry){1, PTR("42"), 2, UINT64_C(0x7ff8000000000042), 0, 2, UINT64_MAX, 0};
    graph_hash_entries[0] = (Entry){1, 42, UINT64_MAX, 0, 0, 4, 3, 2};
    graph_hash_entries[2] = (Entry){1, PTR(binary_key), 2, PTR(&graph_indexed.length), 0, 4, 0, UINT64_MAX};
}

/* Compare returned descriptors and prove the reader never rewrites typed source slots. */
static int check_indexed(uint64_t tag, uint64_t width, const uint64_t *words, size_t count, const Value *expected) {
    Indexed input = {.kind = 0x8002 | (tag << 8), .length = count, .capacity = count, .width = width};
    memcpy(input.data, words, count * width);
    Indexed original = input;
    Value root = {4, PTR(&input.length), 0}, key, value;
    uint64_t cursor = 0;
    for (size_t i = 0; i < count; ++i) {
        CHECK(native_next(NULL, &root, &cursor, &key, &value) == 1);
        CHECK(key.tag == 0 && key.lo == i && key.hi == 0);
        CHECK(value.tag == expected[i].tag && value.lo == expected[i].lo && value.hi == expected[i].hi);
    }
    CHECK(native_next(NULL, &root, &cursor, &key, &value) == 0);
    CHECK(memcmp(&input, &original, sizeof(input)) == 0);
    return 0;
}

/* Exercise all slot layouts, Mixed wrappers, unsupported leaves, and sparse ordered hashes. */
int mb_test_reader(void) {
    initialize_graph();
    uint64_t integers[] = {0, UINT64_MAX, UINT64_C(0x8000000000000000)};
    Value integer_values[] = {{0, 0, 0}, {0, UINT64_MAX, 0}, {0, UINT64_C(0x8000000000000000), 0}};
    CHECK(check_indexed(0, 8, integers, 3, integer_values) == 0);
    uint64_t strings[] = {PTR(text), sizeof(text), 0, 0};
    Value string_values[] = {{1, PTR(text), sizeof(text)}, {1, 0, 0}};
    CHECK(check_indexed(1, 16, strings, 2, string_values) == 0);
    uint64_t floats[] = {UINT64_C(0x8000000000000000), UINT64_C(0x7ff8000000000042)};
    Value float_values[] = {{2, floats[0], 0}, {2, floats[1], 0}};
    CHECK(check_indexed(2, 8, floats, 2, float_values) == 0);
    uint64_t booleans[] = {0, 1};
    Value boolean_values[] = {{3, 0, 0}, {3, 1, 0}};
    CHECK(check_indexed(3, 8, booleans, 2, boolean_values) == 0);
    uint64_t tagged[] = {UINT64_C(0x8000000000000000), 0, 1, 3, 0, 8};
    Value tagged_values[] = {{0, tagged[0], 0}, {3, 1, 0}, {8, 0, 0}};
    CHECK(check_indexed(11, 16, tagged, 3, tagged_values) == 0);
    Value resource = {9, 12, 0}, sentinel = {5, UINT64_C(0x7ffffffffffffffe), 0};
    uint64_t mixed[] = {PTR(&graph_values[0]), PTR(&graph_values[1]), PTR(&graph_values[4]), PTR(&graph_values[5]), PTR(&resource), PTR(&sentinel)};
    Value mixed_values[] = {graph_values[0], wrapped, graph_root, {6, 0, 0}, {6, 0, 0}, {8, 0, 0}};
    CHECK(check_indexed(7, 8, mixed, 6, mixed_values) == 0);
    uint64_t arrays[] = {graph_root.lo, 0, UINT64_C(0x7ffffffffffffffe)};
    Value array_values[] = {graph_root, {8, 0, 0}, {8, 0, 0}};
    CHECK(check_indexed(4, 8, arrays, 3, array_values) == 0);
    Hash original = graph_hash;
    Entry original_entries[4];
    memcpy(original_entries, graph_hash_entries, sizeof(original_entries));
    Value key, value, root = {5, PTR(&graph_hash), 0};
    uint64_t cursor = 0;
    CHECK(native_next(NULL, &root, &cursor, &key, &value) == 1);
    CHECK(key.tag == 1 && key.hi == 2 && memcmp((void *)(uintptr_t)key.lo, "42", 2) == 0);
    CHECK(value.tag == 2 && value.lo == UINT64_C(0x7ff8000000000042));
    CHECK(native_next(NULL, &root, &cursor, &key, &value) == 1);
    CHECK(key.tag == 0 && key.lo == 42 && key.hi == 0 && value.tag == 8);
    CHECK(native_next(NULL, &root, &cursor, &key, &value) == 1);
    CHECK(key.tag == 1 && key.hi == 2 && memcmp((void *)(uintptr_t)key.lo, binary_key, 2) == 0);
    CHECK(value.tag == 4 && value.lo == graph_root.lo);
    CHECK(native_next(NULL, &root, &cursor, &key, &value) == 0);
    CHECK(memcmp(&original, &graph_hash, sizeof(original)) == 0);
    CHECK(memcmp(original_entries, graph_hash_entries, sizeof(original_entries)) == 0);
    root = (Value){4, 0, 0}; cursor = 0;
    CHECK(native_next(NULL, &root, &cursor, &key, &value) == 2);
    root = (Value){1, graph_root.lo, 0};
    CHECK(native_next(NULL, &root, &cursor, &key, &value) == 2);
    Indexed invalid = {.kind = 0x8002, .length = 1, .capacity = 1, .width = 16};
    root = (Value){4, PTR(&invalid.length), 0};
    CHECK(native_next(NULL, &root, &cursor, &key, &value) == 2);
    return 0;
}

/* Export ordinary C symbols for dynamic lookup on ELF and Mach-O hosts. */
const Value *mb_test_root(void) { initialize_graph(); return &graph_root; }
uint64_t mb_test_next(void *context, const Value *array, uint64_t *cursor, Value *key, Value *value) {
    return native_next(context, array, cursor, key, value);
}
