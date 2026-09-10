/* Independent native heap and lease checks for emitted INI persistence and final-release hooks. */
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#define HIDDEN __attribute__((visibility("hidden")))
#define CHECK(value) do { if (!(value)) { return __LINE__; } } while (0)
struct arg { uint64_t kind; int64_t value; const unsigned char *bytes; uint64_t len; };
struct result { uint64_t kind; int64_t value; unsigned char *bytes; uint64_t len; unsigned char *diagnostics; uint64_t diagnostics_len; };
struct host { uint32_t version, size; void *context; int32_t (*diagnostic)(void *, uint32_t, const unsigned char *, uint64_t); };
struct cell { uint64_t tag; void *bytes; uint64_t length; };
typedef int32_t (*ini_fn)(uint32_t, const struct arg *, uint64_t, const struct host *, struct result *);
typedef void (*result_free_fn)(struct result *);
typedef int32_t (*one_fn)(uint64_t);
typedef int32_t (*three_fn)(uint64_t, uint64_t, uint64_t);
typedef uint64_t (*lookup_fn)(uint64_t, uint64_t);
typedef int32_t (*two_fn)(uint64_t, uint64_t);
typedef int32_t (*reset_fn)(void);
static void *const *functions;
static uint64_t copies, frees;

HIDDEN _Alignas(16) unsigned char heap[16384] __asm__("_heap_buf");
HIDDEN uint64_t offset __asm__("_heap_off");
HIDDEN uint64_t live __asm__("_gc_live");
HIDDEN uint64_t free_count __asm__("_gc_frees");
HIDDEN uint64_t debug __asm__("_heap_debug_enabled");
HIDDEN uint64_t guard __asm__("_web_heap_guard_enabled");
HIDDEN void *free_list __asm__("_heap_free_list");
HIDDEN void *bins[4] __asm__("_heap_small_bins");
HIDDEN uint64_t active __asm__("_mbstring_ini_native_active");
HIDDEN _Alignas(16) unsigned char snapshots[2][672] __asm__("_ini_register_snapshots");
HIDDEN const char double_free[] __asm__("_heap_dbg_double_free_msg") = "unexpected double free";
extern void mb_test_registers(void *, uint64_t, uint64_t);
extern void mb_test_bind(void *, uint64_t, uint64_t);
extern void mb_test_literal(const void *, uint64_t);
extern void *mb_test_persist(const void *, uint64_t);
extern void *mb_test_lower(const void *, uint64_t);
extern void *mb_test_upper(const void *, uint64_t);
extern void mb_test_free(void *);
extern void mb_test_reset(void);
extern struct cell *__elephc_eval_value_string_literal(const void *, uint64_t);

/* Allocates from an independent bounded bump arena with the real runtime's uniform header. */
void *mb_test_allocate(uint64_t size) {
    size = (size + 7) & ~UINT64_C(7);
    if (size < 8) { size = 8; }
    if (size + 16 > sizeof(heap) - offset) { abort(); }
    unsigned char *header = heap + offset;
    *(uint32_t *)header = (uint32_t)size;
    *(uint32_t *)(header + 4) = 1;
#if defined(__x86_64__)
    *(uint64_t *)(header + 8) = UINT64_C(0x454c504800000000);
#else
    *(uint64_t *)(header + 8) = 0;
#endif
    offset += size + 16;
    live += size + 16;
    return header + 16;
}

/* Supplies the ordinary eval constructor contract while retaining real native string persistence. */
struct cell *__elephc_eval_value_string(const void *bytes, uint64_t length) {
    void *payload = mb_test_persist(bytes, length);
    struct cell *cell = mb_test_allocate(sizeof(*cell));
    cell->tag = 1;
    cell->bytes = payload;
    cell->length = length;
    return cell;
}

/* Forwards emitted calls to the actual Rust identity registry supplied by the integration harness. */
int32_t elephc_mbstring_native_string_bind_v1(uint64_t owner, uint64_t length, uint64_t identity) {
    int32_t result = ((three_fn)functions[4])(owner, length, identity);
#if defined(__x86_64__)
    __asm__ volatile("pxor %%xmm0, %%xmm0; pxor %%xmm15, %%xmm15; xor %%r8d, %%r8d; xor %%r11d, %%r11d"
        : : : "xmm0", "xmm15", "r8", "r11", "cc");
#else
    __asm__ volatile("movi v0.16b, #0; movi v31.16b, #0; mov x3, #0; mov x17, #0"
        : : : "v0", "v31", "x3", "x17");
#endif
    return result;
}

/* Counts complete persistence copies while preserving the real registry's ownership behavior. */
int32_t elephc_mbstring_native_string_copy_v1(uint64_t destination, uint64_t source, uint64_t length) {
    copies++;
    return ((three_fn)functions[5])(destination, source, length);
}

/* Records origins without reading foreign or scratch storage into the engine. */
int32_t elephc_mbstring_native_string_fresh_v1(uint64_t owner, uint64_t length) {
    return ((two_fn)functions[9])(owner, length);
}

/* Distinguishes explicit PHP literals from other non-heap input buffers. */
int32_t elephc_mbstring_native_string_literal_v1(uint64_t owner, uint64_t length) {
    return ((two_fn)functions[10])(owner, length);
}

/* Tracks ordinary persistence before the first INI call without retaining its source allocation. */
int32_t elephc_mbstring_native_string_persist_v1(uint64_t destination, uint64_t source, uint64_t length) {
    copies++;
    return ((three_fn)functions[11])(destination, source, length);
}

/* Observes metadata retirement before the actual allocator makes a block reusable. */
int32_t elephc_mbstring_native_string_forget_v1(uint64_t owner) {
    frees++;
    return ((one_fn)functions[7])(owner);
}

/* Clears only native metadata leases; external result ownership remains independent. */
int32_t elephc_mbstring_native_string_reset_v1(void) { return ((reset_fn)functions[8])(); }

/* Provides the complete protected host table for diagnostic-free string imports. */
static int32_t diagnostic(void *context, uint32_t level, const unsigned char *bytes, uint64_t length) {
    (void)context; (void)level; (void)bytes; (void)length;
    return 0;
}

/* Imports fresh text into one ordinary result-owned identity lease. */
static struct result import(const unsigned char *bytes, uint64_t length) {
    struct arg argument = {2, 0, bytes, length};
    struct host host = {1, sizeof(host), NULL, diagnostic};
    struct result result = {0};
    if (((ini_fn)functions[0])(5, &argument, 1, &host, &result) != 0 || result.kind != 13) { abort(); }
    return result;
}

/* Runs real assembly persistence, last-owner retirement, address reuse, and dormant request reset. */
int mb_test_native(void *const *callbacks) {
    functions = callbacks;
    CHECK(((reset_fn)functions[8])() == 0);
    void *source = mb_test_persist("ASCII", 5);
    CHECK(copies == 1 && frees == 0 && active == 1);
    copies = 0;
    struct result result = import((const unsigned char *)"ASCII", 5);
    uint64_t identity = (uint64_t)result.value;
    mb_test_registers(source, 5, identity);
    CHECK(memcmp(snapshots[0], snapshots[1], 672) == 0);
    CHECK(active == 1);
    ((result_free_fn)functions[1])(&result);
    void *copy = mb_test_persist(source, 5);
    CHECK(copy != source && memcmp(copy, "ASCII", 5) == 0 && copies == 1);
    CHECK(((lookup_fn)functions[6])((uintptr_t)copy, 5) == identity);
    mb_test_free(copy);
    CHECK(frees == 1 && ((lookup_fn)functions[6])((uintptr_t)copy, 5) == 0);
    void *replacement = mb_test_persist("SJIS", 4);
    CHECK(replacement == copy && ((lookup_fn)functions[6])((uintptr_t)replacement, 4) == 0);
    mb_test_free(replacement);
    mb_test_free(source);
    CHECK(live == 0 && offset == 0 && ((one_fn)functions[2])(identity) == 1);
    source = mb_test_persist("ASCII", 5);
    result = import((const unsigned char *)"ASCII", 5);
    identity = (uint64_t)result.value;
    mb_test_bind(source, 5, identity);
    CHECK(((one_fn)functions[2])(identity) == 0);
    ((result_free_fn)functions[1])(&result);
    mb_test_reset();
    CHECK(active == 0 && ((lookup_fn)functions[6])((uintptr_t)source, 5) == 0);
    uint64_t calls = frees;
    mb_test_free(source);
    CHECK(frees == calls && live == 0 && offset == 0);
    CHECK(((one_fn)functions[3])(identity) == 0 && ((one_fn)functions[2])(identity) == 1);
    static const char literal[] = "ASCII";
    mb_test_literal(literal, 5);
    source = mb_test_persist(literal, 5);
    void *early = mb_test_persist(literal, 5);
    identity = ((lookup_fn)functions[12])((uintptr_t)source, 5);
    CHECK(identity != 0 && ((lookup_fn)functions[12])((uintptr_t)early, 5) == identity);
    mb_test_free(early);
    mb_test_free(source);
    mb_test_reset();
    CHECK(live == 0 && offset == 0 && ((one_fn)functions[2])(identity) == 1);
    char input[] = "ASCII";
    struct cell *first = __elephc_eval_value_string_literal(input, 5);
    struct cell *second = __elephc_eval_value_string_literal(input, 5);
    memset(input, 'X', 5);
    identity = ((lookup_fn)functions[12])((uintptr_t)first->bytes, first->length);
    CHECK(identity != 0 && memcmp(first->bytes, literal, 5) == 0);
    CHECK(((lookup_fn)functions[12])((uintptr_t)second->bytes, second->length) == identity);
    source = second->bytes;
    mb_test_free(second);
    mb_test_free(source);
    source = first->bytes;
    mb_test_free(first);
    mb_test_free(source);
    CHECK(live == 0 && offset == 0 && ((one_fn)functions[2])(identity) == 1);
    source = mb_test_allocate(5);
    memcpy(source, "ASCII", 5);
#if defined(__x86_64__)
    ((uint64_t *)source)[-1] = UINT64_C(0x454c504800000007);
#else
    ((uint64_t *)source)[-1] = 7;
#endif
    CHECK(mb_test_persist(source, 5) == source);
    CHECK(((lookup_fn)functions[6])((uintptr_t)source, 5) == 0);
    identity = ((lookup_fn)functions[12])((uintptr_t)source, 5);
    CHECK(identity != 0);
    mb_test_free(source);
    CHECK(live == 0 && offset == 0 && ((one_fn)functions[2])(identity) == 1);
    source = mb_test_persist("ASCII", 5);
    identity = ((lookup_fn)functions[12])((uintptr_t)source, 5);
    void *unchanged = mb_test_upper(source, 5);
    void *changed = mb_test_lower(source, 5);
    CHECK(unchanged != source && memcmp(unchanged, "ASCII", 5) == 0);
    CHECK(changed != source && memcmp(changed, "ascii", 5) == 0);
    CHECK(((lookup_fn)functions[12])((uintptr_t)unchanged, 5) == identity);
    uint64_t converted_identity = ((lookup_fn)functions[12])((uintptr_t)changed, 5);
    CHECK(converted_identity != 0 && converted_identity != identity);
    early = mb_test_upper(changed, 5);
    uint64_t roundtrip_identity = ((lookup_fn)functions[12])((uintptr_t)early, 5);
    CHECK(memcmp(early, "ASCII", 5) == 0 && memcmp(source, "ASCII", 5) == 0);
    CHECK(roundtrip_identity != 0 && roundtrip_identity != identity && roundtrip_identity != converted_identity);
    mb_test_free(early);
    mb_test_free(changed);
    mb_test_free(unchanged);
    mb_test_free(source);
    CHECK(live == 0 && offset == 0);
    CHECK(((one_fn)functions[2])(identity) == 1 && ((one_fn)functions[2])(converted_identity) == 1);
    CHECK(((one_fn)functions[2])(roundtrip_identity) == 1);
    source = mb_test_persist("", 0);
    identity = ((lookup_fn)functions[12])((uintptr_t)source, 0);
    unchanged = mb_test_lower(source, 0);
    CHECK(identity != 0 && unchanged != source && ((lookup_fn)functions[12])((uintptr_t)unchanged, 0) == identity);
    mb_test_free(unchanged);
    mb_test_free(source);
    CHECK(live == 0 && offset == 0 && ((one_fn)functions[2])(identity) == 1);
    source = mb_test_persist("ASCII", 5);
    early = mb_test_persist(source, 5);
    CHECK(((lookup_fn)functions[6])((uintptr_t)early, 5) == 0);
    mb_test_free(source);
    identity = ((lookup_fn)functions[12])((uintptr_t)early, 5);
    CHECK(identity != 0 && ((lookup_fn)functions[6])((uintptr_t)source, 5) == 0);
    mb_test_free(early);
    mb_test_reset();
    CHECK(live == 0 && ((one_fn)functions[2])(identity) == 1);
    return 0;
}
