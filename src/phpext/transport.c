/* Private, malloc-owned transport. Real Zend arrays are traversed only in host mode. */
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

typedef struct ep_allocation {
    struct ep_allocation *next;
    max_align_t alignment;
    unsigned char bytes[];
} ep_allocation;

typedef struct ep_call {
    ep_allocation *allocations;
    zval *result;
    const char *error;
    char *exception_name;
    size_t exception_name_len;
    char *exception_message;
    size_t exception_message_len;
    zend_long exception_code;
    char *output;
    size_t output_len;
    size_t output_capacity;
    uintptr_t stack_floor;
    zend_fiber_context *fiber_context;
    unsigned char *snapshot;
    unsigned char *checkpoint;
    bool host_exception;
    bool host_bailout;
    zend_string *warning;
    zval pending_exception;
    const HashTable *ancestors[64];
} ep_call;

/* Allocate independently of emalloc: native code cannot cross a Zend bailout frame. */
static void *ep_alloc(ep_call *call, size_t size)
{
    if (size > SIZE_MAX - offsetof(ep_allocation, bytes)) {
        call->error = "PHP extension transport allocation overflow";
        return NULL;
    }
    ep_allocation *allocation = calloc(1, offsetof(ep_allocation, bytes) + size);
    if (!allocation) {
        call->error = "PHP extension transport allocation failed";
        return NULL;
    }
    allocation->next = call->allocations;
    call->allocations = allocation;
    return allocation->bytes;
}

/* Retire every partial input/result copy after normal returns, exceptions and bailouts. */
static void ep_call_destroy(ep_call *call)
{
    zval pending = call->pending_exception;
    ep_allocation *allocation = call->allocations;
    while (allocation) {
        ep_allocation *next = allocation->next;
        free(allocation);
        allocation = next;
    }
    free(call->output);
    free(call);
    if (!Z_ISUNDEF(pending)) { zval_ptr_dtor(&pending); }
}

/* Copy binary bytes into a transport string, using only the shared zend_string value layout. */
static zend_string *ep_string(ep_call *call, const char *bytes, size_t len)
{
    if (len > SIZE_MAX - offsetof(zend_string, val) - 1) {
        call->error = "PHP extension string length overflow";
        return NULL;
    }
    zend_string *string = ep_alloc(call, offsetof(zend_string, val) + len + 1);
    if (!string) { return NULL; }
    string->gc.refcount = 1;
    string->len = len;
    memcpy(string->val, bytes, len);
    return string;
}

static zval *ep_copy(ep_call *call, const zval *source, bool host, unsigned depth);

/* Preserve insertion order, integer/string keys, holes and the append cursor. */
static HashTable *ep_array(ep_call *call, const HashTable *source, bool host, unsigned depth)
{
    if (depth >= 64) {
        call->error = "PHP extension arrays exceed the maximum nesting depth (64)";
        return NULL;
    }
    for (unsigned i = 0; i < depth; i++) {
        if (call->ancestors[i] == source) {
            call->error = "PHP extension arrays cannot be recursive";
            return NULL;
        }
    }
    call->ancestors[depth] = source;
    HashTable *copy = ep_alloc(call, sizeof(HashTable));
    if (!copy) { return NULL; }
    size_t count = source->nNumOfElements;
    if (count > SIZE_MAX / sizeof(Bucket)) {
        call->error = "PHP extension array size overflow";
        return NULL;
    }
    copy->arData = ep_alloc(call, count * sizeof(Bucket));
    if (!copy->arData) { return NULL; }
    copy->gc.refcount = 1;
    copy->nNumUsed = copy->nNumOfElements = (uint32_t)count;
    copy->nTableSize = source->nTableSize;
    copy->nNextFreeElement = source->nNextFreeElement;
    /* Packed transport still has Bucket stride, unlike real Zend arPacked. */
    bool packed = host ? HT_IS_PACKED(source) : source->nTableMask == (uint32_t)-2;
    packed = packed && source->nNumUsed == count && source->nNextFreeElement == (zend_long)count;
    copy->nTableMask = packed ? (uint32_t)-2 : (uint32_t)-8;
    uint32_t dest = 0;
    for (uint32_t i = 0; i < source->nNumUsed; i++) {
        const zval *value;
        zend_string *key = NULL;
        zend_ulong index;
        if (host && HT_IS_PACKED(source)) {
            value = &source->arPacked[i];
            index = i;
        } else {
            const Bucket *bucket = &source->arData[i];
            value = &bucket->val;
            key = bucket->key;
            index = bucket->h;
        }
        if (Z_TYPE_P(value) == IS_UNDEF) { continue; }
        if (Z_TYPE_P(value) == IS_REFERENCE) {
            call->error = "PHP extension arrays cannot contain references";
            return NULL;
        }
        if (dest >= count) {
            call->error = "Invalid PHP extension transport array";
            return NULL;
        }
        zval *child = ep_copy(call, value, host, depth + 1);
        if (!child) { return NULL; }
        Bucket *bucket = &copy->arData[dest++];
        bucket->val = *child;
        bucket->h = index;
        if (key) {
            bucket->key = ep_string(call, ZSTR_VAL(key), ZSTR_LEN(key));
            if (!bucket->key) { return NULL; }
        }
    }
    if (dest != count) {
        call->error = "Invalid PHP extension transport element count";
        return NULL;
    }
    return copy;
}

/* No borrowed Zend allocation survives this recursively independent copy. */
static zval *ep_copy(ep_call *call, const zval *source, bool host, unsigned depth)
{
    zval *copy = ep_alloc(call, sizeof(zval));
    if (!copy) { return NULL; }
    switch (Z_TYPE_P(source)) {
        case IS_NULL: ZVAL_NULL(copy); break;
        case IS_FALSE: ZVAL_FALSE(copy); break;
        case IS_TRUE: ZVAL_TRUE(copy); break;
        case IS_LONG: ZVAL_LONG(copy, Z_LVAL_P(source)); break;
        case IS_DOUBLE: ZVAL_DOUBLE(copy, Z_DVAL_P(source)); break;
        case IS_STRING: {
            zend_string *string = ep_string(call, Z_STRVAL_P(source), Z_STRLEN_P(source));
            if (!string) { return NULL; }
            ZVAL_STR(copy, string);
            break;
        }
        case IS_ARRAY: {
            HashTable *array = ep_array(call, Z_ARRVAL_P(source), host, depth);
            if (!array) { return NULL; }
            ZVAL_ARR(copy, array);
            break;
        }
        default:
            call->error = "PHP extension boundary supports only int, float, bool, string, array and null";
            return NULL;
    }
    return copy;
}

/* This phase may allocate or bailout in Zend, after the native lock was released. */
static void ep_to_zend(const zval *source, zval *destination)
{
    switch (Z_TYPE_P(source)) {
        case IS_NULL: ZVAL_NULL(destination); break;
        case IS_FALSE: ZVAL_FALSE(destination); break;
        case IS_TRUE: ZVAL_TRUE(destination); break;
        case IS_LONG: ZVAL_LONG(destination, Z_LVAL_P(source)); break;
        case IS_DOUBLE: ZVAL_DOUBLE(destination, Z_DVAL_P(source)); break;
        case IS_STRING: ZVAL_STRINGL(destination, Z_STRVAL_P(source), Z_STRLEN_P(source)); break;
        case IS_ARRAY: {
            HashTable *array = Z_ARRVAL_P(source);
            array_init_size(destination, array->nNumOfElements);
            for (uint32_t i = 0; i < array->nNumUsed; i++) {
                const Bucket *bucket = &array->arData[i];
                zval *slot;
                zval empty;
                ZVAL_NULL(&empty);
                if (bucket->key) {
                    zend_string *key = zend_string_init(ZSTR_VAL(bucket->key), ZSTR_LEN(bucket->key), false);
                    slot = zend_hash_add_new(Z_ARRVAL_P(destination), key, &empty);
                    zend_string_release(key);
                } else {
                    slot = zend_hash_index_add_new(Z_ARRVAL_P(destination), bucket->h, &empty);
                }
                ZEND_ASSERT(slot != NULL);
                ep_to_zend(&bucket->val, slot);
            }
            Z_ARRVAL_P(destination)->nNextFreeElement = array->nNextFreeElement;
            break;
        }
        default: ZEND_UNREACHABLE();
    }
}
