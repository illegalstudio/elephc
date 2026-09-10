/* Independent native ownership and identity assertions for internal hash construction. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <setjmp.h>

#define CHECK(condition) do { if (!(condition)) { fprintf(stderr, "line %d: %s\n", __LINE__, #condition); exit(1); } } while (0)
struct header { uint32_t size, refs; uint64_t kind; };
struct entry { uint64_t occupied, key, key_len, value, high, tag, previous, next; };
struct hash { uint64_t length, capacity, value_type, head, tail; struct entry *entries; uint64_t pins; int64_t next_index; };
struct next_index { int64_t key; uint64_t available; };
static size_t live_allocations;
const char allocation_error[] __asm__("_arr_cap_err_msg") = "allocation error";
const char append_error[] __asm__("_hash_append_err_msg") = "Cannot add element to the array as the next element is already occupied";
const uint64_t error_class_id __asm__("_spl_error_class_id") = 123;
uint64_t *exception __asm__("_exc_value");
void *guard_top __asm__("_hash_write_guard_top");
static jmp_buf append_handler;

extern struct hash *hash_new(uint64_t, uint64_t) __asm__("__rt_hash_new");
extern struct hash *grow(struct hash *) __asm__("__rt_hash_grow");
extern struct hash *grow_owned(struct hash *) __asm__("fixture_grow_owned");
extern struct hash *insert(struct hash *, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t) __asm__("__rt_hash_insert_owned");
extern struct entry *lookup(struct hash *, uint64_t) __asm__("fixture_lookup");
extern struct hash *unique(struct hash *) __asm__("__rt_hash_ensure_unique");
extern struct hash *pin(struct hash *) __asm__("__rt_hash_pin");
extern void unpin(struct hash *) __asm__("__rt_hash_unpin");
extern struct hash *set(struct hash *, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t) __asm__("__rt_hash_set");
extern struct hash *unset(struct hash *, uint64_t, uint64_t) __asm__("__rt_hash_unset");
extern struct hash *append(struct hash *, uint64_t, uint64_t, uint64_t) __asm__("__rt_hash_append");
extern struct next_index next_index(struct hash *) __asm__("__rt_hash_try_next_index");
extern int64_t required_index(struct hash *) __asm__("__rt_hash_next_index");

void *fixture_allocate(size_t) __asm__("fixture_allocate");
void fixture_free(void *) __asm__("fixture_free");
void *fixture_allocate(size_t size) {
    struct header *block = malloc(sizeof(*block) + (size ? size : 8));
    CHECK(block != NULL);
    *block = (struct header){ (uint32_t)size, 1, 0 };
    memset(block + 1, 0xa5, size ? size : 8);
    ++live_allocations;
    return block + 1;
}
void fixture_free(void *value) {
    CHECK(value != NULL && live_allocations > 0);
    --live_allocations;
    free((struct header *)value - 1);
}
void fixture_release(struct hash *) __asm__("fixture_release");
void fixture_release(struct hash *hash) {
    struct header *owner = (struct header *)hash - 1;
    int is_hash = (owner->kind & 255) == 3;
    CHECK(owner->refs > (is_hash ? hash->pins : 0));
    if (--owner->refs == 0) {
        if (is_hash) {
            CHECK(hash->pins == 0);
            fixture_free(hash->entries);
        }
        fixture_free(hash);
    }
}
void fixture_throw(void) __asm__("__rt_throw_current");
void fixture_throw(void) { longjmp(append_handler, 1); }
void unexpected_string(void) __asm__("__rt_str_eq");
void unexpected_string(void) { abort(); }
void unexpected_persist(void) __asm__("__rt_str_persist");
void unexpected_persist(void) { abort(); }
void unexpected_retain(void) __asm__("__rt_incref");
void unexpected_retain(void) { abort(); }
void unexpected_reference(void) __asm__("__rt_reference_array_copy");
void unexpected_reference(void) { abort(); }

/* Pins retain storage even with no PHP owner, but only PHP copies trigger COW. */
static void check_pin_ownership(void) {
    struct hash *hash = hash_new(4, 0);
    struct header *owner = (struct header *)hash - 1;
    CHECK(insert(hash, 0, UINT64_MAX, 10, 0, 0) == hash);
    CHECK(hash->pins == 0 && owner->refs == 1);
    CHECK(pin(hash) == hash && pin(hash) == hash);
    CHECK(owner->refs == 3 && hash->pins == 2 && unique(hash) == hash);
    ++owner->refs;
    struct hash *copy = unique(hash);
    CHECK(copy != hash && copy->pins == 0);
    CHECK(((struct header *)copy - 1)->refs == 1 && owner->refs == 3 && hash->pins == 2);
    CHECK(insert(copy, 0, UINT64_MAX, 20, 0, 0) == copy);
    CHECK(lookup(hash, 0)->value == 10 && lookup(copy, 0)->value == 20);
    CHECK(unique(hash) == hash);
    CHECK(insert(hash, 0, UINT64_MAX, 30, 0, 0) == hash);
    CHECK(lookup(hash, 0)->value == 30 && lookup(copy, 0)->value == 20);
    fixture_release(copy);
    fixture_release(hash);
    CHECK(live_allocations == 2 && owner->refs == 2 && hash->pins == 2);
    unpin(hash);
    CHECK(live_allocations == 2 && owner->refs == 1 && hash->pins == 1);
    unpin(hash);
    CHECK(live_allocations == 0);
}

/* Mutating the real table must keep history even after its largest entry disappears. */
static void check_next_indices(void) {
    const int64_t keys[] = {INT64_MIN, -100, -2, -1, 0, 9, INT64_MAX - 1, INT64_MAX};
    for (size_t i = 0; i < sizeof(keys) / sizeof(*keys); ++i) {
        for (int owned = 0; owned < 2; ++owned) {
            int64_t key = keys[i], expected = key == INT64_MAX ? key : key + 1;
            struct hash *hash = hash_new(4, 0);
            CHECK(hash->next_index == INT64_MIN);
            CHECK(next_index(hash).key == 0 && next_index(hash).available == 1);
            CHECK(required_index(hash) == 0);
            CHECK((owned ? insert : set)(hash, key, UINT64_MAX, 10, 0, 0) == hash);
            CHECK(hash->next_index == expected);
            CHECK(next_index(hash).available == (key != INT64_MAX));
            CHECK(unset(hash, key, UINT64_MAX) == hash && hash->length == 0);
            CHECK(hash->next_index == expected);
            CHECK(next_index(hash).key == expected && next_index(hash).available == 1);
            CHECK(grow_owned(hash) == hash && hash->next_index == expected);
            struct header *owner = (struct header *)hash - 1;
            ++owner->refs;
            struct hash *copy = unique(hash);
            CHECK(copy != hash && copy->next_index == expected);
            CHECK(append(copy, 42, 0, 0) == copy && lookup(copy, expected)->value == 42);
            CHECK(hash->length == 0 && hash->next_index == expected);
            CHECK(set(copy, INT64_MIN, UINT64_MAX, 50, 0, 0) == copy);
            CHECK(copy->next_index == (expected == INT64_MAX ? expected : expected + 1));
            fixture_release(copy);
            fixture_release(hash);
            CHECK(live_allocations == 0);
        }
    }
}

/* Query probes stay silent; ordinary appends raise Error and consume uninserted owners. */
static void check_exhaustion(void) {
    const uint64_t tags[] = {0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10};
    struct hash *hash = hash_new(4, 0);
    CHECK(insert(hash, INT64_MAX, UINT64_MAX, 99, 0, 0) == hash);
    struct next_index probe = next_index(hash);
    CHECK(probe.key == INT64_MAX && probe.available == 0 && exception == NULL);
    for (size_t i = 0; i < sizeof(tags) / sizeof(*tags); ++i) {
        uint64_t tag = tags[i], payload = 44;
        if (tag == 1 || (tag >= 4 && tag != 8)) {
            void *value = fixture_allocate(8);
            ((struct header *)value - 1)->kind = 1;
            payload = (uintptr_t)value;
        }
        if (setjmp(append_handler) == 0) {
            append(hash, payload, 0, tag);
            CHECK(0 && "append exhaustion did not raise Error");
        }
        CHECK(exception && exception[0] == error_class_id);
        CHECK(exception[2] == strlen(append_error));
        CHECK(memcmp((void *)(uintptr_t)exception[1], append_error, strlen(append_error)) == 0);
        CHECK(exception[3] == 0 && exception[5] == 0);
        fixture_free(exception);
        exception = NULL;
        CHECK(live_allocations == 2 && hash->length == 1 && lookup(hash, INT64_MAX)->value == 99);
    }
    if (setjmp(append_handler) == 0) {
        required_index(hash);
        CHECK(0 && "required next index did not raise Error");
    }
    CHECK(exception && exception[0] == error_class_id);
    fixture_free(exception);
    exception = NULL;
    fixture_release(hash);
    CHECK(live_allocations == 0);
}

int main(void) {
    struct hash *hash = hash_new(0, 0);
    struct hash *alias = hash;
    struct header *owner = (struct header *)hash - 1;
    uint64_t original_kind = owner->kind;
    CHECK(live_allocations == 2 && hash->capacity == 0 && hash->length == 0 && hash->pins == 0);
    owner->refs = 2;
    CHECK(pin(hash) == hash && pin(hash) == hash);
    for (uint64_t i = 0; i < 257; ++i) {
        if (hash->length * 4 >= hash->capacity * 3) {
            struct entry *old_entries = hash->entries;
            uint64_t old_capacity = hash->capacity;
            CHECK(grow_owned(hash) == alias);
            CHECK(hash->entries != old_entries);
            CHECK(hash->capacity == (old_capacity ? old_capacity * 2 : 1));
            CHECK(live_allocations == 2 && owner->refs == 4 && owner->kind == original_kind && hash->pins == 2);
        }
        CHECK(insert(hash, i * 37, UINT64_MAX, i + 1000, 0, 0) == alias);
        CHECK(alias->length == i + 1);
        uint64_t cursor = alias->head;
        for (uint64_t j = 0; j <= i; ++j) {
            CHECK(cursor < alias->capacity);
            struct entry *entry = &alias->entries[cursor];
            CHECK(entry->occupied == 1 && entry->key == j * 37 && entry->value == j + 1000);
            CHECK(lookup(alias, j * 37) == entry);
            cursor = entry->next;
        }
        CHECK(cursor == UINT64_MAX);
    }
    CHECK(lookup(hash, UINT64_MAX) == NULL);
    unpin(hash);
    unpin(hash);
    fixture_release(hash);
    CHECK(grow(hash) == alias && hash->pins == 0);
    CHECK(live_allocations == 2 && owner->refs == 1 && owner->kind == original_kind);
    fixture_release(hash);
    CHECK(live_allocations == 0);
    check_pin_ownership();
    check_next_indices();
    check_exhaustion();
    return 0;
}
