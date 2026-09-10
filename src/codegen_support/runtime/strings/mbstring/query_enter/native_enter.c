/* Nested query selection shares the independent capture allocator and reference conventions. */
#define main capture_fixture_main
#include "capture_fixture.c"
#undef main

extern int query_enter(void *, struct hash *, const struct value *, struct hash **) __asm__("__rt_mbstring_query_hash_enter");
extern struct hash *pin_hash(struct hash *) __asm__("__rt_hash_pin");
extern void unpin_hash(struct hash *) __asm__("__rt_hash_unpin");
static size_t enter_destructors;
static unsigned enter_mode;
enum { ENTER_REPLACE = 16 };

/* Preserve dense payload owners while promotion creates an independently mutable hash. */
static struct indexed *source_indexed(void) {
    struct indexed *array = fixture_allocate(sizeof(*array));
    *array = (struct indexed){.length = 2, .capacity = 2, .stride = 16};
    header(array)->kind = HEAP_MAGIC | 2 | (1 << 8);
    array->words[0] = (uintptr_t)fixture_persist("first", 5);
    array->words[1] = 5;
    array->words[2] = (uintptr_t)fixture_persist("last", 4);
    array->words[3] = 4;
    return array;
}

/* Retain deleted-key history so child separation cannot recompute a counter from current entries. */
static struct hash *source_hash(void) {
    struct hash *hash = hash_new(4, 7);
    CHECK(insert(hash, -3, UINT64_MAX, (uintptr_t)fixture_persist("first", 5), 5, 1) == hash);
    CHECK(insert(hash, 4, UINT64_MAX, (uintptr_t)fixture_persist("last", 4), 4, 1) == hash);
    CHECK(insert(hash, 99, UINT64_MAX, 0, 0, 3) == hash);
    CHECK(remove_key(hash, 99, UINT64_MAX) == hash && hash->next_index == 100);
    return hash;
}

/* Existing arrays separate on any shared ordinary box, but lifetime pins do not force COW. */
static void array_entry(unsigned depth, unsigned shared_at, int shared_array, int indexed,
                        int persistent, int root_copy, int pinned) {
    CHECK(live == 0 && hash_write_guard_top == NULL);
    allocated = 0;
    destructor_override = NULL;
    struct hash *root = hash_new(4, 7);
    struct hash *root_alias = root_copy ? fixture_retain(root) : NULL;
    void *old = indexed ? (void *)source_indexed() : (void *)source_hash();
    void *owners[4];
    size_t owner_count = 0;
    if (shared_array) { owners[owner_count++] = fixture_retain(old); }
    if (pinned) { CHECK(!indexed); pin_hash(old); }
    struct value value = {indexed ? 4 : 5, (uintptr_t)old, 0};
    for (unsigned i = 0; i < depth; ++i) {
        struct value *box = box_owned(value.tag, value.low, value.high);
        if (shared_at == i + 1) { owners[owner_count++] = fixture_retain(box); }
        value = (struct value){7, (uintptr_t)box, 0};
    }
    struct value *reference = NULL;
    if (persistent) {
        struct value *terminal = box_owned(value.tag, value.low, value.high);
        reference = box_owned(7, (uintptr_t)terminal, 1);
        owners[owner_count++] = fixture_retain(reference);
        value = (struct value){7, (uintptr_t)reference, 0};
    }
    struct value key = {1, (uintptr_t)"child\0key", 9};
    CHECK(insert(root, (uintptr_t)fixture_persist("child\0key", 9), 9,
                 value.low, value.high, value.tag) == root);
    struct hash *child = NULL;
    CHECK(query_enter(NULL, root, &key, &child) == 0);
    CHECK(child != NULL && child->pins == 1 + (uint64_t)(child == old && pinned) && root->pins == 0);
    int copied = indexed || persistent || shared_array || shared_at;
    CHECK((child != old) == copied);
    CHECK(child->length == (persistent ? 0 : 2));
    CHECK(child->next_index == (persistent ? INT64_MIN : indexed ? 2 : 100));
    if (root_alias) { CHECK(root_alias == root && root_alias->length == 1); }
    if (!persistent) {
        check_string(lookup(child, indexed ? 0 : (uint64_t)-3, UINT64_MAX), "first", 5);
        check_string(lookup(child, indexed ? 1 : 4, UINT64_MAX), "last", 4);
    }
    CHECK(store(child, 500, "written", 7) == 0 && child->pins == 1 + (uint64_t)(child == old && pinned));
    if (copied && !indexed && is_live(old)) {
        CHECK(((struct hash *)old)->length == 2 && lookup(old, 500, UINT64_MAX) == NULL);
    }
    if (reference) { CHECK(reference->tag == 7 && reference->high == 1); }
    unpin_hash(child);
    if (pinned) { unpin_hash(old); }
    fixture_release(root);
    if (root_alias) { fixture_release(root_alias); }
    while (owner_count) { fixture_release(owners[--owner_count]); }
    CHECK(live == 0 && hash_write_guard_top == NULL);
}

/* A replacement destructor still sees its original entry until guarded construction publishes. */
static void enter_destructor(void *value) {
    (void)value;
    ++enter_destructors;
    CHECK(selected->pins == 2 && hash_write_guard_top != NULL);
    CHECK(lookup(selected, 0, UINT64_MAX) != NULL);
    if (enter_mode & GROW) {
        struct entry *old_entries = selected->entries;
        for (uint64_t i = 100; i < 150; ++i) { CHECK(store(selected, i, NULL, 0) == 0); }
        CHECK(selected->entries != old_entries);
    }
    if (enter_mode & ENTER_REPLACE) { CHECK(store(selected, 0, "inside", 6) == 0); }
    if (enter_mode & RETARGET) {
        current = hash_new(4, 7);
        fixture_release(selected);
    }
    if ((enter_mode & DROP_ALIAS) && alias) {
        fixture_release(alias);
        alias = NULL;
    }
    if (enter_mode & THROW) { pending_callback = 1; }
}

/* Cursor protection survives loss of every PHP parent owner, including a pending old destructor. */
static void replaced_value(unsigned mode, int copied, int boxed) {
    CHECK(live == 0 && hash_write_guard_top == NULL);
    allocated = 0;
    current = selected = hash_new(4, 7);
    alias = copied ? fixture_retain(selected) : NULL;
    enter_mode = mode;
    enter_destructors = 0;
    destructor_override = enter_destructor;
    struct value value = {6, (uintptr_t)object(0), 0};
    if (boxed) { value = (struct value){7, (uintptr_t)box_owned(6, value.low, 0), 0}; }
    CHECK(insert(selected, 0, UINT64_MAX, value.low, value.high, value.tag) == selected);
    struct value key = {0, 0, 0};
    struct hash *child = NULL;
    CHECK(query_enter(NULL, selected, &key, &child) == ((mode & THROW) ? 2 : 0));
    CHECK(child != NULL && child->length == 0 && child->pins == 1 && enter_destructors == 1);
    CHECK(hash_write_guard_top == NULL);
    if (is_live(selected)) {
        struct entry *entry = lookup(selected, 0, UINT64_MAX);
        CHECK(selected->pins == 0 && entry != NULL && entry->tag == 5 && entry->value == (uintptr_t)child);
    }
    if (mode & RETARGET) { CHECK(current != selected && current->length == 0); }
    CHECK(store(child, 3, "cursor", 6) == 0 && child->pins == 1);
    unpin_hash(child);
    fixture_release(current);
    if (alias) { fixture_release(alias); alias = NULL; }
    CHECK(live == 0 && pending_callback == 0 && hash_write_guard_top == NULL);
}

/* An enclosing construction callback already owns the dying scalar, even during nested Enter. */
static void enter_from_active_release(void *value) {
    (void)value;
    ++enter_destructors;
    CHECK(selected->pins == 1 && hash_write_guard_top != NULL);
    struct value key = {0, 0, 0};
    struct hash *child = NULL;
    CHECK(query_enter(NULL, selected, &key, &child) == 0);
    CHECK(child->pins == 1 && selected->pins == 1);
    CHECK(store(child, 0, "nested", 6) == 0);
    unpin_hash(child);
    if (enter_mode & THROW) { pending_callback = 1; }
}

/* The outer replacement retains ownership of its final value after a nested query cursor finishes. */
static void during_capture(unsigned mode) {
    CHECK(live == 0);
    allocated = 0;
    current = selected = hash_new(4, 7);
    alias = fixture_retain(selected);
    enter_mode = mode;
    enter_destructors = 0;
    destructor_override = enter_from_active_release;
    CHECK(insert(selected, 0, UINT64_MAX, (uintptr_t)object(0), 0, 6) == selected);
    CHECK(store(selected, 0, "final", 5) == ((mode & THROW) ? 2 : 0));
    CHECK(enter_destructors == 1 && selected->pins == 0 && alias == selected);
    check_string(lookup(selected, 0, UINT64_MAX), "final", 5);
    fixture_release(current);
    fixture_release(alias);
    alias = NULL;
    CHECK(live == 0 && pending_callback == 0 && hash_write_guard_top == NULL);
}

/* Missing keys and scalar values all become one owned empty hash plus the returned cursor pin. */
static void scalar_entries(void) {
    for (int tag = -1; tag <= 8; ++tag) {
        if (tag == 4 || tag == 5 || tag == 6 || tag == 7) { continue; }
        CHECK(live == 0);
        allocated = 0;
        struct hash *root = hash_new(4, 7);
        uint64_t low = tag == 1 ? (uintptr_t)fixture_persist("old", 3) : 42;
        if (tag >= 0) { CHECK(insert(root, 8, UINT64_MAX, low, tag == 1 ? 3 : 0, tag) == root); }
        struct value key = {0, 8, 0};
        struct hash *child = NULL;
        CHECK(query_enter(NULL, root, &key, &child) == 0);
        CHECK(child->length == 0 && child->pins == 1 && header(child)->refs == 2);
        CHECK(root->length == 1 && root->next_index == 9 && root->pins == 0);
        unpin_hash(child);
        fixture_release(root);
        CHECK(live == 0);
    }
}

/* A protected release owns the old slot even when its array or wrapper still has one physical owner. */
static void borrowed_array(int boxed, int pinned) {
    CHECK(live == 0 && hash_write_guard_top == NULL);
    allocated = 0;
    struct hash *root = hash_new(4, 7);
    struct hash *old = source_hash();
    void *old_owner = boxed ? (void *)box_owned(5, (uintptr_t)old, 0) : (void *)old;
    CHECK(insert(root, 0, UINT64_MAX, (uintptr_t)old_owner, 0, boxed ? 7 : 5) == root);
    if (pinned) { pin_hash(old); }
    struct guard scope;
    guard_push(&scope, root, 0, UINT64_MAX);
    CHECK(guard_owns(root, 0, UINT64_MAX) == 0);
    struct hash *child = NULL;
    struct value key = {0, 0, 0};
    CHECK(query_enter(NULL, root, &key, &child) == 0);
    CHECK(child != old && child->pins == 1 && child->length == 2 && child->next_index == 100);
    CHECK(guard_pop(&scope) == 1 && hash_write_guard_top == NULL);
    fixture_release(old_owner);
    if (pinned) { unpin_hash(old); }
    CHECK(store(child, 500, "survives", 8) == 0);
    unpin_hash(child);
    fixture_release(root);
    CHECK(live == 0);
}

int main(void) {
    for (unsigned depth = 0; depth < 3; ++depth) {
        for (unsigned shared_at = 0; shared_at <= depth; ++shared_at) {
            for (int shared_array = 0; shared_array < 2; ++shared_array) {
                for (int indexed = 0; indexed < 2; ++indexed) {
                    for (int persistent = 0; persistent < 2; ++persistent) {
                        for (int copied = 0; copied < 2; ++copied) {
                            for (int pinned = 0; pinned < (indexed ? 1 : 2); ++pinned) {
                                array_entry(depth, shared_at, shared_array, indexed, persistent, copied, pinned);
                            }
                        }
                    }
                }
            }
        }
    }
    unsigned modes[] = {0, GROW, ENTER_REPLACE, GROW | ENTER_REPLACE, RETARGET,
        RETARGET | DROP_ALIAS, RETARGET | DROP_ALIAS | GROW | ENTER_REPLACE};
    for (size_t i = 0; i < sizeof(modes) / sizeof(*modes); ++i) {
        for (int copied = 0; copied < 2; ++copied) {
            for (int boxed = 0; boxed < 2; ++boxed) {
                replaced_value(modes[i], copied, boxed);
                replaced_value(modes[i] | THROW, copied, boxed);
            }
        }
    }
    during_capture(0);
    during_capture(THROW);
    scalar_entries();
    borrowed_array(0, 0);
    borrowed_array(0, 1);
    borrowed_array(1, 0);
    borrowed_array(1, 1);
    return 0;
}
