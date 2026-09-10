/* Independent graph ownership and cleanup callbacks for the emitted native collector. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define CHECK(condition) do { if (!(condition)) { fprintf(stderr, "line %d: %s\n", __LINE__, #condition); exit(1); } } while (0)
#if defined(__x86_64__)
#define HEAP_MAGIC UINT64_C(0x454c504800000000)
#else
#define HEAP_MAGIC UINT64_C(0)
#endif

struct header { uint32_t size, refs; uint64_t kind; };
struct cell { uint64_t tag, value, high; };
struct object { uint64_t class_id; void *properties; };
struct entry { uint64_t occupied, key, key_len, value, high, tag, previous, next; };
struct hash { uint64_t length, capacity, data[3]; struct entry *entries; uint64_t pins; };

_Alignas(16) unsigned char heap[65536] __asm__("_heap_buf");
uint64_t heap_offset __asm__("_heap_off");
uint64_t collecting __asm__("_gc_collecting");
uint64_t suppressed __asm__("_gc_release_suppressed");
uint64_t class_count __asm__("_class_gc_desc_count") = 1;
uint64_t payload_sizes[] __asm__("_class_object_payload_sizes") = { sizeof(struct object) };
uint64_t dynamic_flags[] __asm__("_class_object_dynamic_prop_flags") = { 1 };
unsigned char empty_descriptor[] = { 0 };
unsigned char *descriptors[] __asm__("_class_gc_desc_ptrs") = { empty_descriptor };

static unsigned destructions, frees, throws_seen, throw_next, callback_threw;
static unsigned rescue_next, break_cycle_next;
static struct cell *reserved;
extern void collect(void) __asm__("__rt_gc_collect_cycles");

static struct header *header(void *value) { return (struct header *)value - 1; }
static void *allocate(size_t size, unsigned kind) {
    size = (size + 15) & ~(size_t)15;
    struct header *block = (struct header *)(heap + heap_offset);
    heap_offset += sizeof(*block) + size;
    CHECK(heap_offset < sizeof(heap));
    *block = (struct header){ (uint32_t)size, 1, HEAP_MAGIC | kind };
    memset(block + 1, 0, size);
    return block + 1;
}

static struct object *cycle(void) {
    struct object *object = allocate(sizeof(*object), 4);
    struct hash *hash = allocate(sizeof(*hash), 3);
    struct cell *value = allocate(sizeof(*value), 5);
    struct cell *reference = allocate(sizeof(*reference), 5);
    object->properties = hash;
    *value = (struct cell){ 6, (uintptr_t)object, 0 };
    *reference = (struct cell){ 7, (uintptr_t)value, 1 };
    hash->length = hash->capacity = 1;
    hash->entries = malloc(sizeof(*hash->entries));
    CHECK(hash->entries != NULL);
    *hash->entries = (struct entry){ .occupied = 1, .value = (uintptr_t)reference, .tag = 7 };
    return object;
}

void destruct(void *) __asm__("__rt_call_object_destructor");
void destruct(void *value) {
    struct object *object = value;
    if (header(object)->kind & UINT64_C(0x4000)) { return; }
    struct hash *hash = object->properties;
    struct cell *reference = (void *)(uintptr_t)hash->entries->value;
    struct cell *child = (void *)(uintptr_t)reference->value;
    CHECK(collecting == 1 && suppressed == 1);
    CHECK(header(hash)->refs && header(reference)->refs && header(child)->refs);
    CHECK((void *)(uintptr_t)child->value == object && reference->high == 1);
    CHECK(!(header(object)->refs & UINT32_C(0x80000000)));
    header(object)->kind |= UINT64_C(0x4000);
    header(object)->refs |= UINT32_C(0x80000000);
    ++destructions;
    if (rescue_next) { ++header(object)->refs; rescue_next = 0; }
    if (break_cycle_next) {
        *child = (struct cell){ 8, 0, 0 };
        --header(object)->refs;
        break_cycle_next = 0;
    }
    if (header(reserved)->refs == 0) {
        header(reserved)->refs = 1;
        header(reserved)->kind = HEAP_MAGIC | 5;
        *reserved = (struct cell){ 0, 42, 0 };
    }
    if (throw_next) { callback_threw = 1; throw_next = 0; }
}

static void free_node(void *value) {
    CHECK(header(value)->kind != 0);
    header(value)->refs = 0;
    header(value)->kind = 0;
    ++frees;
}
void free_array(void *) __asm__("__rt_array_free_deep");
void free_hash(void *) __asm__("__rt_hash_free_deep");
void free_mixed(void *) __asm__("__rt_mixed_free_deep");
void free_object(void *) __asm__("__rt_object_free_deep");
void free_array(void *value) { free_node(value); }
void free_hash(void *value) { free(((struct hash *)value)->entries); free_node(value); }
void free_mixed(void *value) { free_node(value); }
void free_object(void *value) { free_node(value); }

void cleanup(void (*)(void *), void *, uint64_t *) __asm__("__rt_cleanup_call");
void cleanup(void (*operation)(void *), void *value, uint64_t *pending) {
    operation(value);
    if (callback_threw) { *pending = 1; callback_threw = 0; }
}
void throw_current(void) __asm__("__rt_throw_current");
void throw_current(void) {
    CHECK(collecting == 0 && suppressed == 0);
    ++throws_seen;
}

int main(void) {
    reserved = allocate(sizeof(*reserved), 0);
    header(reserved)->refs = 0;
    struct object *root = cycle();
    header(root)->refs = 2;
    collect();
    CHECK(destructions == 0 && frees == 0);
    header(root)->refs = 1;
    suppressed = 7;
    collect();
    CHECK(destructions == 0 && frees == 0 && suppressed == 7 && collecting == 0);
    suppressed = 0;
    collect();
    CHECK(destructions == 1 && frees == 4 && throws_seen == 0);
    CHECK(header(reserved)->refs == 1 && reserved->value == 42);
    CHECK(collecting == 0 && suppressed == 0);
    cycle();
    throw_next = 1;
    collect();
    CHECK(destructions == 2 && frees == 8 && throws_seen == 1);
    CHECK(header(reserved)->refs == 1 && reserved->value == 42);
    cycle();
    collect();
    CHECK(destructions == 3 && frees == 12 && throws_seen == 1);
    struct object *pinned = cycle();
    struct hash *pinned_hash = pinned->properties;
    ++header(pinned_hash)->refs;
    ++pinned_hash->pins;
    collect();
    CHECK(destructions == 3 && frees == 12 && header(pinned_hash)->refs == 2);
    --pinned_hash->pins;
    --header(pinned_hash)->refs;
    collect();
    CHECK(destructions == 4 && frees == 16 && throws_seen == 1);
    struct object *rescued = cycle();
    rescue_next = 1;
    collect();
    CHECK(destructions == 5 && frees == 16 && header(rescued)->refs == 2);
    CHECK(header(rescued)->kind & UINT64_C(0x4000));
    collect();
    CHECK(destructions == 5 && frees == 16);
    --header(rescued)->refs;
    collect();
    CHECK(destructions == 5 && frees == 20);
    cycle();
    break_cycle_next = 1;
    collect();
    CHECK(destructions == 6 && frees == 24 && throws_seen == 1);
    return 0;
}
