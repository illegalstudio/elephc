/* Request-owned pointer indexes use malloc, including while the native image is active. */
typedef struct ep_object_index_entry {
    uintptr_t key;
    ep_object_lease *lease;
} ep_object_index_entry;

typedef struct ep_object_index {
    ep_object_index_entry *entries;
    size_t capacity;
    size_t count;
    size_t used;
} ep_object_index;

/* Native/Zend allocation pointers are aligned, so zero and one are reserved slot markers. */
static size_t ep_object_index_hash(uintptr_t key)
{
    key ^= key >> 30;
    key *= UINT64_C(0xbf58476d1ce4e5b9);
    key ^= key >> 27;
    key *= UINT64_C(0x94d049bb133111eb);
    return (size_t)(key ^ (key >> 31));
}

static ep_object_lease *ep_object_index_find(const ep_object_index *index, uintptr_t key)
{
    if (!index->capacity) { return NULL; }
    size_t slot = ep_object_index_hash(key) & (index->capacity - 1);
    for (;;) {
        const ep_object_index_entry *entry = &index->entries[slot];
        if (!entry->key) { return NULL; }
        if (entry->key == key) { return entry->lease; }
        slot = (slot + 1) & (index->capacity - 1);
    }
}

/* Rehashing with unchanged capacity also retires accumulated tombstones during object churn. */
static bool ep_object_index_reserve(ep_call *call, ep_object_index *index)
{
    if (index->capacity && index->used + 1 <= index->capacity / 2) { return true; }
    size_t capacity = index->capacity ? index->capacity : 16;
    if (index->count + 1 > capacity / 4) {
        if (capacity > SIZE_MAX / 2) { call->error = "PHP object identity index overflow"; return false; }
        capacity *= 2;
    }
    if (capacity > SIZE_MAX / sizeof(ep_object_index_entry)) {
        call->error = "PHP object identity index size overflow";
        return false;
    }
    ep_object_index_entry *entries = calloc(capacity, sizeof(*entries));
    if (!entries) { call->error = "PHP object identity index allocation failed"; return false; }
    for (size_t i = 0; i < index->capacity; i++) {
        ep_object_index_entry entry = index->entries[i];
        if (entry.key <= 1) { continue; }
        size_t slot = ep_object_index_hash(entry.key) & (capacity - 1);
        while (entries[slot].key) { slot = (slot + 1) & (capacity - 1); }
        entries[slot] = entry;
    }
    free(index->entries);
    index->entries = entries;
    index->capacity = capacity;
    index->used = index->count;
    return true;
}

static bool ep_object_index_put(ep_call *call, ep_object_index *index,
    uintptr_t key, ep_object_lease *lease)
{
    if (!ep_object_index_reserve(call, index)) { return false; }
    size_t slot = ep_object_index_hash(key) & (index->capacity - 1);
    size_t tombstone = SIZE_MAX;
    for (;;) {
        ep_object_index_entry *entry = &index->entries[slot];
        if (entry->key == key) { entry->lease = lease; return true; }
        if (entry->key == 1 && tombstone == SIZE_MAX) { tombstone = slot; }
        if (!entry->key) {
            if (tombstone != SIZE_MAX) { entry = &index->entries[tombstone]; }
            else { index->used++; }
            entry->key = key;
            entry->lease = lease;
            index->count++;
            return true;
        }
        slot = (slot + 1) & (index->capacity - 1);
    }
}

static void ep_object_index_remove(ep_object_index *index, uintptr_t key)
{
    if (!index->capacity) { return; }
    size_t slot = ep_object_index_hash(key) & (index->capacity - 1);
    for (;;) {
        ep_object_index_entry *entry = &index->entries[slot];
        if (!entry->key) { return; }
        if (entry->key == key) {
            entry->key = 1;
            entry->lease = NULL;
            index->count--;
            return;
        }
        slot = (slot + 1) & (index->capacity - 1);
    }
}

/* Updating an existing collision chain must remain allocation-free during retirement. */
static void ep_object_index_replace(ep_object_index *index, uintptr_t key, ep_object_lease *lease)
{
    size_t slot = ep_object_index_hash(key) & (index->capacity - 1);
    while (index->entries[slot].key != key) { slot = (slot + 1) & (index->capacity - 1); }
    index->entries[slot].lease = lease;
}
