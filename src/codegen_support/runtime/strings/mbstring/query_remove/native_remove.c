/* Query deletion uses the independent capture allocator, with a removal-specific destructor. */
#define main capture_fixture_main
#include "capture_fixture.c"
#undef main

extern int query_remove(void *, struct hash *, const struct value *) __asm__("__rt_mbstring_query_hash_remove");
enum { REMOVE_REINSERT = 1, REMOVE_GROW = 2, REMOVE_RETARGET = 4,
       REMOVE_DROP_ALIAS = 8, REMOVE_THROW = 16, REMOVE_COPY = 32 };
static unsigned remove_mode;
static uint64_t removed_key, removed_key_len, before_length;
static size_t removal_destructors;

/* A destructor sees completed deletion, and any newly inserted key must survive the outer call. */
static void after_removal(void *value) {
    (void)value;
    ++removal_destructors;
    CHECK(selected->pins == 1);
    CHECK(lookup(selected, removed_key, removed_key_len) == NULL);
    CHECK(selected->length == before_length - 1);
    if (remove_mode & REMOVE_REINSERT) {
        struct value replacement = {1, (uintptr_t)"replacement", 11};
        CHECK(capture_store(NULL, selected, &reentry_key, &replacement) == 0);
    }
    if (remove_mode & REMOVE_GROW) {
        struct entry *old_entries = selected->entries;
        for (uint64_t i = 100; i < 150; ++i) { CHECK(store(selected, i, NULL, 0) == 0); }
        CHECK(old_entries != selected->entries);
    }
    if (remove_mode & REMOVE_COPY) {
        snapshot = unique(fixture_retain(selected));
        CHECK(snapshot != selected && snapshot->pins == 0);
        CHECK(snapshot->length == selected->length && snapshot->next_index == selected->next_index);
    }
    if (remove_mode & REMOVE_RETARGET) {
        current = hash_new(4, 7);
        CHECK(store(current, 77, "retargeted", 10) == 0);
        fixture_release(selected);
    }
    if ((remove_mode & REMOVE_DROP_ALIAS) && alias) {
        fixture_release(alias);
        alias = NULL;
    }
    if (remove_mode & REMOVE_THROW) { pending_callback = 1; }
}

/* Walk both directions after arbitrary slot removal and callback-triggered growth. */
static void check_links(struct hash *hash) {
    uint64_t cursor = hash->head, previous = UINT64_MAX, count = 0;
    while (cursor != UINT64_MAX) {
        CHECK(cursor < hash->capacity && count < hash->length);
        struct entry *entry = &hash->entries[cursor];
        CHECK(entry->occupied == 1 && entry->previous == previous);
        previous = cursor;
        cursor = entry->next;
        ++count;
    }
    CHECK(count == hash->length && previous == hash->tail);
    uint64_t next = UINT64_MAX;
    cursor = hash->tail;
    while (cursor != UINT64_MAX) {
        CHECK(count > 0 && cursor < hash->capacity);
        struct entry *entry = &hash->entries[cursor];
        CHECK(entry->occupied == 1 && entry->next == next);
        next = cursor;
        cursor = entry->previous;
        --count;
    }
    CHECK(count == 0 && next == hash->head);
}

/* Raw objects, ordinary Mixed boxes, and nested arrays all retire after the root disappears. */
static void removal(unsigned mode, int copied, unsigned position, unsigned wrapping, int named) {
    CHECK(live == 0 && hash_write_guard_top == NULL);
    allocated = 0;
    current = selected = hash_new(4, 7);
    alias = copied ? fixture_retain(selected) : NULL;
    snapshot = NULL;
    remove_mode = mode;
    removal_destructors = 0;
    destructor_override = after_removal;
    removed_key = named ? (uintptr_t)"r\0ot" : INT64_MAX;
    removed_key_len = named ? 4 : UINT64_MAX;
    reentry_key = (struct value){named ? 1 : 0, removed_key, named ? 4 : 0};
    struct value payload = {6, (uintptr_t)object(0), 0};
    if (wrapping == 1) {
        payload = (struct value){7, (uintptr_t)box_owned(6, payload.low, 0), 0};
    } else if (wrapping == 2) {
        struct hash *nested = hash_new(4, 7);
        CHECK(set(nested, 1, UINT64_MAX, payload.low, 0, 6) == nested);
        payload = (struct value){5, (uintptr_t)nested, 0};
    }
    /* Seed through internal stores so an exposed root copy retains the same hash identity. */
    for (unsigned i = 0; i < 3; ++i) {
        if (i == position) {
            uint64_t key = removed_key;
            if (named) { key = (uintptr_t)fixture_persist((void *)(uintptr_t)key, 4); }
            CHECK(insert(selected, key, removed_key_len, payload.low, payload.high, payload.tag) == selected);
        } else { CHECK(insert(selected, i + 10, UINT64_MAX, i, 0, 0) == selected); }
    }
    before_length = selected->length;
    int64_t history = selected->next_index;
    int status = query_remove(NULL, selected, &reentry_key);
    CHECK(status == ((mode & REMOVE_THROW) ? 2 : 0));
    CHECK(removal_destructors == 1 && hash_write_guard_top == NULL);
    if (is_live(selected)) {
        CHECK(selected->pins == 0);
        check_links(selected);
        if (mode & REMOVE_REINSERT) { check_string(lookup(selected, removed_key, removed_key_len), "replacement", 11); }
        else { CHECK(lookup(selected, removed_key, removed_key_len) == NULL); }
        if (!named || !(mode & REMOVE_GROW)) { CHECK(selected->next_index == history); }
        if (alias) { CHECK(alias == selected); }
    }
    if (mode & REMOVE_RETARGET) { check_string(lookup(current, 77, UINT64_MAX), "retargeted", 10); }
    fixture_release(current);
    if (alias) { fixture_release(alias); alias = NULL; }
    if (snapshot) { fixture_release(snapshot); snapshot = NULL; }
    CHECK(live == 0 && hash_write_guard_top == NULL && pending_callback == 0);
}

/* An active construction release owns the dying value even when query deletion removes its slot. */
static void remove_from_active_release(void *value) {
    (void)value;
    ++removal_destructors;
    CHECK(selected->pins == 1 && hash_write_guard_top != NULL);
    CHECK(query_remove(NULL, selected, &reentry_key) == 0);
    CHECK(selected->pins == 1 && lookup(selected, reentry_key.low, UINT64_MAX) == NULL);
    if (remove_mode & REMOVE_THROW) { pending_callback = 1; }
}

/* Capture construction must complete its later replacement without double-releasing the deleted owner. */
static void removal_during_capture(unsigned mode) {
    CHECK(live == 0);
    allocated = 0;
    current = selected = hash_new(4, 7);
    alias = fixture_retain(selected);
    remove_mode = mode;
    removal_destructors = 0;
    reentry_key = (struct value){0, 0, 0};
    destructor_override = remove_from_active_release;
    CHECK(insert(selected, 0, UINT64_MAX, (uintptr_t)object(0), 0, 6) == selected);
    struct value replacement = {1, (uintptr_t)"final", 5};
    CHECK(capture_store(NULL, selected, &reentry_key, &replacement) == ((mode & REMOVE_THROW) ? 2 : 0));
    CHECK(removal_destructors == 1 && selected == alias && selected->pins == 0);
    check_string(lookup(selected, 0, UINT64_MAX), "final", 5);
    check_links(selected);
    fixture_release(current);
    fixture_release(alias);
    alias = NULL;
    CHECK(live == 0 && hash_write_guard_top == NULL && pending_callback == 0);
}

/* Missing and scalar-valued roots need no destructor and preserve history and alias identity. */
static void scalar_removal(void) {
    allocated = 0;
    struct hash *hash = hash_new(4, 7);
    struct value key = {0, 12, 0};
    CHECK(query_remove(NULL, hash, &key) == 0 && hash->length == 0 && hash->pins == 0);
    CHECK(insert(hash, 12, UINT64_MAX, 0, 0, 3) == hash);
    CHECK(query_remove(NULL, hash, &key) == 0 && hash->length == 0 && hash->next_index == 13);
    check_links(hash);
    CHECK(insert(hash, 12, UINT64_MAX, (uintptr_t)fixture_persist("bytes", 5), 5, 1) == hash);
    CHECK(query_remove(NULL, hash, &key) == 0 && hash->length == 0 && hash->next_index == 13);
    fixture_release(hash);
    CHECK(live == 0);
}

int main(void) {
    unsigned modes[] = {0, REMOVE_REINSERT, REMOVE_GROW, REMOVE_COPY,
        REMOVE_REINSERT | REMOVE_GROW | REMOVE_COPY, REMOVE_RETARGET,
        REMOVE_RETARGET | REMOVE_DROP_ALIAS, REMOVE_RETARGET | REMOVE_DROP_ALIAS | REMOVE_COPY};
    for (size_t i = 0; i < sizeof(modes) / sizeof(*modes); ++i) {
        for (int copied = 0; copied < 2; ++copied) {
            for (unsigned position = 0; position < 3; ++position) {
                for (unsigned wrapping = 0; wrapping < 3; ++wrapping) {
                    for (int named = 0; named < 2; ++named) {
                        removal(modes[i], copied, position, wrapping, named);
                        removal(modes[i] | REMOVE_THROW, copied, position, wrapping, named);
                    }
                }
            }
        }
    }
    removal_during_capture(0);
    removal_during_capture(REMOVE_THROW);
    scalar_removal();
    return 0;
}
