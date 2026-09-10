/* Independent ownership and destructor observations for capture-reference initialization. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define CHECK(c) do { if (!(c)) { fprintf(stderr, "line %d: %s\n", __LINE__, #c); exit(1); } } while (0)
#if defined(__x86_64__)
#define HEAP_MAGIC UINT64_C(0x454c504800000000)
#else
#define HEAP_MAGIC UINT64_C(0)
#endif
struct header { uint32_t size, refs; uint64_t kind; };
struct value { uint64_t tag, low, high; };
struct entry { uint64_t occupied, key, key_len, value, high, tag, previous, next; };
struct hash { uint64_t length, capacity, value_type, head, tail; struct entry *entries; uint64_t pins; };
struct result { uint64_t ready; struct value *writer, *discarded; };
struct allocation { void *value; int live; };
static struct allocation allocations[4096];
static size_t allocated, live, old_destroyed, late_destroyed;
static uint64_t pending_callback, mode;
static unsigned actions;
enum { REPLACE = 1, THROW = 2, RECURSE = 4, SNAPSHOT = 8 };
static struct value *reference, *replacement;
static struct hash *snapshot;
static struct result *active_output;
const char allocation_error[] __asm__("_arr_cap_err_msg") = "allocation error";

extern struct hash *hash_new(uint64_t, uint64_t) __asm__("__rt_hash_new");
extern int begin(void *, struct value *, uint64_t, struct result *) __asm__("__rt_mbstring_capture_reference_begin");
extern void invoke(void (*)(void *), void *) __asm__("fixture_invoke");

static struct header *header(void *value) { return (struct header *)value - 1; }
static int is_live(void *value) {
    for (size_t i = 0; i < allocated; ++i) {
        if (allocations[i].value == value && allocations[i].live) { return 1; }
    }
    return 0;
}
void *fixture_allocate(size_t) __asm__("fixture_allocate");
void *fixture_allocate(size_t size) {
    CHECK(allocated < sizeof(allocations) / sizeof(*allocations));
    struct header *owner = malloc(sizeof(*owner) + (size ? size : 8));
    CHECK(owner != NULL);
    *owner = (struct header){(uint32_t)size, 1, 0};
    memset(owner + 1, 0xa5, size ? size : 8);
    allocations[allocated++] = (struct allocation){owner + 1, 1};
    ++live;
    return owner + 1;
}
static void fixture_free(void *value) {
    for (size_t i = 0; i < allocated; ++i) {
        if (allocations[i].value == value && allocations[i].live) {
            allocations[i].live = 0;
            CHECK(live > 0);
            --live;
            free(header(value));
            return;
        }
    }
    CHECK(0 && "release of an unowned allocation");
}
void *fixture_retain(void *) __asm__("fixture_retain");
void *fixture_retain(void *value) {
    CHECK(is_live(value) && header(value)->refs > 0);
    ++header(value)->refs;
    return value;
}
void fixture_release(void *) __asm__("fixture_release");
static struct value *box_owned(uint64_t tag, void *child, uint64_t high) {
    struct value *box = fixture_allocate(sizeof(*box));
    header(box)->kind = HEAP_MAGIC | 5;
    *box = (struct value){tag, (uintptr_t)child, high};
    return box;
}
static void *object(uint64_t late) {
    uint64_t *value = fixture_allocate(sizeof(*value));
    header(value)->kind = HEAP_MAGIC | 4;
    *value = late;
    return value;
}
static struct hash *array_value(struct value *box) {
    CHECK(is_live(box) && box->tag == 5 && box->high == 0);
    struct hash *hash = (void *)(uintptr_t)box->low;
    CHECK(is_live(hash) && hash->value_type == 7 && hash->pins == 0);
    return hash;
}
static struct value *replacement_value(void) {
    struct hash *hash = hash_new(4, 7);
    hash->length = 1;
    hash->head = hash->tail = 0;
    hash->entries[0] = (struct entry){1, 0, UINT64_MAX, (uintptr_t)object(1), 0, 6, UINT64_MAX, UINT64_MAX};
    return box_owned(5, hash, 0);
}

/* Observe the caller during old-value destruction, then optionally change its current value. */
static void destruct_old(void) {
    ++old_destroyed;
    CHECK(active_output->writer == reference && active_output->ready == 0);
    CHECK(active_output->discarded == NULL && header(reference)->refs == 2);
    if (mode) {
        struct hash *hash = array_value((void *)(uintptr_t)reference->low);
        CHECK(hash->length == 0 && header(hash)->refs == 1);
        if (actions & SNAPSHOT) { snapshot = fixture_retain(hash); }
    } else { CHECK(reference->low == 0); }
    if (actions & RECURSE) {
        struct result nested;
        CHECK(begin(NULL, reference, 1 - mode, &nested) == 0);
        CHECK(nested.ready == 1 && nested.writer == reference && nested.discarded == NULL);
        CHECK(header(reference)->refs == 3);
        CHECK(array_value((void *)(uintptr_t)reference->low)->length == 0);
        fixture_release(nested.writer);
    }
    if (actions & REPLACE) {
        struct value *old = (void *)(uintptr_t)reference->low;
        replacement = replacement_value();
        reference->low = (uintptr_t)replacement;
        fixture_release(old);
    }
    if (actions & THROW) { pending_callback = 1; }
}

/* Count each owner independently from the native initializer and free complete payload trees. */
void fixture_release(void *value) {
    if (!value) { return; }
    CHECK(is_live(value) && header(value)->refs > 0);
    if (--header(value)->refs) { return; }
    uint64_t kind = header(value)->kind & 0xff;
    if (kind == 3) {
        struct hash *hash = value;
        CHECK(hash->pins == 0);
        for (uint64_t i = 0; i < hash->capacity; ++i) {
            struct entry *entry = &hash->entries[i];
            if (entry->occupied != 1) { continue; }
            CHECK(entry->tag == 6 && entry->key_len == UINT64_MAX);
            fixture_release((void *)(uintptr_t)entry->value);
        }
        fixture_free(hash->entries);
    } else if (kind == 4) {
        if (*(uint64_t *)value) { ++late_destroyed; } else { destruct_old(); }
    } else if (kind == 5) {
        struct value *box = value;
        CHECK(box->tag == 5 || box->tag == 6 || box->tag == 7 || box->tag == 8 || box->tag == 0);
        if (box->tag >= 5 && box->tag <= 7) { fixture_release((void *)(uintptr_t)box->low); }
    } else { CHECK(0 && "unexpected heap kind"); }
    fixture_free(value);
}
void cleanup(void (*)(void *), void *, uint64_t *) __asm__("__rt_cleanup_call");
void cleanup(void (*operation)(void *), void *value, uint64_t *pending) {
    invoke(operation, value);
    if (pending_callback) { *pending = 1; pending_callback = 0; }
}
void fixture_unsupported(void) __asm__("fixture_unsupported");
void fixture_unsupported(void) { CHECK(0 && "unexpected boxing path"); }

/* Writer ownership does not postpone old destruction; overwritten reentrant owners outlive it. */
static void publication_case(uint64_t typed, unsigned script) {
    CHECK(live == 0);
    old_destroyed = late_destroyed = 0;
    mode = typed;
    actions = script;
    snapshot = NULL;
    replacement = NULL;
    reference = box_owned(7, box_owned(6, object(0), 0), 1);
    struct result output;
    active_output = &output;
    CHECK(begin(NULL, reference, mode, &output) == ((actions & THROW) ? 2 : 0));
    CHECK(output.ready == 1 && output.writer == reference && header(reference)->refs == 2);
    CHECK(old_destroyed == 1 && late_destroyed == 0);
    struct value *current = (void *)(uintptr_t)reference->low;
    struct hash *hash = array_value(current);
    if (mode) {
        CHECK(output.discarded == NULL);
        CHECK(!(actions & REPLACE) || current == replacement);
        CHECK(hash->length == !!(actions & REPLACE));
    } else {
        CHECK(hash->length == 0 && current != replacement);
        CHECK(!!output.discarded == !!(actions & (REPLACE | RECURSE)));
        if (actions & REPLACE) { CHECK(output.discarded == replacement); }
    }
    CHECK(header(current)->refs == 1);
    fixture_release(output.writer);
    CHECK(header(reference)->refs == 1 && late_destroyed == 0);
    fixture_release(reference);
    CHECK(late_destroyed == (mode && (actions & REPLACE) ? 1u : 0u));
    if (output.discarded) { CHECK(is_live(output.discarded) && header(output.discarded)->refs == 1); }
    fixture_release(output.discarded);
    CHECK(late_destroyed == !!(actions & REPLACE));
    if (snapshot) { CHECK(snapshot->length == 0 && header(snapshot)->refs == 1); fixture_release(snapshot); }
    CHECK(live == 0 && pending_callback == 0);
}

/* Shared old boxes and shared old payloads retain their own independent destruction boundary. */
static void alias_case(uint64_t typed, int share_box) {
    CHECK(live == 0);
    late_destroyed = 0;
    void *old_object = object(1);
    struct value *old_box = box_owned(6, old_object, 0);
    void *alias = fixture_retain(share_box ? (void *)old_box : old_object);
    struct value *root = box_owned(7, old_box, 1);
    struct result output;
    CHECK(begin(NULL, root, typed, &output) == 0);
    CHECK(output.writer == root && output.ready == 1 && output.discarded == NULL);
    CHECK(late_destroyed == 0 && header(alias)->refs == 1);
    CHECK(array_value((void *)(uintptr_t)root->low)->length == 0);
    fixture_release(output.writer);
    fixture_release(root);
    CHECK(late_destroyed == 0);
    fixture_release(alias);
    CHECK(late_destroyed == 1 && live == 0);
}

/* Invalid modes and reference shapes clear only the output, without new owners or caller mutation. */
static void invalid_cases(void) {
    struct value malformed[] = {{0, 42, 0}, {7, 0, 0}, {7, 0, 2}};
    struct result output;
    size_t before = allocated;
    for (size_t i = 0; i < sizeof(malformed) / sizeof(*malformed); ++i) {
        struct value saved = malformed[i];
        memset(&output, 0xa5, sizeof(output));
        CHECK(begin(NULL, &malformed[i], 0, &output) == 1);
        CHECK(output.ready == 0 && output.writer == NULL && output.discarded == NULL);
        CHECK(memcmp(&saved, &malformed[i], sizeof(saved)) == 0 && allocated == before);
    }
    struct value *root = box_owned(7, box_owned(0, (void *)(uintptr_t)42, 0), 1);
    uint64_t child = root->low;
    before = allocated;
    CHECK(begin(NULL, root, 0, NULL) == 1);
    for (uint64_t i = 2; i < 4; ++i) {
        memset(&output, 0xa5, sizeof(output));
        CHECK(begin(NULL, root, i == 2 ? i : UINT64_MAX, &output) == 1);
        CHECK(output.ready == 0 && output.writer == NULL && output.discarded == NULL);
        CHECK(allocated == before && root->low == child && header(root)->refs == 1);
    }
    CHECK(begin(NULL, NULL, 0, &output) == 1);
    CHECK(output.ready == 0 && output.writer == NULL && output.discarded == NULL);
    fixture_release(root);
    CHECK(live == 0);
}

int main(void) {
    for (uint64_t typed = 0; typed < 2; ++typed) {
        for (unsigned script = 0; script < 16; ++script) { publication_case(typed, script); }
        for (int shared = 0; shared < 2; ++shared) { alias_case(typed, shared); }
    }
    invalid_cases();
    return 0;
}
