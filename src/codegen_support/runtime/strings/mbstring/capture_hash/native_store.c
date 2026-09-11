/* Independent allocation, destructor, and pending-status observations for native capture stores. */
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
struct hash { uint64_t length, capacity, value_type, head, tail; struct entry *entries; uint64_t pins; int64_t next_index; };
struct indexed { uint64_t length, capacity, stride, words[6]; };
struct allocation { void *value; int live; };
struct guard { struct guard *next; struct hash *hash; uint64_t key, key_len, changed; };
static struct allocation allocations[8192];
static size_t allocated, live, destroyed, pending_callback;
static struct hash *current, *alias, *selected, *snapshot;
static struct value *live_reference;
static unsigned actions;
static void (*destructor_override)(void *);
enum { RETARGET = 1, GROW = 2, THROW = 4, DROP_ALIAS = 8 };
enum { REENTRY_STRING = 16, REENTRY_FALSE = 32, REENTRY_UNSET = 64,
       REENTRY_UNSET_SET = 128, REENTRY_TWO_SETS = 256, REENTRY_CAPTURE = 512,
       REENTRY_CHAIN = 1024, REENTRY_CONVERT = 2048, REENTRY_COPY = 4096,
       REENTRY_DROP_COPY = 8192, REENTRY_MASK = 16368 };
static struct value reentry_key;
void *hash_write_guard_top __asm__("_hash_write_guard_top");
const char allocation_error[] __asm__("_arr_cap_err_msg") = "allocation error";

extern struct hash *hash_new(uint64_t, uint64_t) __asm__("__rt_hash_new");
extern struct hash *unique(struct hash *) __asm__("__rt_hash_ensure_unique");
extern struct hash *to_mixed(struct hash *) __asm__("__rt_hash_to_mixed");
extern struct hash *insert(struct hash *, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t) __asm__("__rt_hash_insert_owned");
extern struct hash *set(struct hash *, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t) __asm__("__rt_hash_set");
extern struct hash *remove_key(struct hash *, uint64_t, uint64_t) __asm__("__rt_hash_unset");
extern void guard_push(struct guard *, struct hash *, uint64_t, uint64_t) __asm__("__rt_hash_write_guard_push");
extern uint64_t guard_pop(struct guard *) __asm__("__rt_hash_write_guard_pop");
extern uint64_t guard_claim(struct hash *, uint64_t, uint64_t) __asm__("__rt_hash_write_guard_claim");
extern uint64_t guard_owns(struct hash *, uint64_t, uint64_t) __asm__("__rt_hash_write_guard_owns");
extern struct entry *lookup(struct hash *, uint64_t, uint64_t) __asm__("fixture_lookup");
extern int capture_store(void *, struct hash *, const struct value *, const struct value *) __asm__("__rt_mbstring_capture_hash_store");
extern int reference_store(void *, struct value *, const struct value *, const struct value *) __asm__("__rt_mbstring_capture_reference_store");
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
    *owner = (struct header){ (uint32_t)size, 1, 0 };
    memset(owner + 1, 0xa5, size ? size : 8);
    allocations[allocated++] = (struct allocation){ owner + 1, 1 };
    ++live;
    return owner + 1;
}
void fixture_free(void *) __asm__("fixture_free");
void fixture_free(void *value) {
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
void *fixture_persist(const void *, size_t) __asm__("fixture_persist");
void *fixture_persist(const void *bytes, size_t length) {
    void *result = fixture_allocate(length);
    header(result)->kind = HEAP_MAGIC | 1;
    memcpy(result, bytes, length);
    return result;
}

static int store(struct hash *hash, uint64_t key, const void *bytes, size_t length) {
    struct value k = { 0, key, 0 };
    struct value v = bytes ? (struct value){ 1, (uintptr_t)bytes, length } : (struct value){ 3, 0, 0 };
    return capture_store(NULL, hash, &k, &v);
}
/* Call through the actual retained reference identity, resolving its current hash each time. */
static int store_reference(struct value *reference, uint64_t key, const void *bytes, size_t length) {
    struct value k = {0, key, 0};
    struct value v = bytes ? (struct value){1, (uintptr_t)bytes, length} : (struct value){3, 0, 0};
    return reference_store(NULL, reference, &k, &v);
}
static struct value *box_owned(uint64_t tag, uint64_t low, uint64_t high) {
    struct value *box = fixture_allocate(sizeof(*box));
    header(box)->kind = HEAP_MAGIC | 5;
    *box = (struct value){tag, low, high};
    return box;
}
static void check_string(struct entry *entry, const void *bytes, size_t length) {
    CHECK(entry != NULL && entry->tag == 1 && entry->high == length);
    CHECK(is_live((void *)(uintptr_t)entry->value));
    CHECK(memcmp((void *)(uintptr_t)entry->value, bytes, length) == 0);
}
void fixture_release(void *) __asm__("fixture_release");

/* One-owner replacement objects carry a finite destructor script. */
static void *object(unsigned remaining) {
    uint64_t *result = fixture_allocate(sizeof(*result));
    header(result)->kind = HEAP_MAGIC | 4;
    *result = remaining;
    return result;
}

/* Exercise real ordinary writers while the capture owns the current destructor release. */
static void destruct_reentry(void *value) {
    uint64_t key = reentry_key.low;
    uint64_t key_len = reentry_key.tag == 0 ? UINT64_MAX : reentry_key.high;
    struct entry *old = lookup(selected, key, key_len);
    uint64_t old_slot_value = old->value;
    uint64_t old_tag = old->tag, old_value = old->value;
    while (old_tag == 7) {
        struct value *box = (void *)(uintptr_t)old_value;
        CHECK(is_live(box) && header(box)->refs == 0);
        old_tag = box->tag;
        old_value = box->low;
    }
    CHECK(old_tag == 6 && old_value == (uintptr_t)value);
    CHECK(hash_write_guard_top != NULL);
    if (actions & REENTRY_COPY) {
        CHECK(snapshot == NULL);
        struct guard *guard = hash_write_guard_top;
        uint64_t changed = guard->changed;
        snapshot = to_mixed(fixture_retain(selected));
        CHECK(snapshot != selected && snapshot->pins == 0 && header(snapshot)->refs == 1);
        CHECK(header(selected)->refs == 2 && selected->pins == 1 && guard->changed == changed);
        struct entry *copy = lookup(snapshot, key, key_len);
        struct value *box = (void *)(uintptr_t)copy->value;
        CHECK(copy->tag == 7 && header(box)->refs == 1);
        CHECK(box->tag == 6 && box->low == (uintptr_t)value);
        CHECK(header(value)->refs == UINT32_C(0x80000001));
        CHECK(lookup(selected, key, key_len)->value == old_slot_value);
        if (actions & REENTRY_DROP_COPY) {
            fixture_release(snapshot);
            snapshot = NULL;
            CHECK(header(value)->refs == UINT32_C(0x80000000));
        }
    }
    if (actions & REENTRY_CONVERT) {
        CHECK(to_mixed(selected) == selected);
        struct entry *entry = lookup(selected, key, key_len);
        struct value *box = (void *)(uintptr_t)entry->value;
        CHECK(entry->tag == 7 && entry->high == 0 && header(box)->refs == 1);
        CHECK(box->tag == 6 && box->low == (uintptr_t)value);
        CHECK(header(value)->refs == UINT32_C(0x80000001) + (snapshot != NULL));
        CHECK(to_mixed(selected) == selected);
        CHECK(lookup(selected, key, key_len)->value == (uintptr_t)box);
        CHECK(header(value)->refs == UINT32_C(0x80000001) + (snapshot != NULL));
    }
    if (actions & (REENTRY_UNSET | REENTRY_UNSET_SET)) {
        CHECK(remove_key(selected, key, key_len) == selected);
        CHECK(lookup(selected, key, key_len) == NULL);
    }
    if (actions & (REENTRY_STRING | REENTRY_UNSET_SET | REENTRY_TWO_SETS)) {
        void *replacement = fixture_persist("intermediate", 12);
        CHECK(set(selected, key, key_len, (uintptr_t)replacement, 12, 1) == selected);
        check_string(lookup(selected, key, key_len), "intermediate", 12);
        if (actions & REENTRY_TWO_SETS) {
            void *last = fixture_persist("last", 4);
            CHECK(set(selected, key, key_len, (uintptr_t)last, 4, 1) == selected);
            CHECK(!is_live(replacement));
            check_string(lookup(selected, key, key_len), "last", 4);
        }
    }
    if (actions & REENTRY_FALSE) {
        CHECK(set(selected, key, key_len, 0, 0, 3) == selected);
        CHECK(lookup(selected, key, key_len)->tag == 3);
    }
    if (actions & REENTRY_CAPTURE) {
        struct value nested = { 1, (uintptr_t)"nested", 6 };
        CHECK(capture_store(NULL, selected, &reentry_key, &nested) == 0);
        check_string(lookup(selected, key, key_len), "nested", 6);
    }
    if ((actions & REENTRY_CHAIN) && *(uint64_t *)value) {
        void *replacement = object((unsigned)*(uint64_t *)value - 1);
        CHECK(set(selected, key, key_len, (uintptr_t)replacement, 0, 6) == selected);
        CHECK(lookup(selected, key, key_len)->value == (uintptr_t)replacement);
    }
    CHECK(selected->pins == 1);
    if (actions & THROW) { pending_callback = 1; }
}

/* The overwritten value remains observable while its destructor runs. */
static void destruct(void *value) {
    ++destroyed;
    CHECK(selected->pins == 1);
    if (actions & REENTRY_MASK) { destruct_reentry(value); return; }
    CHECK(lookup(selected, 0, UINT64_MAX)->value == (uintptr_t)value);
    CHECK(lookup(selected, 0, UINT64_MAX)->tag == 6);
    if (actions & RETARGET) {
        current = hash_new(4, 7);
        CHECK(store(current, 88, "replacement", 11) == 0);
        if (live_reference) {
            struct value *old = (void *)(uintptr_t)live_reference->low;
            live_reference->low = (uintptr_t)box_owned(5, (uintptr_t)current, 0);
            fixture_release(old);
        } else { fixture_release(selected); }
        CHECK(header(selected)->refs == 2 && selected->pins == 1);
        CHECK(unique(alias) == selected);
    }
    if (actions & GROW) {
        struct entry *old_storage = selected->entries;
        for (uint64_t i = 100; i < 300; ++i) {
            CHECK(store(selected, i, NULL, 0) == 0);
            CHECK(selected->pins == 1);
        }
        CHECK(selected->entries != old_storage);
    }
    if (actions & DROP_ALIAS) {
        fixture_release(alias);
        alias = NULL;
        CHECK(header(selected)->refs == 1 && selected->pins == 1);
    }
    if (actions & THROW) { pending_callback = 1; }
}

/* Implements ordinary owner release independently of the emitted store and pin helpers. */
void fixture_release(void *value) {
    CHECK(is_live(value) && header(value)->refs > 0);
    if (--header(value)->refs) { return; }
    uint64_t kind = header(value)->kind & 0xff;
    if (kind == 2) {
        struct indexed *array = value;
        uint64_t tag = (header(value)->kind >> 8) & 127;
        for (size_t i = 0; i < array->length; ++i) {
            if (tag == 1 || (tag >= 4 && tag <= 7) || tag == 10) {
                fixture_release((void *)(uintptr_t)array->words[i * array->stride / 8]);
            }
        }
    } else if (kind == 3) {
        struct hash *hash = value;
        CHECK(hash->pins == 0);
        for (uint64_t i = 0; i < hash->capacity; ++i) {
            struct entry *entry = &hash->entries[i];
            if (entry->occupied != 1) { continue; }
            if (entry->key_len != UINT64_MAX) { fixture_release((void *)(uintptr_t)entry->key); }
            if (entry->tag == 1 || (entry->tag >= 4 && entry->tag <= 7) || entry->tag == 10) {
                fixture_release((void *)(uintptr_t)entry->value);
            } else { CHECK(entry->tag == 0 || entry->tag == 2 || entry->tag == 3 || entry->tag == 8); }
        }
        fixture_free(hash->entries);
    } else if (kind == 4) {
        header(value)->refs = UINT32_C(0x80000000);
        if (!(header(value)->kind & UINT64_C(0x4000))) {
            header(value)->kind |= UINT64_C(0x4000);
            if (destructor_override) { destructor_override(value); } else { destruct(value); }
        }
        header(value)->refs &= UINT32_C(0x7fffffff);
        if (header(value)->refs) { return; }
    } else if (kind == 5) {
        struct value *box = value;
        if (box->tag == 1 || (box->tag >= 4 && box->tag <= 7) || box->tag == 10) {
            fixture_release((void *)(uintptr_t)box->low);
        } else { CHECK(box->tag == 0 || box->tag == 2 || box->tag == 3 || box->tag == 8); }
    } else { CHECK(kind == 1); }
    fixture_free(value);
}

/* Complete the operation first, then publish a modeled escaping callback exception. */
void cleanup(void (*)(void *), void *, uint64_t *) __asm__("__rt_cleanup_call");
void cleanup(void (*operation)(void *), void *value, uint64_t *pending) {
    invoke(operation, value);
    if (pending_callback) { *pending = 1; pending_callback = 0; }
}
void unsupported_resource(void) __asm__("__rt_resource_id_of");
void unsupported_resource(void) { CHECK(0 && "unexpected resource payload"); }

/* Ordinary conversion transfers existing owners and repeated conversion keeps the same boxes. */
static void owned_conversion(void) {
    struct hash *hash = hash_new(4, 7);
    void *text = fixture_persist("owned", 5);
    CHECK(set(hash, 7, UINT64_MAX, (uintptr_t)text, 5, 1) == hash);
    CHECK(set(hash, 8, UINT64_MAX, 42, 0, 0) == hash);
    CHECK(to_mixed(hash) == hash && hash->value_type == 7);
    struct value *box = (void *)(uintptr_t)lookup(hash, 7, UINT64_MAX)->value;
    CHECK(box->tag == 1 && box->low == (uintptr_t)text && box->high == 5);
    CHECK(header(text)->refs == 1 && header(box)->refs == 1);
    CHECK(to_mixed(hash) == hash && lookup(hash, 7, UINT64_MAX)->value == (uintptr_t)box);
    CHECK(header(text)->refs == 1 && header(box)->refs == 1);
    fixture_release(hash);
    CHECK(live == 0 && hash_write_guard_top == NULL);
}

/* Shared construction writes preserve existing keys, insertion order, and binary bytes. */
static void basic(void) {
    struct hash *hash = hash_new(0, 7);
    struct hash *copy = fixture_retain(hash);
    static const char bytes[] = {'a', 0, 'z'};
    for (uint64_t i = 0; i < 257; ++i) {
        CHECK(store(hash, i, bytes, sizeof(bytes)) == 0);
        CHECK(hash == copy && hash->pins == 0 && header(hash)->refs == 2);
        check_string(lookup(copy, i, UINT64_MAX), bytes, sizeof(bytes));
    }
    CHECK(store(hash, 0, NULL, 0) == 0 && lookup(copy, 0, UINT64_MAX)->tag == 3);
    CHECK(store(hash, 1, "", 0) == 0);
    check_string(lookup(copy, 1, UINT64_MAX), "", 0);
    static const char name[] = {'n', 0, 'm'};
    struct value key = { 1, (uintptr_t)name, sizeof(name) };
    struct value value = { 1, (uintptr_t)bytes, sizeof(bytes) };
    CHECK(capture_store(NULL, hash, &key, &value) == 0);
    check_string(lookup(copy, (uintptr_t)name, sizeof(name)), bytes, sizeof(bytes));
    CHECK(capture_store(NULL, hash, &key, &value) == 0);
    uint64_t cursor = hash->head;
    for (uint64_t i = 0; i < 257; ++i) {
        CHECK(cursor < hash->capacity && hash->entries[cursor].key == i);
        cursor = hash->entries[cursor].next;
    }
    CHECK(hash->entries[cursor].key_len == sizeof(name) && hash->entries[cursor].next == UINT64_MAX);
    fixture_release(copy);
    fixture_release(hash);
    CHECK(live == 0);
}

/* Subsequent capture calls follow the live reference; the current write finishes on its old array. */
static void retarget(unsigned mode, int by_reference) {
    actions = mode;
    current = hash_new(4, 7);
    selected = current;
    alias = fixture_retain(current);
    if (by_reference) {
        struct value *value = box_owned(5, (uintptr_t)current, 0);
        live_reference = box_owned(7, (uintptr_t)value, 1);
        fixture_retain(live_reference);
    }
    void *object = fixture_allocate(8);
    header(object)->kind = 4;
    CHECK(insert(current, 0, UINT64_MAX, (uintptr_t)object, 0, 6) == current);
    size_t before = destroyed;
    int status = by_reference ? store_reference(live_reference, 0, "first", 5)
        : store(current, 0, "first", 5);
    CHECK(status == ((mode & THROW) ? 2 : 0));
    CHECK(destroyed == before + 1 && current != selected && current->pins == 0);
    if (alias) {
        CHECK(alias == selected && alias->pins == 0 && header(alias)->refs == 1);
        check_string(lookup(alias, 0, UINT64_MAX), "first", 5);
    } else { CHECK(!is_live(selected)); }
    if (by_reference) {
        CHECK(store_reference(live_reference, 1, "second", 6) == 0);
        CHECK(store_reference(live_reference, 2, NULL, 0) == 0);
        CHECK(header(live_reference)->refs == 2);
    } else {
        CHECK(store(current, 1, "second", 6) == 0);
        CHECK(store(current, 2, NULL, 0) == 0);
    }
    CHECK(lookup(current, 0, UINT64_MAX) == NULL);
    check_string(lookup(current, 1, UINT64_MAX), "second", 6);
    CHECK(lookup(current, 2, UINT64_MAX)->tag == 3);
    if (alias) { CHECK(lookup(alias, 1, UINT64_MAX) == NULL); fixture_release(alias); }
    if (by_reference) {
        fixture_release(live_reference);
        fixture_release(live_reference);
        live_reference = NULL;
    } else { fixture_release(current); }
    current = selected = alias = NULL;
    CHECK(live == 0 && pending_callback == 0);
}

/* Invalid or non-hash reference destinations remain untouched and publish no capture. */
static void invalid_reference(void) {
    CHECK(store_reference(NULL, 0, "no", 2) == 1);
    struct value *scalar = box_owned(0, 42, 0);
    CHECK(store_reference(scalar, 0, "no", 2) == 1 && scalar->low == 42);
    struct value *reference = box_owned(7, (uintptr_t)scalar, 1);
    CHECK(store_reference(reference, 0, "no", 2) == 1);
    CHECK(reference->low == (uintptr_t)scalar && scalar->low == 42);
    reference->high = 0;
    CHECK(store_reference(reference, 0, "no", 2) == 1);
    fixture_release(reference);
    CHECK(live == 0);
}

/* Guards compare key bytes and container identity, and may be unlinked out of order. */
static void guards(void) {
    struct hash first = {0}, second = {0};
    struct guard outer, inner, other;
    const char left[] = {'a', 0, 'z'}, right[] = {'a', 0, 'z'}, different[] = {'a', 0, 'x'};
    CHECK(hash_write_guard_top == NULL);
    CHECK(guard_owns(&first, 7, UINT64_MAX) == 1);
    CHECK(guard_claim(&first, 7, UINT64_MAX) == 1);
    guard_push(&outer, &first, (uintptr_t)left, sizeof(left));
    CHECK(guard_owns(&first, (uintptr_t)right, sizeof(right)) == 0 && outer.changed == 0);
    CHECK(guard_owns(&second, (uintptr_t)right, sizeof(right)) == 1 && outer.changed == 0);
    CHECK(guard_claim(&second, (uintptr_t)right, sizeof(right)) == 1 && outer.changed == 0);
    CHECK(guard_claim(&first, (uintptr_t)different, sizeof(different)) == 1 && outer.changed == 0);
    CHECK(guard_claim(&first, (uintptr_t)right, sizeof(right) - 1) == 1 && outer.changed == 0);
    CHECK(guard_claim(&first, (uintptr_t)left, UINT64_MAX) == 1 && outer.changed == 0);
    guard_push(&inner, &first, (uintptr_t)right, sizeof(right));
    guard_push(&other, &second, 7, UINT64_MAX);
    CHECK(guard_owns(&first, (uintptr_t)left, sizeof(left)) == 0);
    CHECK(inner.changed == 0 && outer.changed == 0 && other.changed == 0);
    CHECK(guard_claim(&first, (uintptr_t)left, sizeof(left)) == 0);
    CHECK(inner.changed == 1 && outer.changed == 1 && other.changed == 0);
    CHECK(guard_owns(&first, (uintptr_t)right, sizeof(right)) == 1);
    CHECK(guard_claim(&first, (uintptr_t)right, sizeof(right)) == 1);
    CHECK(guard_pop(&outer) == 1 && inner.next == NULL && hash_write_guard_top == &other);
    CHECK(guard_pop(&outer) == 1 && hash_write_guard_top == &other);
    CHECK(guard_claim(&second, 8, UINT64_MAX) == 1 && other.changed == 0);
    CHECK(guard_claim(&second, 7, UINT64_MAX) == 0 && other.changed == 1);
    CHECK(guard_pop(&inner) == 1 && other.next == NULL);
    CHECK(guard_pop(&other) == 1 && hash_write_guard_top == NULL);
}

/* Captures consume every callback replacement exactly once and preserve key ordering. */
static void reentry(unsigned mode, int string_key, unsigned box_depth) {
    static const char name[] = {'k', 0, 'y'};
    actions = mode;
    reentry_key = string_key ? (struct value){1, (uintptr_t)name, sizeof(name)} : (struct value){0, 0, 0};
    uint64_t key_len = string_key ? sizeof(name) : UINT64_MAX;
    current = selected = hash_new(4, 7);
    struct value payload = {6, (uintptr_t)object(3), 0};
    for (unsigned depth = 0; depth < box_depth; ++depth) {
        struct value *box = fixture_allocate(sizeof(*box));
        header(box)->kind = HEAP_MAGIC | 5;
        *box = payload;
        if (depth) { box->high = 1; }
        payload = (struct value){7, (uintptr_t)box, 0};
    }
    CHECK(set(selected, reentry_key.low, key_len, payload.low, payload.high, payload.tag) == selected);
    CHECK(store(selected, 90, NULL, 0) == 0);
    size_t before = destroyed;
    struct value capture = {1, (uintptr_t)"final", 5};
    CHECK(capture_store(NULL, selected, &reentry_key, &capture) == ((mode & THROW) ? 2 : 0));
    CHECK(destroyed == before + ((mode & REENTRY_CHAIN) ? 4 : 1));
    CHECK(selected->length == 2 && selected->pins == 0 && header(selected)->refs == 1);
    check_string(lookup(selected, reentry_key.low, key_len), "final", 5);
    struct entry *other = lookup(selected, 90, UINT64_MAX);
    if (mode & REENTRY_CONVERT) {
        CHECK(other->tag == 7);
        CHECK(((struct value *)(uintptr_t)other->value)->tag == 3);
    } else { CHECK(other->tag == 3); }
    uint64_t capture_index = (uint64_t)(lookup(selected, reentry_key.low, key_len) - selected->entries);
    CHECK(((mode & (REENTRY_UNSET | REENTRY_UNSET_SET)) ? selected->tail : selected->head) == capture_index);
    fixture_release(selected);
    current = selected = NULL;
    if (snapshot) {
        struct entry *copy = lookup(snapshot, reentry_key.low, key_len);
        struct value *box = (void *)(uintptr_t)copy->value;
        void *receiver = (void *)(uintptr_t)box->low;
        CHECK(copy->tag == 7 && box->tag == 6 && is_live(receiver));
        CHECK(header(receiver)->refs == 1 && (header(receiver)->kind & UINT64_C(0x4000)));
        CHECK(*(uint64_t *)receiver == 3);
        fixture_release(snapshot);
        snapshot = NULL;
        CHECK(destroyed == before + 1);
    }
    CHECK(live == 0 && pending_callback == 0 && hash_write_guard_top == NULL);
}

/* Indexed promotion preserves terminal-cell aliases and owns surviving callable payloads. */
static void indexed_destinations(void) {
    const uint64_t tags[] = {10, 11};
    for (size_t mode = 0; mode < 2; ++mode) {
        CHECK(live == 0);
        struct indexed *array = fixture_allocate(sizeof(*array));
        *array = (struct indexed){.length = 3, .capacity = 3, .stride = mode ? 16 : 8};
        header(array)->kind = HEAP_MAGIC | 2 | (tags[mode] << 8);
        uint64_t float_bits = UINT64_C(0x400c000000000000);
        if (mode) {
            array->words[0] = 41; array->words[1] = 0;
            array->words[2] = float_bits; array->words[3] = 2;
            array->words[4] = 0; array->words[5] = 8;
        } else {
            for (size_t i = 0; i < 3; ++i) {
                array->words[i] = (uintptr_t)fixture_persist("descriptor", 10);
            }
        }
        struct value *terminal = box_owned(4, (uintptr_t)array, 0);
        struct value *reference = box_owned(7, (uintptr_t)terminal, 1);
        fixture_retain(terminal);
        fixture_retain(array);
        CHECK(store_reference(reference, 0, "capture", 7) == 1);
        CHECK(terminal->tag == 4 && terminal->low == (uintptr_t)array);
        fixture_release(array);
        CHECK(store_reference(reference, 0, "capture", 7) == 0);
        CHECK(terminal->tag == 5 && terminal->high == 0 && header(terminal)->refs == 2);
        CHECK(!is_live(array));
        struct hash *hash = (void *)(uintptr_t)terminal->low;
        CHECK(hash->length == 3 && hash->value_type == 7 && hash->pins == 0);
        check_string(lookup(hash, 0, UINT64_MAX), "capture", 7);
        struct entry *second = lookup(hash, 1, UINT64_MAX);
        struct entry *third = lookup(hash, 2, UINT64_MAX);
        if (mode) {
            CHECK(second->tag == 2 && second->value == float_bits && second->high == 0);
            CHECK(third->tag == 8 && third->value == 0 && third->high == 0);
        } else {
            CHECK(second->tag == 10 && third->tag == 10);
            CHECK(header((void *)(uintptr_t)second->value)->refs == 1);
            CHECK(header((void *)(uintptr_t)third->value)->refs == 1);
        }
        fixture_release(reference);
        CHECK(header(terminal)->refs == 1);
        fixture_release(terminal);
        CHECK(live == 0 && pending_callback == 0 && hash_write_guard_top == NULL);
    }
}

int main(void) {
    indexed_destinations();
    guards();
    owned_conversion();
    basic();
    invalid_reference();
    for (int by_reference = 0; by_reference < 2; ++by_reference) {
        retarget(RETARGET, by_reference);
        retarget(RETARGET | THROW, by_reference);
        retarget(RETARGET | GROW | THROW, by_reference);
        retarget(RETARGET | GROW | THROW | DROP_ALIAS, by_reference);
    }
    CHECK(destroyed == 8 && live == 0);
    const unsigned modes[] = {REENTRY_STRING, REENTRY_FALSE, REENTRY_UNSET,
        REENTRY_UNSET_SET, REENTRY_TWO_SETS, REENTRY_CAPTURE, REENTRY_CHAIN,
        REENTRY_CONVERT, REENTRY_CONVERT | REENTRY_STRING, REENTRY_CONVERT | REENTRY_FALSE,
        REENTRY_CONVERT | REENTRY_UNSET, REENTRY_CONVERT | REENTRY_UNSET_SET,
        REENTRY_CONVERT | REENTRY_TWO_SETS, REENTRY_CONVERT | REENTRY_CAPTURE,
        REENTRY_CONVERT | REENTRY_CHAIN, REENTRY_COPY,
        REENTRY_COPY | REENTRY_DROP_COPY, REENTRY_COPY | REENTRY_STRING,
        REENTRY_COPY | REENTRY_UNSET, REENTRY_COPY | REENTRY_CAPTURE,
        REENTRY_COPY | REENTRY_CONVERT, REENTRY_COPY | REENTRY_CONVERT | REENTRY_STRING,
        REENTRY_COPY | REENTRY_CONVERT | REENTRY_UNSET,
        REENTRY_COPY | REENTRY_CONVERT | REENTRY_CAPTURE};
    for (size_t i = 0; i < sizeof(modes) / sizeof(*modes); ++i) {
        for (int string_key = 0; string_key < 2; ++string_key) {
            for (unsigned box_depth = 0; box_depth < 3; ++box_depth) {
                reentry(modes[i], string_key, box_depth);
                reentry(modes[i] | THROW, string_key, box_depth);
            }
        }
    }
    CHECK(live == 0 && hash_write_guard_top == NULL);
    return 0;
}
