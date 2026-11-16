#ifndef ELEPHC_PERSISTENT_ARRAYS
#define ELEPHC_PERSISTENT_ARRAYS 1
#include <stdint.h>

typedef struct ep_persistent_array {
    struct ep_persistent_array *next;
    HashTable value;
} ep_persistent_array;

static ep_persistent_array *ep_persistent_arrays;

/* Every child is immutable, so no refcounted element destructor belongs to this table. */
static HashTable *ep_persistent_array_new(uint32_t count)
{
    ep_persistent_array *array = pemalloc(sizeof(*array), true);
    zend_hash_init(&array->value, count, NULL, NULL, true);
    array->next = ep_persistent_arrays;
    ep_persistent_arrays = array;
    return &array->value;
}

/* Zend destroys module metadata before unloading its shared library. MSHUTDOWN is too early:
 * other module shutdown callbacks and the remaining global tables can still read constants. */
static void __attribute__((destructor)) ep_persistent_arrays_unload(void)
{
    while (ep_persistent_arrays) {
        ep_persistent_array *array = ep_persistent_arrays;
        ep_persistent_arrays = array->next;
        GC_DEL_FLAGS(&array->value, IS_ARRAY_IMMUTABLE);
        /* Only the module pool owns this table after Zend metadata has been destroyed. */
        GC_SET_REFCOUNT(&array->value, 1);
        zend_hash_destroy(&array->value);
        pefree(array, true);
    }
}
#endif
